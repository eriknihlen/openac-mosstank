//! MossTank as an openAC plugin.
//!
//! The conversion from the C# plugin (`../src/AcDream.Plugins.MossTank`)
//! goes slice by slice: the pure parts live in `mosstank-formats`,
//! `mosstank-expr` and `mosstank-data`; this crate is the plugin shell that
//! will tie them to the client. For now it only starts and says so.

use openac_plugin::{Host, Plugin, StartInfo};

/// The plugin's version, shown to the player.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The plugin.
#[derive(Debug, Default)]
pub struct MossTank;

impl Plugin for MossTank {
    fn enable(&mut self, host: &mut Host<'_>, _info: &StartInfo) -> Result<(), String> {
        host.info(&format!("MossTank {VERSION} (Rust conversion in progress)"));
        Ok(())
    }
}

openac_plugin::export_plugin!(MossTank);

#[cfg(test)]
mod tests {
    use openac_plugin::testing::FakeHost;

    use super::*;

    #[test]
    fn enabling_logs_the_version() {
        let mut host = FakeHost::new("acdream.mosstank");
        let mut plugin = MossTank;
        host.enable(&mut plugin).unwrap();
        assert!(host.logs.iter().any(|(_, line)| line.contains(VERSION)));
    }
}
