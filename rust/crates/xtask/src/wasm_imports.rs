//! The built plugin's imports: a WebAssembly plugin reaches the client
//! only through the SDK's `openac` import module, so a release build that
//! imports anything else (another module of the client, the C runtime's
//! `env`, a WASI module) would load in no client. `scripts/check.sh` runs
//! this on the module it has just built.

use std::path::Path;

use crate::workspace_root;

/// The one module a plugin imports from.
const CLIENT_MODULE: &str = "openac";
/// The module `scripts/check.sh` builds.
const BUILT_MODULE: &str = "target/wasm32-unknown-unknown/release/mosstank.wasm";

/// One import of a module: where from, and its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub module: String,
    pub name: String,
}

/// A reader over a module's bytes.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn byte(&mut self) -> Result<u8, String> {
        let byte = *self
            .bytes
            .get(self.at)
            .ok_or_else(|| format!("the module ends at byte {}", self.at))?;
        self.at += 1;
        Ok(byte)
    }

    /// An unsigned LEB128 number of at most 32 bits.
    fn u32(&mut self) -> Result<u32, String> {
        let mut value: u32 = 0;
        for shift in (0..35).step_by(7) {
            let byte = self.byte()?;
            value |= u32::from(byte & 0x7F)
                .checked_shl(shift)
                .ok_or("a number past 32 bits")?;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err("a number past 32 bits".to_owned())
    }

    /// An unsigned LEB128 number of at most 64 bits, skipped.
    fn skip_u64(&mut self) -> Result<(), String> {
        for _ in 0..10 {
            if self.byte()? & 0x80 == 0 {
                return Ok(());
            }
        }
        Err("a number past 64 bits".to_owned())
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], String> {
        let end = self
            .at
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| format!("the module ends inside {length} bytes at {}", self.at))?;
        let taken = &self.bytes[self.at..end];
        self.at = end;
        Ok(taken)
    }

    fn name(&mut self) -> Result<String, String> {
        let length = usize::try_from(self.u32()?).map_err(|error| error.to_string())?;
        String::from_utf8(self.take(length)?.to_vec()).map_err(|error| error.to_string())
    }

    /// A table's or a memory's limits.
    fn skip_limits(&mut self) -> Result<(), String> {
        let flags = self.byte()?;
        self.skip_u64()?;
        if flags & 1 != 0 {
            self.skip_u64()?;
        }
        Ok(())
    }
}

/// Every import of a WebAssembly module, in order.
///
/// # Errors
/// The bytes are not a module this reader understands.
pub fn imports(module: &[u8]) -> Result<Vec<Import>, String> {
    let mut reader = Reader {
        bytes: module,
        at: 0,
    };
    if reader.take(4)? != b"\0asm" {
        return Err("not a WebAssembly module".to_owned());
    }
    if reader.take(4)? != [1, 0, 0, 0] {
        return Err("not a version 1 WebAssembly module".to_owned());
    }
    let mut found = Vec::new();
    while reader.at < module.len() {
        let id = reader.byte()?;
        let size = usize::try_from(reader.u32()?).map_err(|error| error.to_string())?;
        let body = reader.take(size)?;
        if id != 2 {
            continue;
        }
        let mut section = Reader { bytes: body, at: 0 };
        for _ in 0..section.u32()? {
            let module = section.name()?;
            let name = section.name()?;
            match section.byte()? {
                // A function, by its type; a tag, by its attribute and type.
                0 => {
                    section.u32()?;
                }
                4 => {
                    section.byte()?;
                    section.u32()?;
                }
                // A table: its element type, then its limits.
                1 => {
                    section.byte()?;
                    section.skip_limits()?;
                }
                2 => section.skip_limits()?,
                // A global: its value type and mutability.
                3 => {
                    section.byte()?;
                    section.byte()?;
                }
                kind => return Err(format!("import {module}.{name} has kind {kind}")),
            }
            found.push(Import { module, name });
        }
    }
    Ok(found)
}

/// The imports of `module` that do not come from the client's module.
///
/// # Errors
/// The bytes are not a module this reader understands.
pub fn foreign_imports(module: &[u8]) -> Result<Vec<Import>, String> {
    Ok(imports(module)?
        .into_iter()
        .filter(|import| import.module != CLIENT_MODULE)
        .collect())
}

fn check_module(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let all = imports(&bytes)?;
    let foreign = foreign_imports(&bytes)?;
    if !foreign.is_empty() {
        let names: Vec<String> = foreign
            .iter()
            .map(|import| format!("{}.{}", import.module, import.name))
            .collect();
        return Err(format!(
            "{} imports from outside the `{CLIENT_MODULE}` module: {}",
            path.display(),
            names.join(", ")
        ));
    }
    Ok(format!(
        "{BUILT_MODULE}: {} imports, all from `{CLIENT_MODULE}`",
        all.len()
    ))
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankManifestTests.cs :: TheBuiltPluginFolderShipsNoClientAssemblyBesideThePlugin
// Source: tests/AcDream.Plugins.MossTank.Tests/PluginApiBoundaryTests.cs :: ThePluginReferencesTheContractAssemblyAndNoOtherClientAssembly
/// The built plugin reaches the client through the SDK's one import
/// module and nothing else: the WebAssembly form of the C#'s two checks
/// that the plugin references only the contract assembly and ships no
/// client assembly beside it. Run by `scripts/check.sh` on the module it
/// has just built (`cargo run -p xtask -- wasm-imports`).
///
/// # Errors
/// The module is missing, does not read, or imports from another module.
pub fn check_built_module_imports_only_the_client() -> Result<String, String> {
    check_module(&workspace_root().join(BUILT_MODULE))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A module importing `openac.log` (a function), `env.memory` (a
    /// memory with a maximum) and `wasi.clock` (an immutable i64 global),
    /// with a custom section before the imports.
    fn module() -> Vec<u8> {
        let mut imports = vec![3];
        for (module, name, desc) in [
            ("openac", "log", vec![0x00, 0x00]),
            ("env", "memory", vec![0x02, 0x01, 0x01, 0x80, 0x01]),
            ("wasi", "clock", vec![0x03, 0x7E, 0x00]),
        ] {
            imports.push(u8::try_from(module.len()).unwrap());
            imports.extend_from_slice(module.as_bytes());
            imports.push(u8::try_from(name.len()).unwrap());
            imports.extend_from_slice(name.as_bytes());
            imports.extend(desc);
        }
        let mut bytes = b"\0asm\x01\0\0\0".to_vec();
        bytes.extend([0, 4, 1, b'x', 0xAA, 0xBB]);
        bytes.push(2);
        bytes.push(u8::try_from(imports.len()).unwrap());
        bytes.extend(imports);
        bytes
    }

    #[test]
    fn every_import_is_read_with_its_module() {
        let found = imports(&module()).unwrap();
        let names: Vec<(&str, &str)> = found
            .iter()
            .map(|import| (import.module.as_str(), import.name.as_str()))
            .collect();
        assert_eq!(
            names,
            [("openac", "log"), ("env", "memory"), ("wasi", "clock")]
        );
    }

    #[test]
    fn imports_from_another_module_are_foreign() {
        let foreign = foreign_imports(&module()).unwrap();
        assert_eq!(foreign.len(), 2);
        assert!(foreign.iter().all(|import| import.module != "openac"));
    }

    #[test]
    fn a_truncated_module_is_an_error() {
        let bytes = module();
        assert!(imports(&bytes[..bytes.len() - 2]).is_err());
        assert!(imports(b"\0asn\x01\0\0\0").is_err());
    }
}
