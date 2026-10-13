//! The route and meta formats against the C# plugin's own results: every
//! case in `golden/cases.txt` is read (and written back in both formats)
//! here and compared with `golden/cases.out`, which the C# plugin wrote
//! (`tests/AcDream.Plugins.MossTank.Tests/RustPortGolden.cs`). For a
//! fixture only the length and FNV-1a hash of each part are kept; on a
//! mismatch this test writes its full output to
//! `target/golden/cases.rust.out` to compare with the C# full output in
//! `target/golden/cases.full.out` (written by the C# run).
//!
//! Source: the C# readers and writers as a black box, run over the cases.

use std::fmt::Write as _;
use std::path::PathBuf;

use mosstank_dotnet::compat::number::{to_string_f32, to_string_f64};
use mosstank_formats::meta::{MetaAction, MetaCondition, MetaProfile};
use mosstank_formats::route::{NavigationSettings, NoSpells, Position};
use mosstank_formats::{af, fixtures, met, nav};

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

fn r(value: f64) -> String {
    to_string_f64(value)
}

fn pos(p: &Position) -> String {
    format!(
        "{:08X},{},{},{},{},{}",
        p.cell_id,
        r(p.east_west),
        r(p.north_south),
        r(p.elevation),
        to_string_f32(p.heading_degrees),
        if p.is_outdoor { "True" } else { "False" }
    )
}

fn bool_text(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

fn dump_condition(out: &mut String, c: &MetaCondition, depth: usize) {
    writeln!(
        out,
        "{}C {} t={} s={} n={} n2={} n3={}",
        " ".repeat(depth),
        c.kind.name(),
        esc(&c.text),
        esc(&c.secondary_text),
        r(c.number),
        r(c.secondary_number),
        r(c.tertiary_number)
    )
    .unwrap();
    for child in &c.children {
        dump_condition(out, child, depth + 1);
    }
}

fn dump_action(out: &mut String, a: &MetaAction, depth: usize) {
    writeln!(
        out,
        "{}A {} t={} s={} n={} n2={}",
        " ".repeat(depth),
        a.kind.name(),
        esc(&a.text),
        esc(&a.secondary_text),
        r(a.number),
        r(a.secondary_number)
    )
    .unwrap();
    if let Some(route) = &a.embedded_route {
        dump_route(out, route, depth + 1);
    }
    for child in &a.children {
        dump_action(out, child, depth + 1);
    }
}

fn dump_route(out: &mut String, n: &NavigationSettings, depth: usize) {
    writeln!(
        out,
        "{}route mode={} follow={:08X} {} count={}",
        " ".repeat(depth),
        n.mode.name(),
        n.follow_target_object_id,
        esc(&n.follow_target_name),
        n.waypoints.len()
    )
    .unwrap();
    for w in &n.waypoints {
        writeln!(
            out,
            "{}wp {} pos={} ref={} id={:08X} name={} class={} valid={} text={} dur={} recall={} rid={} rname={} jh={} js={} jc={} jd={}",
            " ".repeat(depth + 1),
            w.kind.name(),
            pos(&w.position),
            pos(&w.reference_position),
            w.object_id,
            esc(&w.object_name),
            w.legacy_object_class,
            bool_text(w.legacy_reference_valid),
            esc(&w.text),
            w.duration_milliseconds,
            w.recall.name(),
            w.recall_spell_id,
            esc(&w.recall_spell_name),
            r(w.jump_heading_degrees),
            bool_text(w.jump_hold_shift),
            w.jump_charge_milliseconds,
            w.jump_direction.name()
        )
        .unwrap();
    }
}

fn dump_profile(profile: &MetaProfile) -> String {
    let mut out = String::new();
    for rule in &profile.rules {
        writeln!(
            out,
            "rule state={} enabled={}",
            esc(&rule.state),
            bool_text(rule.enabled)
        )
        .unwrap();
        dump_condition(&mut out, &rule.condition, 1);
        dump_action(&mut out, &rule.action, 1);
    }
    out
}

fn try_text(result: Result<String, String>) -> String {
    result.unwrap_or_else(|error| format!("!! {error}"))
}

fn strip_header(af: String) -> String {
    // As the C# side cuts it: the route header up to its first "NAV:"
    // (which is inside the header's keyword list).
    let empty_route = af::save_nav(&NavigationSettings::with_mode(
        mosstank_formats::route::RouteMode::Once,
    ))
    .unwrap();
    let nav_prefix = &empty_route[..empty_route.find("NAV:").unwrap()];
    if let Some(rest) = af.strip_prefix(nav_prefix) {
        format!("<nav-header>{rest}")
    } else if let Some(rest) = af.strip_prefix(af::META_HEADER) {
        format!("<meta-header>{rest}")
    } else {
        af
    }
}

fn run(kind: &str, input: &str) -> String {
    let mut out = String::new();
    match kind {
        "met" | "af-meta" => {
            let loaded = if kind == "met" {
                met::try_load(input, Some(&NoSpells))
            } else {
                af::try_load_meta(input, Some(&NoSpells))
            };
            let profile = match loaded {
                Ok(profile) => profile,
                Err(error) => return format!("ERR {error}\n"),
            };
            out.push_str(&dump_profile(&profile));
            writeln!(out, "--- met:\n{}", show(&try_text(met::save(&profile)))).unwrap();
            writeln!(
                out,
                "--- af:\n{}",
                show(&try_text(af::save_meta(&profile, false).map(strip_header)))
            )
            .unwrap();
        }
        "nav" | "af-nav" => {
            let mut route = NavigationSettings::default();
            let loaded = if kind == "nav" {
                nav::try_load(input, &mut route, Some(&NoSpells))
            } else {
                af::try_load_nav(input, &mut route, Some(&NoSpells))
            };
            if let Err(error) = loaded {
                return format!("ERR {error}\n");
            }
            dump_route(&mut out, &route, 0);
            writeln!(out, "--- nav:\n{}", show(&try_text(nav::save(&route)))).unwrap();
            writeln!(
                out,
                "--- af:\n{}",
                show(&try_text(af::save_nav(&route).map(strip_header)))
            )
            .unwrap();
        }
        other => panic!("unknown case kind {other}"),
    }
    out
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
        let fixture = field.strip_prefix('@');
        let input = match fixture {
            Some(path) => fixtures::read(path),
            None => unescape(field),
        };
        let result = run(kind, &input);
        writeln!(actual, "##### {kind} {name}").unwrap();
        actual.push_str(&if fixture.is_some() {
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
        std::fs::write(target.join("cases.rust.out"), &full).unwrap();
        std::fs::write(target.join("cases.rust.hashed.out"), &actual).unwrap();
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
            "{} of {} cases differ from the C# plugin (full output in target/golden/cases.rust.out): {failing:#?}",
            failing.len(),
            expected_blocks.len()
        );
    }
}
