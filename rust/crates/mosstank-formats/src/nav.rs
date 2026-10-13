//! VTank route files (`.nav`), version 1.2.
//!
//! Written from the file format (the fixture routes and the C# tests'
//! inputs and outputs), not from the C# reader; see `NOTICE.md`.
//!
//! ```text
//! uTank2 NAV 1.2
//! <route type: 1 circular, 2 linear, 3 follow, 4 once>
//! follow:  <target name> <target id>
//! others:  <waypoint count>, then per waypoint:
//!          <waypoint type> <east-west> <north-south> <height> 0 <extras by type>
//! ```
//!
//! Extras: portal (1) an object id; recall (2) a spell id; pause (3)
//! milliseconds; chat (4) a line; vendor (5) an object id and a name;
//! portal or NPC by name (6, 7) a name, an object class, `True`/`False`,
//! and the target's three coordinates; jump (9) a heading, the shift flag
//! and the charge, whose fifth decimal carries the direction (3 forward,
//! 4 strafe left, 5 strafe right). A route embedded in a meta, or saved by
//! some tools, is wrapped in a name line and a node-count line.

use mosstank_dotnet::{
    ARITHMETIC_OVERFLOW, compat::number::to_string_f64, format_g15, parse_bool, parse_double,
    parse_int, round_to_int_checked,
};

use crate::route::{
    NavigationSettings, RouteJumpDirection, RouteMode, RouteRecallKind, RouteWaypoint,
    RouteWaypointType, SpellLookup, map_position,
};
use crate::usd::VtankLineCursor;

/// The first line of every route file.
pub const NAV_HEADER: &str = "uTank2 NAV 1.2";

const END_OF_DATA: &str = "Unexpected end of VTank nav data.";

/// Reads a route into `target`: its mode, its waypoints and, for a follow
/// route, its target. Nothing changes when the text does not read.
///
/// # Errors
/// Why the text is not a route.
pub fn try_load(
    text: &str,
    target: &mut NavigationSettings,
    spells: Option<&dyn SpellLookup>,
) -> Result<(), String> {
    // A line ends at CR LF, CR or LF here.
    let lines = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut cursor = VtankLineCursor::new(&lines);
    let first = next_line(&mut cursor)?;
    let parsed = if first == NAV_HEADER {
        read_route_body(&mut cursor, spells)?
    } else {
        // A wrapped route: its name and its node count (which a standalone
        // file does not have to keep), then the route.
        parse_int(next_line(&mut cursor)?)?;
        read_route(&mut cursor, spells)?
    };
    apply(target, parsed);
    Ok(())
}

/// Puts a read route into the live settings.
pub fn apply(target: &mut NavigationSettings, parsed: NavigationSettings) {
    target.mode = parsed.mode;
    target.waypoints = parsed.waypoints;
    if parsed.mode == RouteMode::Target {
        target.follow_target_name = parsed.follow_target_name;
        target.follow_target_object_id = parsed.follow_target_object_id;
    }
}

/// The node count a wrapper line gives for a route: its waypoints, or one
/// for a follow route.
#[must_use]
pub fn node_count(route: &NavigationSettings) -> i32 {
    if route.mode == RouteMode::Target {
        1
    } else {
        i32::try_from(route.waypoints.len()).unwrap_or(i32::MAX)
    }
}

pub(crate) fn next_line<'a>(cursor: &mut VtankLineCursor<'a>) -> Result<&'a str, String> {
    cursor.read_line().map_err(|_| END_OF_DATA.to_owned())
}

/// Reads a route starting at its header line.
///
/// # Errors
/// Why the lines are not a route.
pub(crate) fn read_route(
    cursor: &mut VtankLineCursor<'_>,
    spells: Option<&dyn SpellLookup>,
) -> Result<NavigationSettings, String> {
    if next_line(cursor)? != NAV_HEADER {
        return Err("Nav file version does not match uTank2 NAV 1.2.".to_owned());
    }
    read_route_body(cursor, spells)
}

pub(crate) fn read_route_body(
    cursor: &mut VtankLineCursor<'_>,
    spells: Option<&dyn SpellLookup>,
) -> Result<NavigationSettings, String> {
    let mode = match parse_int(next_line(cursor)?)? {
        1 => RouteMode::Circular,
        2 => RouteMode::Linear,
        3 => RouteMode::Target,
        4 => RouteMode::Once,
        _ => return Err("Unknown VTank navigation type.".to_owned()),
    };
    let mut route = NavigationSettings::with_mode(mode);
    if mode == RouteMode::Target {
        route.follow_target_name = next_line(cursor)?.to_owned();
        route.follow_target_object_id = parse_int(next_line(cursor)?)?.cast_unsigned();
        return Ok(route);
    }
    let count = parse_int(next_line(cursor)?)?;
    for _ in 0..count {
        route.waypoints.push(read_waypoint(cursor, spells)?);
    }
    Ok(route)
}

fn read_waypoint(
    cursor: &mut VtankLineCursor<'_>,
    spells: Option<&dyn SpellLookup>,
) -> Result<RouteWaypoint, String> {
    let code = parse_int(next_line(cursor)?)?;
    let kind = RouteWaypointType::from_i32(code)
        .ok_or_else(|| format!("Unknown VTank waypoint type {code}."))?;
    let east_west = parse_double(next_line(cursor)?)?;
    let north_south = parse_double(next_line(cursor)?)?;
    let elevation = parse_double(next_line(cursor)?)?;
    // A line every waypoint carries and nothing reads.
    next_line(cursor)?;
    let mut waypoint = RouteWaypoint::at(kind, east_west, north_south, elevation);
    match kind {
        RouteWaypointType::Point | RouteWaypointType::Checkpoint => {}
        RouteWaypointType::Portal => {
            waypoint.object_id = parse_int(next_line(cursor)?)?.cast_unsigned();
        }
        RouteWaypointType::Recall => {
            let id = parse_int(next_line(cursor)?)?;
            waypoint.recall_spell_id =
                u32::try_from(id).map_err(|_| ARITHMETIC_OVERFLOW.to_owned())?;
            if let Some(recall) = RouteRecallKind::from_spell_id(waypoint.recall_spell_id) {
                waypoint.recall = recall;
                waypoint.recall_spell_name = recall.display_name().to_owned();
            } else if let Some(name) = spells.and_then(|s| s.name_of(waypoint.recall_spell_id)) {
                waypoint.recall_spell_name = name;
            }
        }
        RouteWaypointType::Pause => {
            waypoint.duration_milliseconds = parse_int(next_line(cursor)?)?;
        }
        RouteWaypointType::ChatCommand => {
            waypoint.text = next_line(cursor)?.to_owned();
        }
        RouteWaypointType::OpenVendor => {
            waypoint.object_id = parse_int(next_line(cursor)?)?.cast_unsigned();
            waypoint.object_name = next_line(cursor)?.to_owned();
        }
        RouteWaypointType::PortalByName | RouteWaypointType::UseNpc => {
            waypoint.object_name = next_line(cursor)?.to_owned();
            waypoint.legacy_object_class = parse_int(next_line(cursor)?)?;
            waypoint.legacy_reference_valid = parse_bool(next_line(cursor)?)?;
            let x = parse_double(next_line(cursor)?)?;
            let y = parse_double(next_line(cursor)?)?;
            let z = parse_double(next_line(cursor)?)?;
            waypoint.reference_position = map_position(x, y, z);
        }
        RouteWaypointType::Jump => {
            waypoint.jump_heading_degrees = parse_double(next_line(cursor)?)?;
            waypoint.jump_hold_shift = parse_bool(next_line(cursor)?)?;
            let charge = parse_double(next_line(cursor)?)?;
            waypoint.jump_charge_milliseconds = round_to_int_checked(charge)?;
            waypoint.jump_direction = decode_jump_direction(charge);
        }
    }
    Ok(waypoint)
}

/// The direction a jump's charge carries in its fifth decimal: the charge
/// read as a decimal of fifteen significant digits whose fraction is
/// exactly `.00003`, `.00004` or `.00005`. Anything else is forward.
#[must_use]
pub fn decode_jump_direction(charge: f64) -> RouteJumpDirection {
    match fifteen_digit_fraction(charge).as_deref() {
        Some("00004") => RouteJumpDirection::StrafeLeft,
        Some("00005") => RouteJumpDirection::StrafeRight,
        _ => RouteJumpDirection::Forward,
    }
}

/// The fraction digits (trailing zeros dropped) of `value` rounded to
/// fifteen significant digits, as a conversion to `decimal` rounds it.
fn fifteen_digit_fraction(value: f64) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    // G15 keeps the same fifteen significant digits, in fixed or E form.
    let text = format_g15(value.abs());
    let (mantissa, exponent) = match text.split_once('E') {
        Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().ok()?),
        None => (text.as_str(), 0),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = format!("{whole}{fraction}");
    // The decimal point sits after this many of `digits`.
    let point = i32::try_from(whole.len()).ok()? + exponent;
    let fraction = if point <= 0 {
        let zeros = usize::try_from(-point).ok()?;
        format!("{}{digits}", "0".repeat(zeros))
    } else {
        let point = usize::try_from(point).ok()?;
        if point >= digits.len() {
            String::new()
        } else {
            digits[point..].to_owned()
        }
    };
    Some(fraction.trim_end_matches('0').to_owned())
}

/// The charge a jump is written with: its milliseconds and its direction
/// code in the fifth decimal.
#[must_use]
pub fn encode_jump_charge(milliseconds: i32, direction: RouteJumpDirection) -> f64 {
    f64::from(milliseconds) + f64::from(jump_direction_code(direction)) / 100_000.0
}

/// The code a direction is written with (a backward jump has none and is
/// written forward where a format does not refuse it).
#[must_use]
pub fn jump_direction_code(direction: RouteJumpDirection) -> i32 {
    match direction {
        RouteJumpDirection::StrafeLeft => 4,
        RouteJumpDirection::StrafeRight => 5,
        RouteJumpDirection::Forward | RouteJumpDirection::Backward => 3,
    }
}

/// A number as route files write it: the shortest text that reads back as
/// the same value, and zero without a sign.
///
/// # Errors
/// The number is not finite.
pub fn file_number(value: f64) -> Result<String, String> {
    if !value.is_finite() {
        return Err("A .nav file cannot hold a number that is not finite.".to_owned());
    }
    Ok(if value == 0.0 {
        "0".to_owned()
    } else {
        to_string_f64(value)
    })
}

/// Writes a route.
///
/// # Errors
/// What the format cannot hold: a backward jump, a line break inside a
/// one-line field, a number that is not finite, a spell id past the
/// format's range.
pub fn save(route: &NavigationSettings) -> Result<String, String> {
    let mut out = String::new();
    line(&mut out, NAV_HEADER);
    let mode = match route.mode {
        RouteMode::Circular => 1,
        RouteMode::Linear => 2,
        RouteMode::Target => 3,
        RouteMode::Once => 4,
    };
    line(&mut out, &mode.to_string());
    if route.mode == RouteMode::Target {
        line(&mut out, &route.follow_target_name);
        line(
            &mut out,
            &route.follow_target_object_id.cast_signed().to_string(),
        );
        return Ok(out);
    }
    line(&mut out, &route.waypoints.len().to_string());
    for waypoint in &route.waypoints {
        write_waypoint(&mut out, waypoint)?;
    }
    Ok(out)
}

fn write_waypoint(out: &mut String, waypoint: &RouteWaypoint) -> Result<(), String> {
    line(out, &(waypoint.kind as i32).to_string());
    line(out, &file_number(waypoint.position.east_west)?);
    line(out, &file_number(waypoint.position.north_south)?);
    line(out, &file_number(waypoint.position.elevation)?);
    line(out, "0");
    match waypoint.kind {
        RouteWaypointType::Point | RouteWaypointType::Checkpoint => {}
        RouteWaypointType::Portal => {
            line(out, &waypoint.object_id.cast_signed().to_string());
        }
        RouteWaypointType::Recall => {
            let id = if waypoint.recall_spell_id != 0 {
                waypoint.recall_spell_id
            } else {
                waypoint.recall.spell_id()
            };
            let id = i32::try_from(id).map_err(|_| ARITHMETIC_OVERFLOW.to_owned())?;
            line(out, &id.to_string());
        }
        RouteWaypointType::Pause => {
            line(out, &waypoint.duration_milliseconds.to_string());
        }
        RouteWaypointType::ChatCommand => {
            line(out, single_line(&waypoint.text, "chat command")?);
        }
        RouteWaypointType::OpenVendor => {
            line(out, &waypoint.object_id.cast_signed().to_string());
            line(out, single_line(&waypoint.object_name, "vendor name")?);
        }
        RouteWaypointType::PortalByName | RouteWaypointType::UseNpc => {
            line(out, &waypoint.object_name);
            line(out, &waypoint.legacy_object_class.to_string());
            line(
                out,
                if waypoint.legacy_reference_valid {
                    "True"
                } else {
                    "False"
                },
            );
            line(out, &file_number(waypoint.reference_position.east_west)?);
            line(out, &file_number(waypoint.reference_position.north_south)?);
            line(out, &file_number(waypoint.reference_position.elevation)?);
        }
        RouteWaypointType::Jump => {
            if waypoint.jump_direction == RouteJumpDirection::Backward {
                return Err("A .nav file has no backward jump.".to_owned());
            }
            line(out, &file_number(waypoint.jump_heading_degrees)?);
            line(
                out,
                if waypoint.jump_hold_shift {
                    "True"
                } else {
                    "False"
                },
            );
            line(
                out,
                &file_number(encode_jump_charge(
                    waypoint.jump_charge_milliseconds,
                    waypoint.jump_direction,
                ))?,
            );
        }
    }
    Ok(())
}

fn single_line<'a>(text: &'a str, what: &str) -> Result<&'a str, String> {
    if text.contains(['\r', '\n']) {
        Err(format!(
            "A .nav file cannot hold a {what} that runs over more than one line."
        ))
    } else {
        Ok(text)
    }
}

fn line(out: &mut String, text: &str) {
    out.push_str(text);
    out.push_str("\r\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::line_endings::match_existing;
    use crate::route::NoSpells;

    fn load(text: &str) -> NavigationSettings {
        let mut route = NavigationSettings::default();
        try_load(text, &mut route, Some(&NoSpells)).unwrap_or_else(|error| panic!("{error}"));
        route
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankFileWriterTests.cs :: EveryNavFixtureSavesBackToTheSameBytes
    #[test]
    fn every_nav_fixture_saves_back_to_the_same_bytes() {
        let fixtures = crate::fixtures::list("vtank", "nav");
        assert!(fixtures.len() >= 10, "{fixtures:?}");
        for fixture in fixtures {
            let original = crate::fixtures::read(&fixture);
            let route = load(&original);
            let saved = match_existing(&save(&route).unwrap(), Some(&original));
            assert_eq!(saved, original, "{fixture}");
        }
    }

    const EVERY_WAYPOINT_NAV: &str = concat!(
        "uTank2 NAV 1.2\r\n4\r\n9\r\n",
        "0\r\n1.5\r\n-2.25\r\n0.05\r\n0\r\n",
        "1\r\n1.5\r\n-2.25\r\n0.05\r\n0\r\n-2147483000\r\n",
        "2\r\n1.5\r\n-2.25\r\n0.05\r\n0\r\n48\r\n",
        "3\r\n1.5\r\n-2.25\r\n0.05\r\n0\r\n2500\r\n",
        "4\r\n1.5\r\n-2.25\r\n0.05\r\n0\r\n/say hi\r\n",
        "5\r\n1.5\r\n-2.25\r\n0.05\r\n0\r\n1342431769\r\nArchmage\r\n",
        "8\r\n1.5\r\n-2.25\r\n0.05\r\n0\r\n",
        "9\r\n1.5\r\n-2.25\r\n0.05\r\n0\r\n12.3456\r\nTrue\r\n150.00004\r\n",
        "9\r\n1.5\r\n-2.25\r\n0.05\r\n0\r\n270\r\nFalse\r\n5E-05\r\n"
    );

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankFileWriterTests.cs :: EveryWaypointTypeSavesBackAsItWasRead
    #[test]
    fn every_waypoint_type_saves_back_as_it_was_read() {
        let route = load(EVERY_WAYPOINT_NAV);
        assert_eq!(
            route.waypoints[7].jump_direction,
            RouteJumpDirection::StrafeLeft
        );
        assert_eq!(route.waypoints[7].jump_charge_milliseconds, 150);
        assert_eq!(
            route.waypoints[8].jump_direction,
            RouteJumpDirection::StrafeRight
        );
        assert_eq!(route.waypoints[8].jump_charge_milliseconds, 0);
        assert_eq!(save(&route).unwrap(), EVERY_WAYPOINT_NAV);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankFileWriterTests.cs :: AJumpMadeHereIsWrittenAsTheRouteToolWritesOne
    #[test]
    fn a_jump_made_here_is_written_as_the_route_tool_writes_one() {
        let mut route = NavigationSettings::with_mode(RouteMode::Circular);
        let mut first = RouteWaypoint::at(RouteWaypointType::Jump, 10.0, 20.0, 0.0);
        first.jump_heading_degrees = 90.0;
        first.jump_hold_shift = true;
        first.jump_charge_milliseconds = 50;
        route.waypoints.push(first);
        let mut second = RouteWaypoint::at(RouteWaypointType::Jump, 10.0, 20.0, 0.0);
        second.jump_heading_degrees = 180.0;
        second.jump_charge_milliseconds = 0;
        route.waypoints.push(second);
        assert_eq!(
            save(&route).unwrap(),
            concat!(
                "uTank2 NAV 1.2\r\n1\r\n2\r\n",
                "9\r\n10\r\n20\r\n0\r\n0\r\n90\r\nTrue\r\n50.00003\r\n",
                "9\r\n10\r\n20\r\n0\r\n0\r\n180\r\nFalse\r\n3E-05\r\n"
            )
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankFileWriterTests.cs :: ANumberFifteenDigitsCannotHoldKeepsItsDigits
    #[test]
    fn a_number_fifteen_digits_cannot_hold_keeps_its_digits() {
        let nav = "uTank2 NAV 1.2\r\n1\r\n1\r\n0\r\n-34.89208150285805\r\n1\r\n0\r\n0\r\n";
        assert_eq!(save(&load(nav)).unwrap(), nav);

        let mut measured = NavigationSettings::with_mode(RouteMode::Once);
        measured.waypoints.push(RouteWaypoint::at(
            RouteWaypointType::Point,
            f64::from(12.3f32),
            -0.0,
            1.0,
        ));
        assert_eq!(
            save(&measured).unwrap(),
            "uTank2 NAV 1.2\r\n4\r\n1\r\n0\r\n12.300000190734863\r\n0\r\n1\r\n0\r\n"
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankFileWriterTests.cs :: AFollowRouteSavesAsItsTarget
    #[test]
    fn a_follow_route_saves_as_its_target() {
        let nav = "uTank2 NAV 1.2\r\n3\r\nBarris\r\n1342431769\r\n";
        assert_eq!(save(&load(nav)).unwrap(), nav);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankFileWriterTests.cs :: ARouteWithABackwardJumpIsRefused
    #[test]
    fn a_route_with_a_backward_jump_is_refused() {
        let mut route = NavigationSettings::default();
        route.waypoints.push(RouteWaypoint {
            kind: RouteWaypointType::Jump,
            jump_direction: RouteJumpDirection::Backward,
            ..RouteWaypoint::default()
        });
        assert!(save(&route).unwrap_err().contains("backward"));
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankFileWriterTests.cs :: AChatCommandOverTwoLinesIsRefused
    #[test]
    fn a_chat_command_over_two_lines_is_refused() {
        let mut route = NavigationSettings::default();
        route.waypoints.push(RouteWaypoint {
            kind: RouteWaypointType::ChatCommand,
            text: "/say one\n/say two".to_owned(),
            ..RouteWaypoint::default()
        });
        assert!(save(&route).is_err());
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankNavRouteSerializerTests.cs :: LoadsEveryOfficialNav12WaypointPayload
    #[test]
    fn loads_every_official_nav12_waypoint_payload() {
        let nav = [
            "uTank2 NAV 1.2",
            "1",
            "10",
            "0",
            "12.5",
            "-3.25",
            "0.5",
            "0",
            "1",
            "13",
            "-4",
            "0",
            "0",
            "1234",
            "2",
            "14",
            "-5",
            "0",
            "0",
            "987",
            "3",
            "15",
            "-6",
            "0",
            "0",
            "2500",
            "4",
            "16",
            "-7",
            "0",
            "0",
            "/say hello",
            "5",
            "17",
            "-8",
            "0",
            "0",
            "4321",
            "Vendor Bob",
            "6",
            "18",
            "-9",
            "0",
            "0",
            "Portal to Holtburg",
            "14",
            "True",
            "18.5",
            "-9.5",
            "1",
            "7",
            "19",
            "-10",
            "0",
            "0",
            "Town Crier",
            "37",
            "True",
            "19.5",
            "-10.5",
            "2",
            "8",
            "20",
            "-11",
            "0",
            "0",
            "9",
            "21",
            "-12",
            "0",
            "0",
            "180.5",
            "True",
            "1000.00005",
        ]
        .join("\n");
        let mut settings = NavigationSettings {
            enabled: true,
            ..NavigationSettings::default()
        };
        try_load(&nav, &mut settings, Some(&NoSpells)).unwrap();
        assert!(settings.enabled);
        assert_eq!(settings.mode, RouteMode::Circular);
        let w = &settings.waypoints;
        assert_eq!(w.len(), 10);
        assert_eq!(w[0].kind, RouteWaypointType::Point);
        assert_eq!(w[0].position.east_west, 12.5);
        assert_eq!(w[1].object_id, 1234);
        assert_eq!(w[2].recall_spell_id, 987);
        assert_eq!(w[3].duration_milliseconds, 2500);
        assert_eq!(w[4].text, "/say hello");
        assert_eq!(w[5].object_name, "Vendor Bob");
        assert_eq!(w[6].position.east_west, 18.0);
        assert_eq!(w[6].reference_position.east_west, 18.5);
        assert_eq!(w[6].reference_position.north_south, -9.5);
        assert_eq!(w[6].reference_position.elevation, 1.0);
        assert_eq!(w[7].object_name, "Town Crier");
        assert_eq!(w[7].position.east_west, 19.0);
        assert_eq!(w[7].reference_position.east_west, 19.5);
        assert_eq!(w[6].legacy_object_class, 14);
        assert_eq!(w[7].legacy_object_class, 37);
        assert!(w[6].legacy_reference_valid);
        assert_eq!(w[8].kind, RouteWaypointType::Checkpoint);
        assert_eq!(w[9].jump_heading_degrees, 180.5);
        assert!(w[9].jump_hold_shift);
        assert_eq!(w[9].jump_charge_milliseconds, 1000);
        assert_eq!(w[9].jump_direction, RouteJumpDirection::StrafeRight);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankNavRouteSerializerTests.cs :: LoadsEmbeddedWrapperAndDoesNotMutateOnFailure
    #[test]
    fn loads_embedded_wrapper_and_does_not_mutate_on_failure() {
        let wrapped = "Route Name\n1\nuTank2 NAV 1.2\n4\n1\n0\n1\n2\n3\n0";
        let mut settings = NavigationSettings::default();
        try_load(wrapped, &mut settings, Some(&NoSpells)).unwrap();
        assert_eq!(settings.mode, RouteMode::Once);
        assert_eq!(settings.waypoints.len(), 1);

        let error = try_load("broken", &mut settings, Some(&NoSpells)).unwrap_err();
        assert!(!error.is_empty());
        assert_eq!(settings.mode, RouteMode::Once);
        assert_eq!(settings.waypoints.len(), 1);
    }

    #[test]
    fn a_jump_charge_is_written_as_its_milliseconds_and_code() {
        // Golden: the C# writer, every charge 0..=20000 ms in each direction
        // (the oracle sweep in tests/golden/README of this crate).
        for ms in 0..=20_000 {
            for (direction, code) in [
                (RouteJumpDirection::Forward, 3),
                (RouteJumpDirection::StrafeLeft, 4),
                (RouteJumpDirection::StrafeRight, 5),
            ] {
                let expected = if ms == 0 {
                    format!("{code}E-05")
                } else {
                    format!("{ms}.0000{code}")
                };
                let written = file_number(encode_jump_charge(ms, direction)).unwrap();
                assert_eq!(written, expected);
                assert_eq!(
                    decode_jump_direction(encode_jump_charge(ms, direction)),
                    direction
                );
            }
        }
        assert_eq!(
            file_number(encode_jump_charge(-5, RouteJumpDirection::Forward)).unwrap(),
            "-4.99997"
        );
    }

    #[test]
    fn a_charge_carries_a_direction_only_in_exactly_its_fifth_decimal() {
        // Golden: the C# reader (oracle probes).
        for (charge, direction) in [
            (300.123_456, RouteJumpDirection::Forward),
            (300.000_035, RouteJumpDirection::Forward),
            (300.000_045, RouteJumpDirection::Forward),
            (299.999_99, RouteJumpDirection::Forward),
            (0.000_04, RouteJumpDirection::StrafeLeft),
            (1e-6, RouteJumpDirection::Forward),
            (300.000_049_999, RouteJumpDirection::Forward),
            (300.000_13, RouteJumpDirection::Forward),
            (-300.000_04, RouteJumpDirection::StrafeLeft),
            (300.000_02, RouteJumpDirection::Forward),
            (300.000_09, RouteJumpDirection::Forward),
            (300.5, RouteJumpDirection::Forward),
        ] {
            assert_eq!(decode_jump_direction(charge), direction, "{charge}");
        }
    }
}
