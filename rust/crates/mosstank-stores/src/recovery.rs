//! A profile that would not load is kept: its text is copied into the
//! plugin's own storage under `recovery/<family>/`, named by a hash of its
//! key and content, so a later save cannot destroy the only copy.

use mosstank_dotnet::text::casing;
use sha2::{Digest, Sha256};

use crate::storage::{Root, StoreHost};

/// `Convert.ToHexString(SHA256.HashData(UTF8(text)))`: upper-case hex.
#[must_use]
pub fn sha256_hex(text: &str) -> String {
    hex_upper(&Sha256::digest(text.as_bytes()))
}

/// Bytes as `Convert.ToHexString` writes them.
#[must_use]
pub fn hex_upper(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02X}");
    }
    out
}

/// The key a corrupt profile's text is preserved under.
#[must_use]
pub fn recovery_key(family: &str, key: &str, content: &str) -> String {
    let identity = sha256_hex(&format!("{key}\n{content}"));
    format!(
        "recovery/{}/{}.txt",
        casing::to_lower_invariant(family),
        &identity[..16]
    )
}

/// Keeps a profile that would not load, and says so: the summary line, and
/// where the text went when it could be kept. Nothing is kept without
/// storage or without text.
pub fn preserve(
    host: &mut dyn StoreHost,
    family: &str,
    key: &str,
    content: Option<&str>,
    error: &str,
) -> String {
    preserve_reporting(host, family, key, content, error).notice
}

/// What [`preserve_reporting`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preserved {
    /// The line that says so.
    pub notice: String,
    /// Whether the original is safe to write over: it was copied aside, or
    /// there was no text to lose.
    pub kept: bool,
}

/// [`preserve`], saying also whether the copy was made, so a caller can
/// leave the original alone when it was not.
pub fn preserve_reporting(
    host: &mut dyn StoreHost,
    family: &str,
    key: &str,
    content: Option<&str>,
    error: &str,
) -> Preserved {
    let summary = format!("{family} profile '{key}' could not be loaded: {error}");
    let not_kept = |notice: String| Preserved {
        notice,
        kept: false,
    };
    let Some(content) = content.filter(|text| !text.is_empty()) else {
        return Preserved {
            notice: summary,
            kept: true,
        };
    };
    let storage = host.storage(Root::Plugin);
    if !storage.is_available() {
        return not_kept(summary);
    }
    let recovery = recovery_key(family, key, content);
    let payload = format!("Original key: {key}\nLoad error: {error}\n\n{content}");
    match storage.write_text(&recovery, &payload) {
        Ok(()) => Preserved {
            notice: format!("{summary} Raw data was preserved as {recovery}."),
            kept: true,
        },
        Err(backup_error) => {
            host.log().warn(&format!(
                "MossTank could not preserve corrupt {family} profile: {backup_error}"
            ));
            not_kept(summary)
        }
    }
}
