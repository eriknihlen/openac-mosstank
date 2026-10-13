//! The spell table: `SpellId.g.cs` (a C# enum, one member per spell with
//! its name in a doc comment) converted to Rust constants and a table.

use std::fmt::Write as _;

use crate::{repository_root, workspace_root};

const SOURCE: &str = "src/AcDream.Plugins.MossTank/SpellId.g.cs";
const TARGET: &str = "crates/mosstank-data/src/spell_id.rs";

/// One enum member.
#[derive(Debug, PartialEq, Eq)]
struct Member {
    ident: String,
    id: u32,
    name: Option<String>,
}

fn parse(source: &str) -> Result<Vec<Member>, String> {
    let mut members = Vec::new();
    let mut name: Option<String> = None;
    for (number, raw) in source.lines().enumerate() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("/// <summary>") {
            let text = rest
                .strip_suffix("</summary>")
                .ok_or_else(|| format!("line {}: unterminated summary", number + 1))?;
            name = Some(decode_xml(text));
        } else if let Some((ident, value)) = line.split_once(" = ") {
            let hex = value
                .trim_end_matches(',')
                .strip_prefix("0x")
                .ok_or_else(|| format!("line {}: expected a hex value", number + 1))?;
            let id = u32::from_str_radix(hex, 16)
                .map_err(|error| format!("line {}: {error}", number + 1))?;
            members.push(Member {
                ident: ident.to_owned(),
                id,
                name: name.take(),
            });
        }
    }
    Ok(members)
}

fn decode_xml(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The Rust file for `source`.
fn render(source: &str) -> Result<String, String> {
    let members = parse(source)?;
    let mut out = String::new();
    out.push_str(
        "//! Spell ids by name, generated from the C# `SpellId.g.cs` by\n\
         //! `cargo run -p xtask -- spell-ids`. Do not edit.\n\
         //!\n\
         //! The constants keep the C# member names, so code converted from C#\n\
         //! reads the same (`SpellId.AdjaSGift` becomes `spell_id::AdjaSGift`).\n\n\
         #![allow(non_upper_case_globals)]\n\n\
         /// One spell of the table.\n\
         #[derive(Debug, Clone, Copy, PartialEq, Eq)]\n\
         pub struct SpellIdEntry {\n\
         \x20   /// The spell id.\n\
         \x20   pub id: u32,\n\
         \x20   /// The C# member name.\n\
         \x20   pub ident: &'static str,\n\
         \x20   /// The spell's name, where the table has one.\n\
         \x20   pub name: Option<&'static str>,\n\
         }\n\n",
    );
    for member in &members {
        if let Some(name) = &member.name {
            writeln!(out, "/// {name}").expect("write to a string");
        }
        writeln!(
            out,
            "pub const {}: u32 = 0x{:04X};",
            member.ident, member.id
        )
        .expect("write to a string");
    }
    out.push_str(
        "\n/// Every spell, in the table's order.\npub static ALL: &[SpellIdEntry] = &[\n",
    );
    for member in &members {
        let name = match &member.name {
            Some(name) => format!("Some({name:?})"),
            None => "None".to_owned(),
        };
        writeln!(
            out,
            "    SpellIdEntry {{ id: 0x{:04X}, ident: {:?}, name: {name} }},",
            member.id, member.ident
        )
        .expect("write to a string");
    }
    out.push_str("];\n");
    Ok(out)
}

fn generated() -> Result<String, String> {
    let path = repository_root().join(SOURCE);
    let source =
        std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    render(&source)
}

/// Regenerates the Rust table.
pub fn write() -> Result<String, String> {
    let text = generated()?;
    let path = workspace_root().join(TARGET);
    std::fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(format!("wrote {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generated_table_is_up_to_date() {
        let expected = generated().unwrap();
        let committed = std::fs::read_to_string(workspace_root().join(TARGET)).unwrap();
        assert!(
            committed.replace("\r\n", "\n") == expected,
            "{TARGET} is stale: run `cargo run -p xtask -- spell-ids`"
        );
    }

    #[test]
    fn members_parse_with_and_without_a_name() {
        let source = "    /// <summary>Adja&apos;s Gift</summary>\n    AdjaSGift = 0x0010,\n    Bare = 0x0A,\n";
        assert_eq!(
            parse(source).unwrap(),
            vec![
                Member {
                    ident: "AdjaSGift".into(),
                    id: 0x10,
                    name: Some("Adja's Gift".into())
                },
                Member {
                    ident: "Bare".into(),
                    id: 0x0A,
                    name: None
                },
            ]
        );
    }
}
