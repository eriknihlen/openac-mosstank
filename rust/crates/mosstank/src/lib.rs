//! MossTank as an openAC plugin.
//!
//! The conversion from the C# plugin (`../src/AcDream.Plugins.MossTank`)
//! goes slice by slice: the pure parts live in `mosstank-formats`,
//! `mosstank-expr`, `mosstank-data`, `mosstank-settings` and `mosstank-stores`;
//! this crate is the plugin shell that will tie them to the client. For now
//! it starts and says so, gives the stores the client's storage
//! ([`storage`]), the rules their view of the client ([`automation`]), and
//! registers MossTank's loot classifier for the other plugins ([`loot`]).

pub mod automation;
pub mod loot;
pub mod storage;

pub mod meta_host;
pub mod nav_host;
pub mod vendor_host;

use ac_plugin_abi::callback::{Callback, CallbackAnswer};
use openac_plugin::{Host, Plugin, StartInfo};

/// The plugin's version, shown to the player.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The plugin.
#[derive(Debug, Default)]
pub struct MossTank {
    /// MossTank's loot classifier, as the client's other plugins ask it.
    loot_classifier: loot::LootClassifierService,
    /// The live loot rules the classifier judges with (the loot profile's,
    /// once the plugin shell holds one).
    loot_rules: Vec<mosstank_formats::utl::LootRule>,
}

impl Plugin for MossTank {
    fn enable(&mut self, host: &mut Host<'_>, _info: &StartInfo) -> Result<(), String> {
        host.info(&format!("MossTank {VERSION} (Rust conversion in progress)"));
        if let Err(error) = self.loot_classifier.register(host) {
            host.warn(&format!(
                "MossTank's loot classifier is not registered: {error}"
            ));
        }
        Ok(())
    }

    fn disable(&mut self, host: &mut Host<'_>) {
        self.loot_classifier.unregister(host);
    }

    fn callback(&mut self, host: &mut Host<'_>, callback: Callback) -> CallbackAnswer {
        self.loot_classifier
            .answer(host, &self.loot_rules, &callback)
            .unwrap_or_default()
    }
}

openac_plugin::export_plugin!(MossTank);

#[cfg(test)]
mod tests {
    use ac_plugin_abi::calls;
    use openac_plugin::testing::FakeHost;

    use super::*;

    #[test]
    fn enabling_logs_the_version() {
        let mut host = FakeHost::new("acdream.mosstank");
        let mut plugin = MossTank::default();
        host.enable(&mut plugin).unwrap();
        assert!(host.logs.iter().any(|(_, line)| line.contains(VERSION)));
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPluginTests.cs :: EnableRegistersTheLootClassifierAndDisableRevokesIt
    #[test]
    fn enable_registers_the_loot_classifier_and_disable_revokes_it() {
        let mut host = FakeHost::new("acdream.mosstank");
        host.set_reply::<calls::LootClassifierRegister>(&Ok(3));
        let mut plugin = MossTank::default();
        assert!(host.requests::<calls::LootClassifierRegister>().is_empty());

        host.enable(&mut plugin).unwrap();

        let registered = host.requests::<calls::LootClassifierRegister>();
        assert_eq!(registered.len(), 1);
        assert_eq!(registered[0].id, "moss-tank");
        assert_eq!(registered[0].display_name, "MossTank");
        assert!(
            host.requests::<calls::LootClassifierUnregister>()
                .is_empty()
        );

        host.disable(&mut plugin);

        assert_eq!(host.requests::<calls::LootClassifierUnregister>(), [3]);
    }
}
