//! The peer features the macro asks the other clients for
//! (`MossTankPanel.Peers`): the UB tab's sharing and command switches, the
//! vitals helper and the network readout, the meta's own calls to
//! `netclients[]` and `netcasts[]`, and the calls an expression made in the
//! last five seconds. One lease is held for all of them, replaced when what
//! is needed changes.

use mosstank_formats::meta::MetaProfile;
use mosstank_meta::services::PeerNeed;
use mosstank_ub::catalog::UbCatalog;
use mosstank_ub::hud;
use mosstank_ub::network::{PeerFeatureSubscriptions, capabilities, meta_peer_requirements};

use super::{Core, TankCx};
use crate::client::TankClient;

/// How long an expression's call keeps its peers asked for, milliseconds.
const EXPRESSION_PEERS_MS: i64 = 5_000;

/// The peer features in force.
#[derive(Debug, Default)]
pub(crate) struct PeerState {
    subscriptions: PeerFeatureSubscriptions,
    /// What the meta's enabled rules call for, and which profile it was
    /// read from (the engine's profile generation: the C# reads it again
    /// when the profile is another object).
    meta_requirements: u32,
    requirements_profile: Option<u64>,
    /// What expressions asked for, kept until `expression_peers_until_ms`
    /// while the meta is off.
    dynamic_requirements: u32,
    expression_peers_until_ms: i64,
}

impl Core {
    /// `UpdatePeerSubscriptions`. `profile` is the meta in force with its
    /// generation, or `None` inside the meta's own call, where it is the one
    /// last read. `readout_shown` is whether the network readout's window
    /// is open.
    pub(crate) fn update_peer_subscriptions(
        &mut self,
        client: &mut dyn TankClient,
        catalog: &dyn UbCatalog,
        meta_enabled: bool,
        profile: Option<(u64, &MetaProfile)>,
        readout_shown: bool,
    ) {
        if !client.rules().is_available() {
            self.peers.subscriptions.dispose(client.ub());
            self.peers.dynamic_requirements = capabilities::NONE;
            return;
        }
        if let Some((generation, profile)) = profile
            && self.peers.requirements_profile != Some(generation)
        {
            self.peers.requirements_profile = Some(generation);
            self.peers.meta_requirements = meta_peer_requirements(profile);
        }
        let mut requested = capabilities::NONE;
        if catalog.flag("Sharing.Vitals") {
            requested |= capabilities::CLIENT_STATE | capabilities::CASTS;
        }
        if catalog.flag("Networking.ReceiveCommands") {
            requested |= capabilities::COMMANDS;
        }
        // The network readout is on screen when it is on and either its
        // window is open or it may stay with the window shut (`ShouldShow`,
        // register PEER-2).
        let readout = hud::read_settings(catalog);
        let readout_shows = readout.enabled && (readout_shown || readout.show_hud_when_closed);
        let vitals = &self.settings.vitals;
        if (client.meta().has_ui() && readout_shows)
            || (self.combat.enabled()
                && vitals.enabled
                && vitals.help_others
                && vitals.help_network_peers)
        {
            requested |= capabilities::CLIENT_STATE;
        }
        let now = client.clock().utc_unix_ms;
        if !meta_enabled && now >= self.peers.expression_peers_until_ms {
            self.peers.dynamic_requirements = capabilities::NONE;
        }
        if meta_enabled {
            requested |= self.peers.meta_requirements;
        }
        requested |= self.peers.dynamic_requirements;
        self.peers.subscriptions.update(client.ub(), requested);
    }

    /// `RequireExpressionPeers`: an expression's call keeps what it needs
    /// asked for for five seconds.
    pub(crate) fn require_expression_peers(
        &mut self,
        client: &mut dyn TankClient,
        catalog: &dyn UbCatalog,
        meta_enabled: bool,
        readout_shown: bool,
        need: PeerNeed,
    ) {
        self.peers.dynamic_requirements |= match need {
            PeerNeed::ClientState => capabilities::CLIENT_STATE,
            PeerNeed::Casts => capabilities::CASTS,
        };
        self.peers.expression_peers_until_ms = client.clock().utc_unix_ms + EXPRESSION_PEERS_MS;
        self.update_peer_subscriptions(client, catalog, meta_enabled, None, readout_shown);
    }

    /// `MetaPeersReady`: the meta waits for the other clients only when
    /// what it needs is shared over a hub that is not connected yet. A
    /// host with no hub hands out no lease, and its peers need none
    /// (register PEER-1).
    pub(crate) fn meta_peers_ready(&self, client: &mut dyn TankClient) -> bool {
        !self.peers.subscriptions.holds_lease()
            || (self.peers.meta_requirements | self.peers.dynamic_requirements)
                == capabilities::NONE
            || client.ub().peers_connected()
    }

    /// The lease goes back with the plugin.
    pub(crate) fn release_peer_subscriptions(&mut self, client: &mut dyn TankClient) {
        self.peers.subscriptions.dispose(client.ub());
    }
}

impl TankCx<'_> {
    /// `UpdatePeerSubscriptions` on the frame.
    pub(crate) fn update_peer_subscriptions(&mut self) {
        let meta_enabled = self.meta.enabled();
        self.core.update_peer_subscriptions(
            self.client,
            &self.ub.catalog,
            meta_enabled,
            Some((self.meta.profile_generation(), self.meta.profile())),
            self.ub.hud.shown(),
        );
    }
}

impl PeerState {
    /// A session's end drops what its expressions asked for.
    pub(crate) fn forget_expression_requirements(&mut self) {
        self.dynamic_requirements = capabilities::NONE;
    }
}

impl super::Tank {
    /// What is asked of the other clients now (`PluginPeerCapabilities`).
    #[must_use]
    pub fn requested_peer_features(&self) -> u32 {
        self.core.peers.subscriptions.current()
    }
}
