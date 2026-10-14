//! The autostart (`MossTankAutostart`): the session's settings name the
//! profiles, the log channels, the meta switch and whether the macro
//! starts, and they are applied once per time in the world, through the
//! same paths the Profiles tab and `/vt start` use.

use mosstank_expr::ExpressionValue;

use super::options::parse_bool;
use super::{Tank, TankCx};
use crate::client::TankClient;

impl Tank {
    /// One frame of the autostart (`TickAutostart`).
    pub fn tick_autostart(&mut self, client: &mut dyn TankClient) {
        self.begin_call(client);
        let (mut cx, _) = self.cx(client);
        cx.tick_autostart();
        cx.core.write_out(cx.client);
        self.end_call(client);
    }
}

impl TankCx<'_> {
    fn tick_autostart(&mut self) {
        if !self.client.rules().is_available() {
            // Dropped out of the world: the next arrival is a new generation.
            self.core.autostart_applied_for_current_session = false;
            return;
        }
        if self.core.autostart_applied_for_current_session {
            return;
        }
        // The character's own profile set settles first: the autostart
        // waits for a load under way (register PROF-1).
        if self.core.staged_load.is_some() {
            return;
        }
        // The client can report itself available before the character
        // arrives, and a set applied before the name lands is thrown away
        // when the name binds the character's own choices. Wait for it.
        if self.client.rules().character().name.is_empty() {
            return;
        }
        self.core.autostart_applied_for_current_session = true;
        let settings = self.client.session_settings();
        if settings.is_empty() {
            return;
        }
        // Settle the character's own binding first.
        self.ensure_character_profile();
        self.finish_staged_load();
        self.apply_autostart(&settings);
    }

    /// `ApplyAutostart`.
    fn apply_autostart(&mut self, settings: &[(String, String)]) {
        let get = |key: &str| {
            settings
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.as_str())
        };
        self.apply_named_profile(get("settingsProfile"), "settings profile", Kind::Settings);
        self.apply_named_profile(get("metaProfile"), "Meta profile", Kind::Meta);
        self.apply_named_profile(get("navProfile"), "navigation profile", Kind::Route);
        self.apply_named_profile(get("lootProfile"), "loot profile", Kind::Loot);

        // Before the macro: a channel switched on after the first pass has
        // already missed every once-per-run line.
        if let Some(channels) = get("logChannels").filter(|value| !value.trim().is_empty()) {
            self.core.apply_log_channels(channels);
        }
        if let Some(enable_meta) = get("enableMeta").and_then(parse_bool)
            && let Err(error) =
                self.set_meta_option("enablemeta", &ExpressionValue::Boolean(enable_meta))
        {
            self.client.looter().log_error(&format!(
                "Autostart: failed to set EnableMeta. {}",
                error.message()
            ));
        }
        if get("startMacro").and_then(parse_bool) == Some(true) && !self.core.combat.enabled() {
            self.core.set_macro_running(self.client, true);
        }
    }

    /// `ApplyNamedProfile`.
    fn apply_named_profile(&mut self, name: Option<&str>, label: &str, kind: Kind) {
        let Some(name) = name.filter(|name| !name.trim().is_empty()) else {
            return;
        };
        let core = &mut *self.core;
        let stores = self.client.stores();
        let exists = match kind {
            Kind::Settings => core.profiles.exists(stores, Some(name)),
            Kind::Meta => core.meta_profiles.exists(stores, Some(name)),
            Kind::Route => core.route_profiles.exists(stores, Some(name)),
            Kind::Loot => core.loot_profiles.exists(stores, Some(name)),
        };
        match exists {
            Ok(true) => {}
            Ok(false) => {
                self.client
                    .looter()
                    .log_error(&format!("Autostart: {label} '{name}' was not found."));
                return;
            }
            Err(error) => {
                self.client.looter().log_error(&format!(
                    "Autostart: failed to select {label} '{name}'. {error}"
                ));
                return;
            }
        }
        match kind {
            Kind::Settings => self.select_profile(name),
            Kind::Meta => self.select_meta_profile(name),
            Kind::Route => self.select_route_profile(name),
            Kind::Loot => self.select_loot_profile(name),
        }
    }
}

/// Which store a named profile is looked for in.
#[derive(Debug, Clone, Copy)]
enum Kind {
    Settings,
    Meta,
    Route,
    Loot,
}
