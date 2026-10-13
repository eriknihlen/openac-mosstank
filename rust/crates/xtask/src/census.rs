//! The test census: every C# test (a `[Fact]` or `[Theory]` method) with
//! the Rust tests that port it, found by their `// Source:` comments; "n/a"
//! with the reason, for a test of .NET mechanics or of a C# path the plugin
//! does not have, found by a `// Not applicable:` comment; or "not yet".
//! Written to `tests/census.md`.
//!
//! The comments sit in any Rust file under `crates/`, beside the tests of
//! the same area. A `// Source:` comment names the test function declared
//! after it; a `// Not applicable:` comment stands alone and ends with the
//! reason, after a colon:
//!
//! ```text
//! // Source: tests/AcDream.Plugins.MossTank.Tests/File.cs :: Method
//! fn the_rust_test() {}
//!
//! // Not applicable: tests/AcDream.Plugins.MossTank.Tests/File.cs :: Method: why
//! ```

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::{repository_root, workspace_root};

const CSHARP_TESTS: &str = "tests/AcDream.Plugins.MossTank.Tests";
const SOURCE_PREFIX: &str = "// Source: tests/AcDream.Plugins.MossTank.Tests/";
const NOT_APPLICABLE_PREFIX: &str = "// Not applicable: tests/AcDream.Plugins.MossTank.Tests/";
const TARGET: &str = "tests/census.md";

/// The C# tests: file name to its test methods, in file order.
type CSharpTests = BTreeMap<String, Vec<String>>;

/// The Rust counterparts: (C# file, C# method) to the Rust tests
/// (`path :: fn`).
type Counterparts = BTreeMap<(String, String), Vec<String>>;

/// The C# tests marked not applicable: (C# file, C# method) to the reason.
type NotApplicable = BTreeMap<(String, String), String>;

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

/// A comment's `File.cs :: Method` and what follows the method's name.
fn file_and_method<'a>(
    rest: &'a str,
    relative: &str,
    number: usize,
) -> Result<(String, String, &'a str), String> {
    let (file, method) = rest
        .split_once(" :: ")
        .ok_or_else(|| format!("{relative}:{}: expected 'File.cs :: Method'", number + 1))?;
    let end = method
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(method.len());
    Ok((file.to_owned(), method[..end].to_owned(), &method[end..]))
}

/// The `// Source:` comments under `crates/`, each with the test function
/// declared after it, and the `// Not applicable:` comments with their
/// reasons.
fn counterparts() -> Result<(Counterparts, NotApplicable), String> {
    let root = workspace_root();
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files)?;
    let mut found = Counterparts::new();
    let mut not_applicable = NotApplicable::new();
    for path in files {
        let relative = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let text = read(&path)?;
        let lines: Vec<&str> = text.lines().collect();
        for (number, raw) in lines.iter().enumerate() {
            let line = raw.trim();
            if let Some(rest) = line.strip_prefix(NOT_APPLICABLE_PREFIX) {
                let (file, method, after) = file_and_method(rest, &relative, number)?;
                let reason = after
                    .strip_prefix(':')
                    .map(str::trim)
                    .filter(|reason| !reason.is_empty())
                    .ok_or_else(|| {
                        format!("{relative}:{}: expected ':: Method: reason'", number + 1)
                    })?;
                if not_applicable
                    .insert((file.clone(), method.clone()), reason.to_owned())
                    .is_some()
                {
                    return Err(format!(
                        "{relative}:{}: {file} :: {method} is marked not applicable twice",
                        number + 1
                    ));
                }
                continue;
            }
            let Some(rest) = line.strip_prefix(SOURCE_PREFIX) else {
                continue;
            };
            let (file, method, _) = file_and_method(rest, &relative, number)?;
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
                .entry((file, method))
                .or_default()
                .push(format!("`{relative}` :: `{function}`"));
        }
    }
    Ok((found, not_applicable))
}

/// The census text.
///
/// # Errors
/// A file cannot be read; a comment names a C# test that does not exist;
/// or a C# test is both ported and marked not applicable.
pub fn generated() -> Result<String, String> {
    let tests = csharp_tests()?;
    let (found, not_applicable) = counterparts()?;

    let exists = |file: &str, method: &str| {
        tests
            .get(file)
            .is_some_and(|methods| methods.iter().any(|known| known == method))
    };
    for (file, method) in found.keys() {
        if !exists(file, method) {
            return Err(format!(
                "a // Source: comment names {file} :: {method}, which is not a C# test"
            ));
        }
    }
    for (file, method) in not_applicable.keys() {
        if !exists(file, method) {
            return Err(format!(
                "a // Not applicable: comment names {file} :: {method}, which is not a C# test"
            ));
        }
        if found.contains_key(&(file.clone(), method.clone())) {
            return Err(format!(
                "{file} :: {method} is both ported and marked not applicable"
            ));
        }
    }

    let key = |file: &str, method: &str| (file.to_owned(), method.to_owned());
    let count = |file: &str, methods: &[String], marks: &dyn Fn(&(String, String)) -> bool| {
        methods
            .iter()
            .filter(|method| marks(&key(file, method)))
            .count()
    };
    let ported = |entry: &(String, String)| found.contains_key(entry);
    let skipped = |entry: &(String, String)| not_applicable.contains_key(entry);
    let total: usize = tests.values().map(Vec::len).sum();
    let ported_total: usize = tests
        .iter()
        .map(|(file, methods)| count(file, methods, &ported))
        .sum();
    let skipped_total: usize = tests
        .iter()
        .map(|(file, methods)| count(file, methods, &skipped))
        .sum();
    let files_touched = tests
        .iter()
        .filter(|(file, methods)| count(file, methods, &ported) > 0)
        .count();

    let mut out = String::new();
    out.push_str("# C# test census\n\n");
    out.push_str(
        "Every C# test (`[Fact]` or `[Theory]` method) in\n\
         `tests/AcDream.Plugins.MossTank.Tests/` and the Rust tests that port it,\n\
         found by their `// Source:` comments, or why it does not apply, from a\n\
         `// Not applicable:` comment. Generated by\n\
         `cargo run -p xtask -- census`; do not edit by hand.\n\n",
    );
    let _ = writeln!(
        out,
        "Ported: **{ported_total} of {total}** C# tests, from {files_touched} of {} files. \
         Not applicable: **{skipped_total}**. Not yet: **{}**.\n",
        tests.len(),
        total - ported_total - skipped_total
    );
    out.push_str("| C# file | Tests | Ported | N/A |\n|---|---:|---:|---:|\n");
    for (file, methods) in &tests {
        let _ = writeln!(
            out,
            "| {file} | {} | {} | {} |",
            methods.len(),
            count(file, methods, &ported),
            count(file, methods, &skipped)
        );
    }
    for (file, methods) in &tests {
        let _ = writeln!(out, "\n## {file}\n");
        for method in methods {
            let entry = key(file, method);
            if let Some(rust) = found.get(&entry) {
                let _ = writeln!(out, "- {method}: {}", rust.join(", "));
            } else if let Some(reason) = not_applicable.get(&entry) {
                let _ = writeln!(out, "- {method}: n/a: {reason}");
            } else {
                let _ = writeln!(out, "- {method}: not yet");
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

    #[test]
    fn a_comment_names_its_file_and_method_and_keeps_what_follows() {
        let (file, method, after) =
            file_and_method("A.cs :: TheTest: a reason here", "x.rs", 0).unwrap();
        assert_eq!(
            (file.as_str(), method.as_str(), after),
            ("A.cs", "TheTest", ": a reason here")
        );
        assert!(file_and_method("A.cs TheTest", "x.rs", 0).is_err());
    }
}
