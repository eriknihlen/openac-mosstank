//! The test census: every C# test (a `[Fact]` or `[Theory]` method) with
//! the Rust tests that port it, found by their `// Source:` comments, or
//! "not yet". Written to `tests/census.md`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::{repository_root, workspace_root};

const CSHARP_TESTS: &str = "tests/AcDream.Plugins.MossTank.Tests";
const SOURCE_PREFIX: &str = "// Source: tests/AcDream.Plugins.MossTank.Tests/";
const TARGET: &str = "tests/census.md";

/// The C# tests: file name to its test methods, in file order.
type CSharpTests = BTreeMap<String, Vec<String>>;

/// The Rust counterparts: (C# file, C# method) to the Rust tests
/// (`path :: fn`).
type Counterparts = BTreeMap<(String, String), Vec<String>>;

fn identifier_before_paren(line: &str) -> Option<&str> {
    let head = &line[..line.find('(')?];
    let start = head
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
        .map_or(0, |index| index + 1);
    let name = &head[start..];
    (!name.is_empty()).then_some(name)
}

/// The test methods of one C# file: each `[Fact]` or `[Theory]` names the
/// next public or internal method declared after it.
fn csharp_test_methods(source: &str) -> Result<Vec<String>, String> {
    let mut methods = Vec::new();
    let mut pending: Option<usize> = None;
    for (number, raw) in source.lines().enumerate() {
        let line = raw.trim();
        if line.starts_with("[Fact") || line.starts_with("[Theory") {
            if let Some(previous) = pending {
                return Err(format!(
                    "line {}: a test attribute with no method",
                    previous + 1
                ));
            }
            pending = Some(number);
        } else if pending.is_some()
            && (line.starts_with("public ") || line.starts_with("internal "))
            && line.contains('(')
        {
            let name = identifier_before_paren(line)
                .ok_or_else(|| format!("line {}: no method name", number + 1))?;
            methods.push(name.to_owned());
            pending = None;
        }
    }
    match pending {
        Some(number) => Err(format!(
            "line {}: a test attribute with no method",
            number + 1
        )),
        None => Ok(methods),
    }
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn sorted_entries(folder: &Path) -> Result<Vec<PathBuf>, String> {
    let mut entries = std::fs::read_dir(folder)
        .map_err(|error| format!("{}: {error}", folder.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{}: {error}", folder.display()))?;
    entries.sort();
    Ok(entries)
}

fn csharp_tests() -> Result<CSharpTests, String> {
    let mut tests = CSharpTests::new();
    for path in sorted_entries(&repository_root().join(CSHARP_TESTS))? {
        if path.extension().is_none_or(|extension| extension != "cs") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("{}: not a UTF-8 name", path.display()))?
            .to_owned();
        let methods =
            csharp_test_methods(&read(&path)?).map_err(|error| format!("{name} {error}"))?;
        if !methods.is_empty() {
            tests.insert(name, methods);
        }
    }
    Ok(tests)
}

fn rust_files(folder: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for path in sorted_entries(folder)? {
        if path.is_dir() {
            rust_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

/// The `// Source:` comments under `crates/`, each with the test function
/// declared after it.
fn counterparts() -> Result<Counterparts, String> {
    let root = workspace_root();
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files)?;
    let mut found = Counterparts::new();
    for path in files {
        let relative = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let text = read(&path)?;
        let lines: Vec<&str> = text.lines().collect();
        for (number, raw) in lines.iter().enumerate() {
            let Some(rest) = raw.trim().strip_prefix(SOURCE_PREFIX) else {
                continue;
            };
            let (file, method) = rest.split_once(" :: ").ok_or_else(|| {
                format!("{relative}:{}: expected 'File.cs :: Method'", number + 1)
            })?;
            let method: String = method
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let function = lines[number + 1..]
                .iter()
                .find_map(|line| {
                    let line = line.trim();
                    let rest = line
                        .strip_prefix("fn ")
                        .or_else(|| line.strip_prefix("pub fn "))?;
                    identifier_before_paren(rest)
                })
                .ok_or_else(|| {
                    format!("{relative}:{}: no function after the comment", number + 1)
                })?;
            found
                .entry((file.to_owned(), method))
                .or_default()
                .push(format!("`{relative}` :: `{function}`"));
        }
    }
    Ok(found)
}

/// The census text.
///
/// # Errors
/// A file cannot be read, or a `// Source:` comment names a C# test that
/// does not exist.
pub fn generated() -> Result<String, String> {
    let tests = csharp_tests()?;
    let found = counterparts()?;

    for (file, method) in found.keys() {
        if !tests
            .get(file)
            .is_some_and(|methods| methods.contains(method))
        {
            return Err(format!(
                "a // Source: comment names {file} :: {method}, which is not a C# test"
            ));
        }
    }

    let ported =
        |file: &str, method: &str| found.contains_key(&(file.to_owned(), method.to_owned()));
    let count = |file: &str, methods: &[String]| {
        methods.iter().filter(|method| ported(file, method)).count()
    };
    let total: usize = tests.values().map(Vec::len).sum();
    let ported_total: usize = tests
        .iter()
        .map(|(file, methods)| count(file, methods))
        .sum();
    let files_touched = tests
        .iter()
        .filter(|(file, methods)| count(file, methods) > 0)
        .count();

    let mut out = String::new();
    out.push_str("# C# test census\n\n");
    out.push_str(
        "Every C# test (`[Fact]` or `[Theory]` method) in\n\
         `tests/AcDream.Plugins.MossTank.Tests/` and the Rust tests that port it,\n\
         found by their `// Source:` comments. Generated by\n\
         `cargo run -p xtask -- census`; do not edit by hand.\n\n",
    );
    let _ = writeln!(
        out,
        "Ported: **{ported_total} of {total}** C# tests, from {files_touched} of {} files.\n",
        tests.len()
    );
    out.push_str("| C# file | Tests | Ported |\n|---|---:|---:|\n");
    for (file, methods) in &tests {
        let _ = writeln!(
            out,
            "| {file} | {} | {} |",
            methods.len(),
            count(file, methods)
        );
    }
    for (file, methods) in &tests {
        let _ = writeln!(out, "\n## {file}\n");
        for method in methods {
            match found.get(&(file.clone(), method.clone())) {
                Some(rust) => {
                    let _ = writeln!(out, "- {method}: {}", rust.join(", "));
                }
                None => {
                    let _ = writeln!(out, "- {method}: not yet");
                }
            }
        }
    }
    Ok(out)
}

/// Rewrites `tests/census.md`.
///
/// # Errors
/// As [`generated`], or the file cannot be written.
pub fn write() -> Result<String, String> {
    let text = generated()?;
    let path = workspace_root().join(TARGET);
    std::fs::write(&path, &text).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(format!("wrote {TARGET}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_census_is_up_to_date() {
        let expected = generated().unwrap();
        let committed = std::fs::read_to_string(workspace_root().join(TARGET)).unwrap();
        assert!(
            committed.replace("\r\n", "\n") == expected,
            "{TARGET} is stale: run `cargo run -p xtask -- census`"
        );
    }

    #[test]
    fn test_methods_are_found_after_their_attributes() {
        let source = "    [Fact]\n    public void One() { }\n\n    [Theory]\n    [InlineData(\"a(\")]\n    public async Task Two(string a) { }\n    public void Helper() { }\n";
        assert_eq!(csharp_test_methods(source).unwrap(), vec!["One", "Two"]);
    }
}
