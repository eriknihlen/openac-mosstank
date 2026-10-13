//! The settings crate against the C# plugin's own results: every case in
//! `golden/cases.txt` is run here and compared with `golden/cases.out`,
//! which the C# plugin wrote (`RustPortGolden.cs` with
//! `RustPortSettingsGolden.cs` in the C# tests). Large results (the
//! fixtures and whole documents) are kept as the length and FNV-1a hash of
//! each part; on a mismatch this test writes its full output to
//! `target/golden/settings.rust.out` to compare with the C# full output in
//! `target/golden/settings.full.out`.
//!
//! Source: the C# settings serializer, default document, monster specs and
//! UB catalogue as a black box, run over the cases.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use mosstank_data::damage::MonsterDamageType;
use mosstank_formats::fixtures;
use mosstank_settings::model::{
    AllSettings, ConsumableCategory, GemFoodItem, MeleeAttackHeight, TargetSelectionMethod,
};
use mosstank_settings::monster::{
    MonsterExpression, MonsterExpressionContext, MonsterRule, MonsterRuleActions, MonsterValue,
    MonsterValueKind,
};
use mosstank_settings::ub::{
    UbSettingCatalog, UbSettingKind, UbSettingValue, UbSettingValueBag, definitions,
};
use mosstank_settings::{defaults, options, profile};

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' || chars.peek().is_none() {
            out.push(c);
            continue;
        }
        let next = chars.next().expect("peeked");
        match next {
            'r' => out.push('\r'),
            'n' => out.push('\n'),
            't' => out.push('\t'),
            '\\' => out.push('\\'),
            'u' => {
                let rest: String = chars.by_ref().take_while(|c| *c != '}').collect();
                let hex = rest.trim_start_matches('{');
                let code = u32::from_str_radix(hex, 16).expect("a hex escape");
                out.push(char::from_u32(code).expect("a scalar value"));
            }
            other => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    out
}

fn show(text: &str) -> String {
    text.replace('\r', "\\r")
        .replace('\n', "\\n\n")
        .replace('\t', "\\t")
}

fn esc(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn bool_text(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

fn fnv64(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("fnv64={hash:016x} bytes={}\n", text.len())
}

fn hashed(result: &str) -> String {
    let mut out = String::new();
    let mut part = String::new();
    for line in result.split('\n') {
        if line.starts_with("--- ") {
            out.push_str(&fnv64(&part));
            part.clear();
            out.push_str(line);
            out.push('\n');
            continue;
        }
        part.push_str(line);
        part.push('\n');
    }
    out.push_str(&fnv64(&part));
    out
}

fn is_large(kind: &str, field: &str) -> bool {
    field.starts_with('@') || kind == "defaults" || kind == "create-new"
}

fn kind_name(kind: MonsterValueKind) -> &'static str {
    match kind {
        MonsterValueKind::Number => "Number",
        MonsterValueKind::Text => "Text",
        MonsterValueKind::Boolean => "Boolean",
    }
}

fn dump(out: &mut String, s: &AllSettings) {
    for name in options::COMMAND_NAMES.iter() {
        match profile::capture(name, s) {
            Some(cell) => writeln!(out, "{name} = {} {}", cell.tag, esc(cell.as_string())),
            None => writeln!(out, "{name} = -"),
        }
        .unwrap();
    }
    for rule in &s.combat.rules {
        let a = rule.actions();
        writeln!(
            out,
            "rule {} flags={} p={} dmg={:?} extra={:?} pet={:?} w={:08X} o={:08X} wr={} sr={} ignored={} default={} dynamic={}",
            esc(rule.expression()),
            a.flags.0,
            a.priority,
            a.damage_type,
            a.extra_vulnerability,
            a.pet_damage_type,
            a.weapon_object_id,
            a.offhand_object_id,
            a.weapon_to_use_raw,
            a.secondary_equip_raw,
            bool_text(rule.is_ignored_spec()),
            bool_text(rule.is_default()),
            bool_text(rule.is_dynamic())
        )
        .unwrap();
    }
    let items: Vec<String> = s
        .combat
        .combat_item_order_ids
        .iter()
        .map(|id| format!("{id:08X}"))
        .collect();
    writeln!(out, "items {}", items.join(",")).unwrap();
    for row in &s.buffs.item_enchant_rows {
        writeln!(
            out,
            "enchant {} {} nothing={}",
            row.object_id
                .map_or_else(|| "-".to_owned(), |id| format!("{id:08X}")),
            row.spell_id
                .map_or_else(|| "-".to_owned(), |id| id.to_string()),
            bool_text(row.casts_nothing())
        )
        .unwrap();
    }
    for (object, uses) in s.combat.item_use_specifiers.iter() {
        writeln!(out, "uses {object:08X}={uses}").unwrap();
    }
    for item in &s.combat.imported_assist_items {
        writeln!(out, "assist {} {:?}", esc(&item.name), item.category).unwrap();
    }
    for item in &s.buffs.gem_food_items {
        writeln!(out, "gem {} {}", esc(&item.name), item.spell_id).unwrap();
    }
    let join = |ids: &[u32]| ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    writeln!(out, "extra {}", join(&s.buffs.extra_buff_spell_ids)).unwrap();
    writeln!(out, "anti {}", join(&s.buffs.anti_extra_buff_spell_ids)).unwrap();
    for row in &s.vitals.recharge_handler_rows {
        writeln!(
            out,
            "recharge {}|{}|{}|{}|{}",
            row.vital,
            esc(&row.handler_string),
            row.min_percent,
            row.max_percent,
            row.stance
        )
        .unwrap();
    }
    writeln!(out, "meta {}", bool_text(s.meta.enabled)).unwrap();
}

fn usd(input: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut s = AllSettings::default();
    let mut warnings: Vec<String> = Vec::new();
    let mut warn = |text: &str| warnings.push(text.to_owned());
    let mut document = profile::load(input, &mut s, Some(&mut warn)).map_err(|e| e.0)?;
    for warning in &warnings {
        writeln!(out, "warn {warning}").unwrap();
    }
    dump(&mut out, &s);
    let saved = profile::save(&mut document, &mut s).map_err(|e| e.0)?;
    write!(out, "--- saved:\n{}\n", show(&saved)).unwrap();

    s.combat.maximum_range = 30.0;
    s.vitals.normal_health = 0.6;
    s.buffs.protection_elements = "AB".to_owned();
    s.combat.selection_method = TargetSelectionMethod::RANGE;
    s.navigation.minimum_distance_meters = 3.3;
    s.buffs.idle_buff_topoff_seconds = 1234.5;
    s.combat.attack_height = MeleeAttackHeight(3);
    s.meta.enabled = true;
    s.combat.consumable_names.insert("Golden Kit".to_owned());
    s.combat
        .consumable_categories
        .insert("Golden Kit".to_owned(), ConsumableCategory::HealthKit);
    s.combat.combat_item_order_ids.push(0x5000_0001);
    s.buffs.gem_food_items = vec![GemFoodItem {
        name: "Golden Gem".to_owned(),
        spell_id: 4321,
    }];
    s.buffs.extra_buff_spell_ids.push(777);
    s.combat.rules.push(
        MonsterRule::new(
            "name==Golden Drudge",
            MonsterRuleActions {
                priority: 3,
                damage_type: MonsterDamageType::Fire,
                ..MonsterRuleActions::default()
            },
        )
        .map_err(|e| e.clone())?,
    );
    let edited = profile::save(&mut document, &mut s).map_err(|e| e.0)?;
    write!(out, "--- edited:\n{}\n", show(&edited)).unwrap();
    Ok(out)
}

fn monster(input: &str) -> String {
    let expression = match MonsterExpression::compile(input) {
        Ok(expression) => expression,
        Err(error) => return format!("ERR {error}\n"),
    };
    let settings: HashMap<&str, MonsterValue> = HashMap::from([
        ("DoJiggle", MonsterValue::from_boolean(true)),
        ("Count", MonsterValue::from_number(3.0)),
        ("Label", MonsterValue::from_text("Hunting")),
    ]);
    let lookup = |name: &str| settings.get(name).cloned();
    let context = MonsterExpressionContext {
        name: "Drudge Lurker",
        type_id: 1234,
        species: "Drudge",
        maximum_health: 450,
        range: 8.5,
        has_shield: true,
        meta_state: "Hunting",
        setting: Some(&lookup),
    };
    let mut out = String::new();
    writeln!(
        out,
        "source {} dynamic={}",
        esc(expression.source()),
        bool_text(expression.is_dynamic())
    )
    .unwrap();
    match expression.try_evaluate(&context) {
        Ok(value) => writeln!(
            out,
            "value {} {}",
            kind_name(value.kind),
            esc(&value.to_string())
        ),
        Err(error) => writeln!(out, "eval-error {error}"),
    }
    .unwrap();
    match expression.is_match(&context) {
        Ok(matched) => writeln!(out, "match={}", bool_text(matched)),
        Err(error) => writeln!(out, "match=False error={error}"),
    }
    .unwrap();
    out
}

fn ub() -> String {
    let catalog = UbSettingCatalog::new(UbSettingValueBag::default(), Vec::new());
    let mut out = String::new();
    for d in definitions::all()
        .iter()
        .chain(definitions::accepted_only())
    {
        let row = catalog.require(&d.name).unwrap();
        let choices: Vec<String> = d
            .choices
            .iter()
            .map(|c| format!("{}:{}", c.value, c.label))
            .collect();
        writeln!(
            out,
            "{}|{:?}|{:?}|{}|{}|{}|{}|{}|{}",
            d.name,
            d.kind,
            d.scope,
            d.category(),
            esc(&d.default.to_storage_string()),
            esc(&d.display(&d.default)),
            esc(&catalog.full_display_value(&row)),
            esc(&d.summary),
            choices.join(",")
        )
        .unwrap();
    }
    writeln!(out, "categories {}", catalog.categories().join(",")).unwrap();
    for (spelling, _) in definitions::ORIGINAL_SPELLINGS {
        writeln!(
            out,
            "alias {spelling} -> {}",
            catalog.require(spelling).unwrap().name()
        )
        .unwrap();
    }
    for probe in [
        "nametags.enabled",
        "AutoXp.Policy",
        "Plugin.SettingsProfile",
        "No.Such",
    ] {
        writeln!(out, "kind {probe} {:?}", definitions::kind_of(probe)).unwrap();
    }
    out
}

fn ub_parse(input: &str) -> String {
    let (kind, text) = input.split_once('|').unwrap();
    let kind = match kind {
        "Bool" => UbSettingKind::Bool,
        "Int" => UbSettingKind::Int,
        "Single" => UbSettingKind::Single,
        "Double" => UbSettingKind::Double,
        "String" => UbSettingKind::String,
        "Enum" => UbSettingKind::Enum,
        "Color" => UbSettingKind::Color,
        "Collection" => UbSettingKind::Collection,
        other => panic!("unknown kind {other}"),
    };
    match UbSettingValue::try_parse(kind, text) {
        None => "FAIL\n".to_owned(),
        Some(value) => format!(
            "OK {:?} {} int={} color={:08X} items={}\n",
            value.kind,
            esc(&value.to_storage_string()),
            value.as_int32(),
            value.as_color(),
            value.items.len()
        ),
    }
}

fn run(kind: &str, input: &str) -> String {
    let result = match kind {
        "defaults" => Ok(format!("{}\n", show(&defaults::create().render()))),
        "create-new" => profile::create_new(&mut AllSettings::default())
            .map(|database| format!("{}\n", show(&database.render())))
            .map_err(|e| e.0),
        "usd" => usd(input),
        "monster" => Ok(monster(input)),
        "ub" => Ok(ub()),
        "ubparse" => Ok(ub_parse(input)),
        other => panic!("unknown case kind {other}"),
    };
    result.unwrap_or_else(|error| format!("!! {error}\n"))
}

// Source: tests/AcDream.Plugins.MossTank.Tests/RustPortGolden.cs :: WriteGoldenData (the cases.out it writes is the expected one)
#[test]
fn every_case_matches_the_csharp_plugin() {
    let cases = std::fs::read_to_string(golden_dir().join("cases.txt")).unwrap();
    let expected = std::fs::read_to_string(golden_dir().join("cases.out")).unwrap();
    let mut actual = String::new();
    let mut full = String::new();
    for raw in cases.lines() {
        if raw.is_empty() || raw.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = raw.splitn(3, " ::: ").collect();
        let (kind, name) = (parts[0], parts[1]);
        let field = parts.get(2).copied().unwrap_or("");
        let input = match field.strip_prefix('@') {
            Some(path) => fixtures::read(path),
            None => unescape(field),
        };
        let result = run(kind, &input);
        writeln!(actual, "##### {kind} {name}").unwrap();
        actual.push_str(&if is_large(kind, field) {
            hashed(&result)
        } else {
            result.clone()
        });
        actual.push('\n');
        writeln!(full, "##### {kind} {name}").unwrap();
        full.push_str(&result);
        full.push('\n');
    }
    let expected = expected.replace("\r\n", "\n");
    if actual != expected {
        let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/golden");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("settings.rust.out"), &full).unwrap();
        std::fs::write(target.join("settings.rust.hashed.out"), &actual).unwrap();
        let split = |text: &str| -> Vec<(String, String)> {
            text.split("##### ")
                .skip(1)
                .map(|block| {
                    let (head, body) = block.split_once('\n').unwrap_or((block, ""));
                    (head.to_owned(), body.to_owned())
                })
                .collect()
        };
        let expected_blocks = split(&expected);
        let actual_blocks = split(&actual);
        let failing: Vec<&str> = expected_blocks
            .iter()
            .zip(&actual_blocks)
            .filter(|(e, a)| e != a)
            .map(|(e, _)| e.0.as_str())
            .collect();
        panic!(
            "{} of {} cases differ from the C# plugin (full output in target/golden/settings.rust.out): {failing:#?}",
            failing.len(),
            expected_blocks.len()
        );
    }
}
