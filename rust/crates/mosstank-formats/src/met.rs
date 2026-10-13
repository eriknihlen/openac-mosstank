//! VTank meta files (`.met`).
//!
//! Written from the file format (the fixture metas and the C# tests'
//! inputs and outputs), not from the C# reader; see `NOTICE.md`.
//!
//! A `.met` is a settings-database text (see [`crate::usd`]) with one table,
//! `CondAct`, of five columns: the condition type, the action type, their
//! data and the state. Data cells take the shape the type needs: a plain
//! `i` or `s`, a `k`/`v` table of named values, a `K`/`V` table of nested
//! conditions or actions (`All`, `Any`, `Not`, `DoAll`), or a `ba` blob (an
//! embedded route, a view's XML). The reader expects every line exactly
//! where the writer puts it and names the line when it does not find it.

use mosstank_dotnet::{compat::number::to_string_f64, parse_double, parse_int};
use openac_plugin_compat::text::utf16;

use crate::meta::{
    MetaAction, MetaActionKind, MetaCondition, MetaConditionKind, MetaProfile, MetaRule,
};
use crate::nav;
use crate::route::{NavigationSettings, SpellLookup};
use crate::usd::VtankLineCursor;

const HEADER_LINES: [&str; 13] = [
    "1", "CondAct", "5", "CType", "AType", "CData", "AData", "State", "n", "n", "n", "n", "n",
];

/// The most rules a file may hold.
const MAXIMUM_RULES: i32 = 100_000;
/// The most entries a nested list may hold.
const MAXIMUM_COLLECTION: i32 = 100_000;
/// A blob's length is only refused when negative.
const MAXIMUM_SERIALIZED_LENGTH: i32 = i32::MAX;

/// Reads a meta.
///
/// # Errors
/// Why the text is not a meta, naming the line.
pub fn try_load(text: &str, spells: Option<&dyn SpellLookup>) -> Result<MetaProfile, String> {
    let mut reader = Reader {
        cursor: VtankLineCursor::new(text),
        spells,
    };
    reader.read_profile()
}

struct Reader<'a, 's> {
    cursor: VtankLineCursor<'a>,
    spells: Option<&'s dyn SpellLookup>,
}

impl<'a> Reader<'a, '_> {
    fn error(&self, message: &str) -> String {
        format!("VTank Meta line {}: {message}", self.cursor.line_number())
    }

    fn line(&mut self) -> Result<&'a str, String> {
        let number = self.cursor.line_number();
        self.cursor
            .read_line()
            .map_err(|_| format!("VTank Meta line {number}: Unexpected end of VTank Meta data."))
    }

    fn expect(&mut self, expected: &str) -> Result<(), String> {
        let found = self.line()?;
        if found == expected {
            Ok(())
        } else {
            Err(self.error(&format!("Expected '{expected}', found '{found}'.")))
        }
    }

    fn int(&mut self) -> Result<i32, String> {
        parse_int(self.line()?)
    }

    fn count(&mut self, maximum: i32, what: &str) -> Result<i32, String> {
        let count = parse_int(self.line()?)?;
        if (0..=maximum).contains(&count) {
            Ok(count)
        } else {
            Err(self.error(&format!("Invalid VTank Meta {what}.")))
        }
    }

    fn read_profile(&mut self) -> Result<MetaProfile, String> {
        for expected in HEADER_LINES {
            self.expect(expected)?;
        }
        let count = self.count(MAXIMUM_RULES, "rule count")?;
        let mut profile = MetaProfile::default();
        for _ in 0..count {
            self.expect("i")?;
            let condition_type = self.int()?;
            self.expect("i")?;
            let action_type = self.int()?;
            let condition = self.condition(condition_type)?;
            let action = self.action(action_type)?;
            self.expect("s")?;
            let state = self.line()?.to_owned();
            profile.rules.push(MetaRule::new(&state, condition, action));
        }
        loop {
            let number = self.cursor.line_number();
            let Ok(rest) = self.cursor.read_line() else {
                break;
            };
            if !rest.trim().is_empty() {
                return Err(format!(
                    "VTank Meta line {number}: Unexpected trailing VTank Meta data '{rest}'."
                ));
            }
        }
        Ok(profile)
    }

    /// An `i` cell's value.
    fn int_cell(&mut self) -> Result<i32, String> {
        self.expect("i")?;
        self.int()
    }

    /// An `s` cell's value.
    fn string_cell(&mut self) -> Result<String, String> {
        self.expect("s")?;
        Ok(self.line()?.to_owned())
    }

    /// A `d` cell's value.
    fn double_cell(&mut self) -> Result<f64, String> {
        self.expect("d")?;
        parse_double(self.line()?)
    }

    /// A `ba` cell's characters.
    fn blob_cell(&mut self) -> Result<String, String> {
        self.expect("ba")?;
        let length = self.count(MAXIMUM_SERIALIZED_LENGTH, "serialized length")?;
        let blob = self.cursor.read_blob(length).map_err(|error| error.0)?;
        Ok(blob.to_owned())
    }

    /// The start of a `k`/`v` table with exactly `rows` rows.
    fn named_values(&mut self, rows: usize) -> Result<(), String> {
        for expected in ["TABLE", "2", "k", "v", "n", "n"] {
            self.expect(expected)?;
        }
        self.expect(&rows.to_string())
    }

    /// A `k`/`v` row's key.
    fn key(&mut self, key: &str) -> Result<(), String> {
        self.expect("s")?;
        self.expect(key)
    }

    /// The start of a `K`/`V` list; its row count.
    fn list(&mut self) -> Result<i32, String> {
        for expected in ["TABLE", "2", "K", "V", "n", "n"] {
            self.expect(expected)?;
        }
        self.count(MAXIMUM_COLLECTION, "collection count")
    }

    fn condition(&mut self, condition_type: i32) -> Result<MetaCondition, String> {
        let kind = condition_kind(condition_type)
            .ok_or_else(|| format!("Unknown VTank Meta condition type {condition_type}."))?;
        let mut condition = MetaCondition::of(kind);
        use MetaConditionKind as K;
        match kind {
            K::Never
            | K::Always
            | K::NavigationRouteEmpty
            | K::CharacterDeath
            | K::AnyVendorOpen
            | K::VendorClosed
            | K::NeedToBuff
            | K::PortalspaceEntered
            | K::PortalspaceExited => {
                self.int_cell()?;
            }
            K::PackSlotsLessThanOrEqual
            | K::SecondsInStateGreaterThanOrEqual
            | K::LandblockEquals
            | K::LandcellEquals
            | K::PersistentSecondsInStateGreaterThanOrEqual
            | K::BurdenPercentGreaterThanOrEqual => {
                condition.number = f64::from(self.int_cell()?);
            }
            K::ChatMessage => condition.text = self.string_cell()?,
            K::All | K::Any | K::Not => {
                let rows = self.list()?;
                if kind == K::Not && rows != 1 {
                    return Err(self.error("VTank Meta Not requires exactly one condition."));
                }
                for _ in 0..rows {
                    let child_type = self.int_cell()?;
                    condition.children.push(self.condition(child_type)?);
                }
            }
            K::InventoryItemCountLessThanOrEqual | K::InventoryItemCountGreaterThanOrEqual => {
                self.named_values(2)?;
                self.key("n")?;
                condition.text = self.string_cell()?;
                self.key("c")?;
                condition.number = f64::from(self.int_cell()?);
            }
            K::MonsterNameCountWithinDistance => {
                self.named_values(3)?;
                self.key("n")?;
                condition.text = self.string_cell()?;
                self.key("c")?;
                condition.number = f64::from(self.int_cell()?);
                self.key("r")?;
                condition.secondary_number = self.double_cell()?;
            }
            K::MonsterPriorityCountWithinDistance => {
                self.named_values(3)?;
                self.key("p")?;
                condition.tertiary_number = f64::from(self.int_cell()?);
                self.key("c")?;
                condition.number = f64::from(self.int_cell()?);
                self.key("r")?;
                condition.secondary_number = self.double_cell()?;
            }
            K::NoMonstersWithinDistance => {
                self.named_values(1)?;
                self.key("r")?;
                condition.number = self.double_cell()?;
            }
            K::TimeLeftOnSpellGreaterThanOrEqual => {
                self.named_values(2)?;
                self.key("sid")?;
                condition.number = f64::from(self.int_cell()?);
                self.key("sec")?;
                condition.secondary_number = f64::from(self.int_cell()?);
            }
            K::DistanceFromAnyRoutePointGreaterThanOrEqual => {
                self.named_values(1)?;
                self.key("dist")?;
                condition.number = self.double_cell()?;
            }
            K::Expression => {
                self.named_values(1)?;
                self.key("e")?;
                condition.text = self.string_cell()?;
            }
            K::ChatMessageCapture => {
                self.named_values(2)?;
                self.key("p")?;
                condition.text = self.string_cell()?;
                self.key("c")?;
                condition.secondary_text = self.string_cell()?;
            }
            K::LoginComplete
            | K::Logoff
            | K::PortalTransition
            | K::ItemUseCompleted
            | K::ContainerOpened
            | K::ContainerClosed
            | K::ConfirmationRequested => {
                return Err(format!(
                    "Unknown VTank Meta condition type {condition_type}."
                ));
            }
        }
        Ok(condition)
    }

    fn action(&mut self, action_type: i32) -> Result<MetaAction, String> {
        let kind = action_kind(action_type)
            .ok_or_else(|| format!("Unknown VTank Meta action type {action_type}."))?;
        let mut action = MetaAction::of(kind);
        use MetaActionKind as K;
        match kind {
            K::None | K::ReturnFromCall => {
                self.int_cell()?;
            }
            K::SetMetaState | K::ChatCommand => action.text = self.string_cell()?,
            K::All => {
                let rows = self.list()?;
                for _ in 0..rows {
                    let child_type = self.int_cell()?;
                    action.children.push(self.action(child_type)?);
                }
            }
            K::LoadEmbeddedNavigationRoute => {
                self.expect("ba")?;
                let length = self.count(i32::MAX, "serialized length")?;
                let start = self.cursor.position();
                action.secondary_text = self.line()?.to_owned();
                let count = self.int()?;
                // The route follows only when the blob holds more than its
                // name and node count.
                let consumed = utf16::len(self.cursor.read_since(start));
                let route = if usize::try_from(length).unwrap_or(0) <= consumed {
                    NavigationSettings::default()
                } else {
                    self.expect(nav::NAV_HEADER)?;
                    let route = nav::read_route_body(&mut self.cursor, self.spells)?;
                    if nav::node_count(&route) != count {
                        return Err(
                            self.error("Embedded VTank navigation node counts do not match.")
                        );
                    }
                    route
                };
                action.embedded_route = Some(Box::new(route));
            }
            K::CallMetaState => {
                self.named_values(2)?;
                self.key("st")?;
                action.text = self.string_cell()?;
                self.key("ret")?;
                action.secondary_text = self.string_cell()?;
            }
            K::ExpressionAction | K::ChatExpression => {
                self.named_values(1)?;
                self.key("e")?;
                action.text = self.string_cell()?;
            }
            K::SetWatchdog => {
                self.named_values(3)?;
                self.key("s")?;
                action.text = self.string_cell()?;
                self.key("r")?;
                action.number = self.double_cell()?;
                self.key("t")?;
                action.secondary_number = self.double_cell()?;
            }
            K::ClearWatchdog | K::DestroyAllViews => self.named_values(0)?,
            K::GetVtankOption | K::SetVtankOption => {
                self.named_values(2)?;
                self.key("o")?;
                action.text = self.string_cell()?;
                self.key("v")?;
                action.secondary_text = self.string_cell()?;
            }
            K::CreateView => {
                self.named_values(2)?;
                self.key("n")?;
                action.text = self.string_cell()?;
                self.key("x")?;
                action.secondary_text = self.blob_cell()?;
            }
            K::DestroyView => {
                self.named_values(1)?;
                self.key("n")?;
                action.text = self.string_cell()?;
            }
        }
        Ok(action)
    }
}

/// VTank's condition number for a kind (the edge kinds have none).
#[must_use]
pub fn condition_type(kind: MetaConditionKind) -> Option<i32> {
    match kind {
        MetaConditionKind::ChatMessageCapture => Some(28),
        MetaConditionKind::LoginComplete
        | MetaConditionKind::Logoff
        | MetaConditionKind::PortalTransition
        | MetaConditionKind::ItemUseCompleted
        | MetaConditionKind::ContainerOpened
        | MetaConditionKind::ContainerClosed
        | MetaConditionKind::ConfirmationRequested => None,
        other => MetaConditionKind::ALL
            .iter()
            .position(|candidate| *candidate == other)
            .and_then(|index| i32::try_from(index).ok()),
    }
}

/// The kind VTank's condition number stands for.
#[must_use]
pub fn condition_kind(condition_type: i32) -> Option<MetaConditionKind> {
    match condition_type {
        0..=26 => usize::try_from(condition_type)
            .ok()
            .map(|index| MetaConditionKind::ALL[index]),
        28 => Some(MetaConditionKind::ChatMessageCapture),
        _ => None,
    }
}

/// VTank's action number for a kind.
#[must_use]
pub fn action_type(kind: MetaActionKind) -> i32 {
    MetaActionKind::ALL
        .iter()
        .position(|candidate| *candidate == kind)
        .and_then(|index| i32::try_from(index).ok())
        .expect("every kind is in the table")
}

/// The kind VTank's action number stands for.
#[must_use]
pub fn action_kind(action_type: i32) -> Option<MetaActionKind> {
    usize::try_from(action_type)
        .ok()
        .and_then(|index| MetaActionKind::ALL.get(index).copied())
}

/// Writes a meta.
///
/// # Errors
/// What a `.met` cannot hold: a switched-off rule, a condition only this
/// plugin knows, a fraction in a whole-number field, a line break inside a
/// one-line field, a `Not` without exactly one condition.
pub fn save(profile: &MetaProfile) -> Result<String, String> {
    let disabled = profile.rules.iter().filter(|rule| !rule.enabled).count();
    if disabled > 0 {
        return Err(format!(
            "{disabled} rule(s) are switched off, and a .met file has no switched-off rule."
        ));
    }
    let mut out = String::new();
    for header in HEADER_LINES {
        line(&mut out, header);
    }
    line(&mut out, &profile.rules.len().to_string());
    for rule in &profile.rules {
        let condition_type = condition_type(rule.condition.kind).ok_or_else(|| {
            format!(
                "A .met file has no condition {}; save this meta as .af instead.",
                rule.condition.kind.name()
            )
        })?;
        line(&mut out, "i");
        line(&mut out, &condition_type.to_string());
        line(&mut out, "i");
        line(&mut out, &action_type(rule.action.kind).to_string());
        write_condition(&mut out, &rule.condition)?;
        write_action(&mut out, &rule.action)?;
        string_cell(&mut out, &rule.state, "state name")?;
    }
    Ok(out)
}

fn write_condition(out: &mut String, condition: &MetaCondition) -> Result<(), String> {
    use MetaConditionKind as K;
    match condition.kind {
        K::Never
        | K::Always
        | K::NavigationRouteEmpty
        | K::CharacterDeath
        | K::AnyVendorOpen
        | K::VendorClosed
        | K::NeedToBuff
        | K::PortalspaceEntered
        | K::PortalspaceExited => int_cell(out, 0),
        K::PackSlotsLessThanOrEqual
        | K::SecondsInStateGreaterThanOrEqual
        | K::LandblockEquals
        | K::LandcellEquals
        | K::PersistentSecondsInStateGreaterThanOrEqual
        | K::BurdenPercentGreaterThanOrEqual => int_cell(out, whole(condition.number)?),
        K::ChatMessage => string_cell(out, &condition.text, "chat pattern")?,
        K::All | K::Any => {
            list(out, condition.children.len());
            for child in &condition.children {
                write_child_condition(out, child)?;
            }
        }
        K::Not => {
            if condition.children.len() != 1 {
                return Err("A .met Not holds exactly one condition.".to_owned());
            }
            list(out, 1);
            write_child_condition(out, &condition.children[0])?;
        }
        K::InventoryItemCountLessThanOrEqual | K::InventoryItemCountGreaterThanOrEqual => {
            named_values(out, 2);
            key(out, "n");
            string_cell(out, &condition.text, "item name")?;
            key(out, "c");
            int_cell(out, whole(condition.number)?);
        }
        K::MonsterNameCountWithinDistance => {
            named_values(out, 3);
            key(out, "n");
            string_cell(out, &condition.text, "monster name")?;
            key(out, "c");
            int_cell(out, whole(condition.number)?);
            key(out, "r");
            double_cell(out, condition.secondary_number)?;
        }
        K::MonsterPriorityCountWithinDistance => {
            named_values(out, 3);
            key(out, "p");
            int_cell(out, whole(condition.tertiary_number)?);
            key(out, "c");
            int_cell(out, whole(condition.number)?);
            key(out, "r");
            double_cell(out, condition.secondary_number)?;
        }
        K::NoMonstersWithinDistance => {
            named_values(out, 1);
            key(out, "r");
            double_cell(out, condition.number)?;
        }
        K::TimeLeftOnSpellGreaterThanOrEqual => {
            named_values(out, 2);
            key(out, "sid");
            int_cell(out, whole(condition.number)?);
            key(out, "sec");
            int_cell(out, whole(condition.secondary_number)?);
        }
        K::DistanceFromAnyRoutePointGreaterThanOrEqual => {
            named_values(out, 1);
            key(out, "dist");
            double_cell(out, condition.number)?;
        }
        K::Expression => {
            named_values(out, 1);
            key(out, "e");
            string_cell(out, &condition.text, "expression")?;
        }
        K::ChatMessageCapture => {
            named_values(out, 2);
            key(out, "p");
            string_cell(out, &condition.text, "chat pattern")?;
            key(out, "c");
            string_cell(out, &condition.secondary_text, "colour list")?;
        }
        K::LoginComplete
        | K::Logoff
        | K::PortalTransition
        | K::ItemUseCompleted
        | K::ContainerOpened
        | K::ContainerClosed
        | K::ConfirmationRequested => {
            return Err(format!(
                "A .met file has no condition {}; save this meta as .af instead.",
                condition.kind.name()
            ));
        }
    }
    Ok(())
}

fn write_child_condition(out: &mut String, child: &MetaCondition) -> Result<(), String> {
    let child_type = condition_type(child.kind).ok_or_else(|| {
        format!(
            "A .met file has no condition {}; save this meta as .af instead.",
            child.kind.name()
        )
    })?;
    int_cell(out, child_type);
    write_condition(out, child)
}

fn write_action(out: &mut String, action: &MetaAction) -> Result<(), String> {
    use MetaActionKind as K;
    match action.kind {
        K::None | K::ReturnFromCall => int_cell(out, 0),
        K::SetMetaState => string_cell(out, &action.text, "state name")?,
        K::ChatCommand => string_cell(out, &action.text, "chat command")?,
        K::All => {
            list(out, action.children.len());
            for child in &action.children {
                int_cell(out, action_type(child.kind));
                write_action(out, child)?;
            }
        }
        K::LoadEmbeddedNavigationRoute => {
            let name = single_line(&action.secondary_text, "route name")?;
            let blob = match &action.embedded_route {
                None => format!("{name}\r\n0\r\n"),
                Some(route) => format!(
                    "{name}\r\n{}\r\n{}",
                    nav::node_count(route),
                    nav::save(route)?
                ),
            };
            blob_cell(out, &blob);
        }
        K::CallMetaState => {
            named_values(out, 2);
            key(out, "st");
            string_cell(out, &action.text, "state name")?;
            key(out, "ret");
            string_cell(out, &action.secondary_text, "state name")?;
        }
        K::ExpressionAction | K::ChatExpression => {
            named_values(out, 1);
            key(out, "e");
            string_cell(out, &action.text, "expression")?;
        }
        K::SetWatchdog => {
            named_values(out, 3);
            key(out, "s");
            string_cell(out, &action.text, "state name")?;
            key(out, "r");
            double_cell(out, action.number)?;
            key(out, "t");
            double_cell(out, action.secondary_number)?;
        }
        K::ClearWatchdog | K::DestroyAllViews => named_values(out, 0),
        K::GetVtankOption | K::SetVtankOption => {
            named_values(out, 2);
            key(out, "o");
            string_cell(out, &action.text, "option name")?;
            key(out, "v");
            string_cell(out, &action.secondary_text, "option value")?;
        }
        K::CreateView => {
            named_values(out, 2);
            key(out, "n");
            string_cell(out, &action.text, "view name")?;
            key(out, "x");
            blob_cell(out, &action.secondary_text);
        }
        K::DestroyView => {
            named_values(out, 1);
            key(out, "n");
            string_cell(out, &action.text, "view name")?;
        }
    }
    Ok(())
}

/// A whole number for an `i` field.
fn whole(value: f64) -> Result<i32, String> {
    if value.is_finite()
        && value == value.trunc()
        && value >= f64::from(i32::MIN)
        && value <= f64::from(i32::MAX)
    {
        #[allow(clippy::cast_possible_truncation)]
        Ok(value as i32)
    } else {
        Err(format!(
            "A .met file holds only whole numbers in this field, not {}.",
            to_string_f64(value)
        ))
    }
}

fn single_line<'a>(text: &'a str, what: &str) -> Result<&'a str, String> {
    if text.contains(['\r', '\n']) {
        Err(format!(
            "A .met file cannot hold a {what} that runs over more than one line."
        ))
    } else {
        Ok(text)
    }
}

fn line(out: &mut String, text: &str) {
    out.push_str(text);
    out.push_str("\r\n");
}

fn int_cell(out: &mut String, value: i32) {
    line(out, "i");
    line(out, &value.to_string());
}

fn string_cell(out: &mut String, value: &str, what: &str) -> Result<(), String> {
    let value = single_line(value, what)?;
    line(out, "s");
    line(out, value);
    Ok(())
}

fn double_cell(out: &mut String, value: f64) -> Result<(), String> {
    line(out, "d");
    line(out, &nav::file_number(value)?);
    Ok(())
}

fn blob_cell(out: &mut String, blob: &str) {
    line(out, "ba");
    line(out, &utf16::len(blob).to_string());
    out.push_str(blob);
}

fn named_values(out: &mut String, rows: usize) {
    for header in ["TABLE", "2", "k", "v", "n", "n"] {
        line(out, header);
    }
    line(out, &rows.to_string());
}

fn key(out: &mut String, key: &str) {
    line(out, "s");
    line(out, key);
}

fn list(out: &mut String, rows: usize) {
    for header in ["TABLE", "2", "K", "V", "n", "n"] {
        line(out, header);
    }
    line(out, &rows.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::line_endings::match_existing;
    use crate::route::{RouteJumpDirection, RouteMode, RouteWaypoint, RouteWaypointType};

    fn rule(condition: MetaCondition, action: MetaAction) -> MetaRule {
        MetaRule::new("Default", condition, action)
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankFileWriterTests.cs :: EveryMetFixtureSavesBackToTheSameBytes
    #[test]
    fn every_met_fixture_saves_back_to_the_same_bytes() {
        let mut fixtures = crate::fixtures::list("vtank", "met");
        fixtures.extend(crate::fixtures::list("vt-proof", "met"));
        assert!(fixtures.len() >= 15, "{fixtures:?}");
        for fixture in fixtures {
            let original = crate::fixtures::read(&fixture);
            let profile = try_load(&original, None).unwrap_or_else(|e| panic!("{fixture}: {e}"));
            let saved = match_existing(&save(&profile).unwrap(), Some(&original));
            assert_eq!(saved, original, "{fixture}");
        }
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankFileWriterTests.cs :: EveryConditionAndActionTypeSurvivesAWriteAndARead
    #[test]
    fn every_condition_and_action_type_survives_a_write_and_a_read() {
        let mut jump_route = NavigationSettings::with_mode(RouteMode::Once);
        jump_route.waypoints.push(RouteWaypoint::at(
            RouteWaypointType::Checkpoint,
            1.0,
            2.0,
            3.0,
        ));
        let mut jump = RouteWaypoint::at(RouteWaypointType::Jump, 1.0, 2.0, 3.0);
        jump.jump_heading_degrees = 45.0;
        jump.jump_charge_milliseconds = 300;
        jump.jump_direction = RouteJumpDirection::StrafeRight;
        jump_route.waypoints.push(jump);
        let follow_route = NavigationSettings {
            mode: RouteMode::Target,
            follow_target_name: "Barris".to_owned(),
            follow_target_object_id: 0x5000_0ABC,
            ..NavigationSettings::default()
        };
        let c = |kind| MetaCondition::of(kind);
        let mut profile = MetaProfile::default();
        profile.rules.push(rule(
            MetaCondition {
                kind: MetaConditionKind::All,
                children: vec![
                    c(MetaConditionKind::NeedToBuff),
                    MetaCondition {
                        number: 30.0,
                        ..c(MetaConditionKind::PersistentSecondsInStateGreaterThanOrEqual)
                    },
                    MetaCondition {
                        number: 2081.0,
                        secondary_number: 120.0,
                        ..c(MetaConditionKind::TimeLeftOnSpellGreaterThanOrEqual)
                    },
                    MetaCondition {
                        number: -1_275_133_952.0,
                        ..c(MetaConditionKind::LandblockEquals)
                    },
                    c(MetaConditionKind::PortalspaceEntered),
                    MetaCondition {
                        children: vec![MetaCondition {
                            text: "getcharvital_current[2]<50".to_owned(),
                            ..c(MetaConditionKind::Expression)
                        }],
                        ..c(MetaConditionKind::Not)
                    },
                    MetaCondition {
                        tertiary_number: 3.0,
                        number: 2.0,
                        secondary_number: 12.5,
                        ..c(MetaConditionKind::MonsterPriorityCountWithinDistance)
                    },
                    MetaCondition {
                        text: "^(?<who>.+) tells you".to_owned(),
                        secondary_text: "3,4".to_owned(),
                        ..c(MetaConditionKind::ChatMessageCapture)
                    },
                ],
                ..MetaCondition::default()
            },
            MetaAction {
                kind: MetaActionKind::All,
                children: vec![
                    MetaAction {
                        text: "EnableCombat".to_owned(),
                        secondary_text: "combat".to_owned(),
                        ..MetaAction::of(MetaActionKind::GetVtankOption)
                    },
                    MetaAction {
                        text: "EnableLooting".to_owned(),
                        secondary_text: "1".to_owned(),
                        ..MetaAction::of(MetaActionKind::SetVtankOption)
                    },
                    MetaAction {
                        text: "Stuck".to_owned(),
                        number: 5.0,
                        secondary_number: 60.0,
                        ..MetaAction::of(MetaActionKind::SetWatchdog)
                    },
                    MetaAction::of(MetaActionKind::ClearWatchdog),
                    MetaAction {
                        text: "Buff".to_owned(),
                        secondary_text: "Default".to_owned(),
                        ..MetaAction::of(MetaActionKind::CallMetaState)
                    },
                    MetaAction::of(MetaActionKind::DestroyAllViews),
                ],
                ..MetaAction::default()
            },
        ));
        profile.rules.push(rule(
            MetaCondition::always(),
            MetaAction {
                secondary_text: "jumps".to_owned(),
                embedded_route: Some(Box::new(jump_route)),
                ..MetaAction::of(MetaActionKind::LoadEmbeddedNavigationRoute)
            },
        ));
        profile.rules.push(rule(
            c(MetaConditionKind::Never),
            MetaAction {
                secondary_text: "[None]".to_owned(),
                embedded_route: Some(Box::new(follow_route)),
                ..MetaAction::of(MetaActionKind::LoadEmbeddedNavigationRoute)
            },
        ));
        profile.rules.push(rule(
            c(MetaConditionKind::Always),
            MetaAction {
                text: "v".to_owned(),
                secondary_text: "<view/>".to_owned(),
                ..MetaAction::of(MetaActionKind::CreateView)
            },
        ));

        let first = save(&profile).unwrap();
        let reread = try_load(&first, None).unwrap();
        assert_eq!(save(&reread).unwrap(), first);
        let jump = &reread.rules[1]
            .action
            .embedded_route
            .as_ref()
            .unwrap()
            .waypoints[1];
        assert_eq!(jump.jump_direction, RouteJumpDirection::StrafeRight);
        assert_eq!(jump.jump_charge_milliseconds, 300);
        assert_eq!(
            reread.rules[2]
                .action
                .embedded_route
                .as_ref()
                .unwrap()
                .follow_target_name,
            "Barris"
        );
        assert_eq!(reread.rules[3].action.secondary_text, "<view/>");
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankFileWriterTests.cs :: WhatAMetCannotHoldIsRefused
    #[test]
    fn what_a_met_cannot_hold_is_refused() {
        let mut disabled = MetaProfile::default();
        disabled
            .rules
            .push(rule(MetaCondition::always(), MetaAction::default()));
        disabled.rules[0].enabled = false;
        assert!(save(&disabled).unwrap_err().contains("switched off"));

        let mut edge = MetaProfile::default();
        edge.rules.push(rule(
            MetaCondition::of(MetaConditionKind::LoginComplete),
            MetaAction::default(),
        ));
        assert!(save(&edge).is_err());

        let mut fraction = MetaProfile::default();
        fraction.rules.push(rule(
            MetaCondition {
                number: 2.5,
                ..MetaCondition::of(MetaConditionKind::SecondsInStateGreaterThanOrEqual)
            },
            MetaAction::default(),
        ));
        assert!(save(&fraction).is_err());
    }

    const ROW_HEAD: &str = "1\r\nCondAct\r\n5\r\nCType\r\nAType\r\nCData\r\nAData\r\nState\r\nn\r\nn\r\nn\r\nn\r\nn\r\n1\r\n";

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankMetaProfileSerializerTests.cs :: LoadsKnownTypedCondActRecord
    #[test]
    fn loads_known_typed_cond_act_record() {
        let source =
            format!("{ROW_HEAD}i\r\n1\r\ni\r\n2\r\ni\r\n0\r\ns\r\n/say ready\r\ns\r\nDefault\r\n");
        let profile = try_load(&source, None).unwrap();
        assert_eq!(profile.rules.len(), 1);
        let rule = &profile.rules[0];
        assert_eq!(rule.condition.kind, MetaConditionKind::Always);
        assert_eq!(rule.action.kind, MetaActionKind::ChatCommand);
        assert_eq!(rule.action.text, "/say ready");
        assert_eq!(rule.state, "Default");
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankMetaProfileSerializerTests.cs :: AnEmbeddedRouteLongerThanTheCollectionBoundLoads
    #[test]
    fn an_embedded_route_longer_than_the_collection_bound_loads() {
        let nodes: u32 = 2_000;
        let mut route = format!("uTank2 NAV 1.2\r\n4\r\n{nodes}\r\n");
        for index in 0..nodes {
            route.push_str(&format!(
                "0\r\n{}\r\n-41.2450457890828\r\n0.483354187011719\r\n0\r\n",
                to_string_f64(44.086_141_777_038_6 + f64::from(index) / 1000.0)
            ));
        }
        let name = "long";
        let serialized = name.len() + 2 + nodes.to_string().len() + 2 + route.len();
        assert!(serialized > 100_000);
        let source = format!(
            "{ROW_HEAD}i\r\n1\r\ni\r\n4\r\ni\r\n0\r\nba\r\n{serialized}\r\n{name}\r\n{nodes}\r\n{route}s\r\nDefault\r\n"
        );
        let profile = try_load(&source, None).unwrap();
        let rule = &profile.rules[0];
        assert_eq!(
            rule.action.kind,
            MetaActionKind::LoadEmbeddedNavigationRoute
        );
        assert_eq!(
            rule.action.embedded_route.as_ref().unwrap().waypoints.len(),
            nodes as usize
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankMetaProfileSerializerTests.cs :: SignedHighBitLandblockIdRoundTripsExactly
    #[test]
    fn signed_high_bit_landblock_id_round_trips_exactly() {
        let high_bit_value = 0x8B37_0000_u32.cast_signed();
        assert!(high_bit_value < 0);
        let source = format!(
            "{ROW_HEAD}i\r\n17\r\ni\r\n0\r\ni\r\n{high_bit_value}\r\ni\r\n0\r\ns\r\nDefault\r\n"
        );
        let profile = try_load(&source, None).unwrap();
        let rule = &profile.rules[0];
        assert_eq!(rule.condition.kind, MetaConditionKind::LandblockEquals);
        assert_eq!(rule.condition.number, f64::from(high_bit_value));
    }
}
