//! The `/vt` commands (`MossTankCommands`): the macro's own words, the
//! profile verbs, the options, the meta's state and expressions, the route's
//! waypoints, the log channels and the death verbs. `/ub` answers through
//! the UB tab's dispatcher ([`mosstank_ub::commands`]).
//!
//! The verbs that drive the UB tools or the panel's editors (the jumps,
//! the route-point verbs, the item, monster, spell and pet tests, the
//! property and table dumps, the game database) answer that they are not
//! converted yet; they come with MT-10 and MT-11.

use mosstank_expr::ExpressionValue;
use mosstank_formats::meta::MetaProfile;
use mosstank_formats::route::{RouteMode, RouteStepBackBlock};
use mosstank_meta::engine::{
    DECISION_INTERVAL_SECONDS, DEFAULT_STATE_NAME, MAXIMUM_INTERVAL_MILLISECONDS,
    MINIMUM_INTERVAL_MILLISECONDS,
};
use mosstank_settings::options;
use mosstank_ub::commands::{split_head, strip_extension};
use openac_plugin_compat::number::{self, NumberStyles};
use openac_plugin_compat::text::{casing, ordinal};

use super::options::{declared_clr_type_name, try_parse_option_value};
use super::rules::route_turn;
use super::{CHANNELS, Core, TankCx, channel_name, diagnostics};
use crate::client::TankClient;

/// The macro's own command word.
pub(crate) const VTANK_VERB: &str = "vt";

/// The UtilityBelt command word.
pub(crate) const UB_VERB: &str = "ub";

/// What `/vt` answers to a word it does not know.
pub(crate) const UNKNOWN_VTANK_COMMAND: &str = "Unknown /vt command. Use /vt help.";

/// The macro's verb lists (`VtankHelp`).
const VTANK_HELP: [&str; 4] = [
    "MossTank /vt \u{2014} profiles: settings nav navaf loot meta metaaf opt testitem propertydump addnavpt refresh getdb gamedb addnavjump addnavcheckpoint",
    "MossTank /vt \u{2014} actions: start stop forcebuff cancelforcebuff setmetastate fakedeath deathrestore deletemonster reverseroute reverseroutequery equipitemsfor mexec metainterval nextwp prevwp echo tapjump jump setattackbar",
    "MossTank /vt \u{2014} game info: dumpspells dumpspecies dumpmats dumpskills",
    "MossTank /vt \u{2014} debug: log testmonster lockdump dumptracker clearlocks clearbusy listmonstervariables dumpmetavars listmetafunctions metafunchelp fakeimp pscount testspell testpet",
];

/// The log channels the reference lists.
const LOG_TYPES: &str = "Log types: ActiveRule SalvageList SpellCast RuleInfo Timers CastInfo DebuffChoice Loot CharProps Misc BusyState";

/// `/vt meta load <name>` or `/vt metaaf load <name>`, read from a line: the
/// name and whether it is the `.af` form.
pub(crate) fn meta_load_request(input: &str) -> Option<(String, bool)> {
    let (typed, arguments) = split_head(input);
    let verb = casing::to_lower_invariant(&typed);
    let af = match verb.as_str() {
        "meta" => false,
        "metaaf" => true,
        _ => return None,
    };
    let (operation, name) = split_head(&arguments);
    (ordinal::equals_ignore_case(&operation, "load") && !name.is_empty()).then_some((name, af))
}

/// Runs one line typed under `/vt` (`ExecuteVtankCommand`).
#[allow(clippy::too_many_lines)]
pub(crate) fn execute_vtank(cx: &mut TankCx<'_>, input: &str) {
    let (typed, arguments) = split_head(input);
    let verb = casing::to_lower_invariant(&typed);
    match verb.as_str() {
        "" | "help" => vtank_help(cx.client, &arguments),
        "ub" => execute_ub(cx, ""),
        "start" => {
            if cx.core.combat.enabled() {
                Core::write_vtank(cx.client, "Macro is already running.");
            } else {
                cx.core.set_macro_running(cx.client, true);
            }
        }
        "stop" => {
            if cx.core.combat.enabled() {
                cx.core.set_macro_running(cx.client, false);
            }
            Core::write_vtank(cx.client, "Macro stopped.");
        }
        "forcebuff" => cx.core.force_buff(cx.client),
        "cancelforcebuff" => cx.core.cancel_force_buff(cx.client),
        "settings" => settings_command(cx, &arguments),
        "navaf" => route_profile_command(cx, &arguments, true),
        "nav" => route_profile_command(cx, &arguments, false),
        "loot" => loot_profile_command(cx, &arguments),
        "metaaf" => meta_profile_command(cx, &arguments, true),
        "meta" => meta_profile_command(cx, &arguments, false),
        "opt" => option_command(cx, &arguments),
        "nextwp" => next_waypoint_command(cx, &arguments),
        "prevwp" => previous_waypoint_command(cx, &arguments),
        "metainterval" => meta_interval_command(cx, &arguments),
        "setmetastate" => set_meta_state_from_command(cx, &arguments),
        "mexec" => execute_expression(cx, &arguments),
        "echo" => Core::write_vtank(cx.client, &arguments),
        "setattackbar" => set_attack_bar(cx, &arguments),
        "reverseroute" => {
            cx.core.navigation.toggle_reverse();
            let reversing = pascal(cx.core.navigation.reversing());
            Core::write_vtank(cx.client, &format!("Setting nav backwards to: {reversing}"));
        }
        "reverseroutequery" => {
            let reversing = pascal(cx.core.navigation.reversing());
            Core::write_vtank(cx.client, &format!("Nav backwards is: {reversing}"));
        }
        "deletemonster" => delete_selected_monster(cx.client),
        "listmonstervariables" => {
            Core::write_vtank(cx.client, "Monster expression variables you can use:");
            Core::write_vtank(
                cx.client,
                "true, false, name, typeid, species, maxhp, range, hasshield, metastate, setting_<OptionName>",
            );
        }
        "deathrestore" => cx.restore_after_death(),
        // The reference's verb runs the whole death: the meta edge and the
        // macro stop.
        "fakedeath" => fake_death(cx),
        "pscount" => {
            let count = cx.core.command_portal_count;
            Core::write_vtank(cx.client, &format!("Portal space toggle count: {count}"));
        }
        "refresh" => {
            super::panel::monsters::ensure_default_monster_rule(cx);
            cx.refresh_item_editors();
            cx.refresh_loot_editor(false);
            cx.refresh_route_editor();
            cx.refresh_meta_editor();
            Core::write_vtank(cx.client, "Settings pages reloaded.");
        }
        "getdb" => cx.core.start_game_info_update_command(cx.client),
        "gamedb" => cx.core.game_db_command(cx.client, &arguments),
        "log" => log_command(cx, &arguments),
        "lockdump" => {
            let automation = cx.client.rules();
            let line = format!(
                "Action busy: magic={}, items={}, equipment={}",
                pascal(automation.is_casting()),
                pascal(automation.items_busy()),
                pascal(automation.equipment_busy())
            );
            Core::write_vtank(cx.client, &line);
        }
        "dumptracker" => dump_object_tracker(cx.client),
        "clearlocks" => {
            cx.core.clear_action_locks(cx.client);
            Core::write_vtank(cx.client, "Action locks cleared.");
        }
        "fakeimp" => fake_imperil(cx),
        "dumpmats" => dump_materials(cx.client),
        "tapjump" => cx.tap_jump(),
        "jump" => cx.jump_command(&arguments, false),
        "addnavjump" => cx.jump_command(&arguments, true),
        "addnavpt" => cx.add_command_route_point(&arguments, false),
        "addnavcheckpoint" => cx.add_command_route_point(&arguments, true),
        "equipitemsfor" => diagnostics::equip_items_for(cx, &arguments),
        "testitem" => diagnostics::test_selected_item(cx),
        "propertydump" => diagnostics::dump_selected_properties(cx.client),
        "testmonster" => diagnostics::test_selected_monster(cx),
        "testspell" => diagnostics::test_spell(cx.client, &arguments),
        "testpet" => diagnostics::test_pet(cx),
        "dumpmetavars" => diagnostics::dump_meta_variables(cx),
        "listmetafunctions" => diagnostics::list_meta_functions(cx),
        "metafunchelp" => diagnostics::meta_function_help(cx, &arguments),
        "clearbusy" => diagnostics::clear_busy(cx.client),
        "dumpspells" => diagnostics::dump_spells(cx.client),
        "dumpspecies" => diagnostics::dump_species(cx.client),
        "dumpskills" => diagnostics::dump_skills(cx.client),
        _ => Core::write_vtank(cx.client, UNKNOWN_VTANK_COMMAND),
    }
    cx.core.write_out(cx.client);
}

/// A failure the C# throws out of a `/vt` command ends it there: its
/// dispatcher's catch says why in chat and logs it (`ExecuteVtankCommand`).
pub(crate) fn command_failed(client: &mut dyn TankClient, error: &str) {
    Core::write_vtank(client, &format!("Command failed: {error}"));
    client
        .looter()
        .log_error(&format!("MossTank /vt command failed. {error}"));
}

/// .NET's `bool.ToString()`.
fn pascal(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

/// `HandleVtankHelpCommand`.
fn vtank_help(client: &mut dyn TankClient, arguments: &str) {
    let requested = arguments.trim();
    if !requested.is_empty() {
        let Some(usage) = mosstank_ub::help::try_get_vtank(requested) else {
            Core::write_vtank(client, &format!("No help found for command: {requested}"));
            return;
        };
        Core::write_vtank(client, &format!("Syntax: {}", usage.usage));
        Core::write_vtank(client, usage.summary);
        return;
    }
    for line in VTANK_HELP {
        Core::write_vtank(client, line);
    }
    let documented: Vec<&str> = mosstank_ub::help::VTANK_ENTRIES
        .iter()
        .map(|entry| entry.name)
        .collect();
    Core::write_vtank(
        client,
        &format!(
            "MossTank /vt \u{2014} documented: {}",
            documented.join(", ")
        ),
    );
    Core::write_vtank(
        client,
        "For help with a specific command, use /vt help [command].",
    );
    Core::write_vtank(
        client,
        "UtilityBelt commands answer on /ub; type /ub help for them.",
    );
}

/// `/vt settings [save/load/savechar/loadchar] name`.
fn settings_command(cx: &mut TankCx<'_>, arguments: &str) {
    let (operation, name) = split_head(arguments);
    let operation = casing::to_lower_invariant(&operation);
    if name.is_empty()
        || !matches!(
            operation.as_str(),
            "save" | "load" | "savechar" | "loadchar"
        )
    {
        Core::write_vtank(
            cx.client,
            "Syntax: /vt settings [save/load/savechar/loadchar] [filename]",
        );
        return;
    }
    let name = strip_extension(&name, &[".usd", ".settings"]);
    if operation == "save" || operation == "savechar" {
        let core = &mut *cx.core;
        if operation == "savechar"
            && let Err(error) = core.profiles.set_mine_only(cx.client.stores(), true)
        {
            command_failed(cx.client, &error);
            return;
        }
        let created = core.profiles.create(
            cx.client.stores(),
            Some(&name),
            true,
            &mut core.settings,
            &mut core.no_buff_item_names,
            &mut core.command_log_types,
        );
        match created {
            Ok((made, notice)) => {
                if made {
                    cx.reset_profile_consumers();
                }
                Core::write_vtank(cx.client, &notice);
            }
            Err(error) => command_failed(cx.client, &error),
        }
        return;
    }
    let selected = match cx.core.profiles.select(cx.client.stores(), Some(&name)) {
        Ok(selected) => selected,
        Err(error) => return command_failed(cx.client, &error),
    };
    if !selected {
        Core::write_vtank(
            cx.client,
            &format!("Settings profile '{name}' was not found."),
        );
        return;
    }
    if cx.load_selected_profile() == Some(mosstank_stores::profile_store::ProfileLoad::Failed) {
        let notice = cx.core.profile_lifecycle_notice.clone();
        Core::write_vtank(cx.client, &notice);
        return;
    }
    let selected = cx.core.profiles.selected();
    Core::write_vtank(cx.client, &format!("Loaded settings profile {selected}."));
}

/// `/vt nav [save/load] name`, and `/vt navaf`.
fn route_profile_command(cx: &mut TankCx<'_>, arguments: &str, af: bool) {
    let (operation, name) = split_head(arguments);
    let operation = casing::to_lower_invariant(&operation);
    let word = if af { "navaf" } else { "nav" };
    if name.is_empty() || !matches!(operation.as_str(), "save" | "load") {
        Core::write_vtank(
            cx.client,
            &format!("Syntax: /vt {word} [save/load] [filename]"),
        );
        return;
    }
    let bare_name = strip_extension(&name, &[".nav", ".af"]);
    let file_name = if af { format!("{bare_name}.af") } else { name };
    if operation == "save" {
        let created = cx.core.route_profiles.create(
            cx.client.stores(),
            Some(&file_name),
            true,
            &cx.core.settings.navigation,
        );
        match created {
            Ok((_, notice)) => Core::write_vtank(cx.client, &notice),
            Err(error) => command_failed(cx.client, &error),
        }
        return;
    }
    let selected = match cx
        .core
        .route_profiles
        .select(cx.client.stores(), Some(&file_name), af)
    {
        Ok(selected) => selected,
        Err(error) => return command_failed(cx.client, &error),
    };
    if !selected {
        if af {
            Core::write_vtank(
                cx.client,
                &format!("Navigation profile {file_name} was not found."),
            );
            return;
        }
        let spells = super::profiles::SpellsSnapshot::read(cx.client);
        let imported = cx.core.route_profiles.try_import_legacy(
            cx.client.stores(),
            Some(&bare_name),
            &mut cx.core.settings.navigation,
            Some(&spells),
        );
        match imported {
            Ok((true, notice)) => {
                let core = &mut *cx.core;
                let mut turn = route_turn!(cx.client, core);
                core.navigation.reset(&mut turn);
                Core::write_vtank(cx.client, &notice);
            }
            Ok((false, notice)) => Core::write_vtank(cx.client, &notice),
            Err(error) => command_failed(cx.client, &error),
        }
        return;
    }
    cx.load_route_profile();
    match cx.core.route_profiles.selected(cx.client.stores()) {
        Ok(selected) => {
            Core::write_vtank(cx.client, &format!("Loaded navigation profile {selected}."));
        }
        Err(error) => command_failed(cx.client, &error),
    }
}

/// `/vt loot [load/new] name`.
fn loot_profile_command(cx: &mut TankCx<'_>, arguments: &str) {
    let (operation, name) = split_head(arguments);
    let operation = casing::to_lower_invariant(&operation);
    if name.is_empty() || !matches!(operation.as_str(), "new" | "save" | "load") {
        Core::write_vtank(cx.client, "Syntax: /vt loot [load/new] [filename]");
        return;
    }
    let name = strip_extension(&name, &[".utl", ".json"]);
    if operation == "new" || operation == "save" {
        let loot = &mut cx.core.settings.inventory.loot;
        let rules = loot.rules.clone();
        let created = cx.core.loot_profiles.create(
            cx.client.stores(),
            Some(&name),
            operation == "save",
            &rules,
            Some(loot),
        );
        match created {
            Ok((made, notice)) => {
                if made {
                    cx.core.load_loot_profile(cx.client);
                }
                Core::write_vtank(cx.client, &notice);
            }
            Err(error) => command_failed(cx.client, &error),
        }
        return;
    }
    let exists = match cx
        .core
        .loot_profiles
        .exists(cx.client.stores(), Some(&name))
    {
        Ok(exists) => exists,
        Err(error) => return command_failed(cx.client, &error),
    };
    if exists {
        cx.select_loot_profile(&name);
        let notice = cx.core.loot_editor_notice.clone();
        Core::write_vtank(cx.client, &notice);
        return;
    }
    let loot = &mut cx.core.settings.inventory.loot;
    let mut rules = std::mem::take(&mut loot.rules);
    let imported = cx.core.loot_profiles.try_import_legacy(
        cx.client.stores(),
        Some(&name),
        &mut rules,
        Some(loot),
    );
    cx.core.settings.inventory.loot.rules = rules;
    match imported {
        Ok((true, notice)) => {
            let core = &mut *cx.core;
            let lent = super::rules::lent_loot!(core);
            lent.loot.reset(cx.client.looter());
            drop(lent);
            Core::write_vtank(cx.client, &notice);
        }
        Ok((false, notice)) => Core::write_vtank(cx.client, &notice),
        Err(error) => command_failed(cx.client, &error),
    }
}

impl Core {
    /// The load half of `/vt meta load <name>` (`HandleMetaProfileCommand`):
    /// the profile now in force, or `None` when none was loaded; the lines
    /// are said here.
    pub(crate) fn load_meta_profile_command(
        &mut self,
        client: &mut dyn TankClient,
        name: &str,
        af: bool,
    ) -> Option<MetaProfile> {
        let name = strip_extension(name, &[".json"]);
        let bare_name = strip_extension(&name, &[".met", ".af"]);
        let file_name = if af { format!("{bare_name}.af") } else { name };
        let selected = match self
            .meta_profiles
            .select(client.stores(), Some(&file_name), af)
        {
            Ok(selected) => selected,
            Err(error) => {
                command_failed(client, &error);
                return None;
            }
        };
        if selected {
            let profile = self.load_meta_profile_current(client);
            // A file that could not be read has already been announced.
            if self.meta_profiles.last_load_error().is_none() {
                match self.meta_profiles.selected(client.stores()) {
                    Ok(selected) => {
                        Core::write_vtank(client, &format!("Loaded Meta profile {selected}."));
                    }
                    Err(error) => command_failed(client, &error),
                }
            }
            return Some(profile);
        }
        if af {
            Core::write_vtank(client, &format!("Meta profile {file_name} was not found."));
            return None;
        }
        let legacy = match self
            .meta_profiles
            .legacy_import_exists(client.stores(), Some(&bare_name))
        {
            Ok(legacy) => legacy,
            Err(error) => {
                command_failed(client, &error);
                return None;
            }
        };
        if !legacy {
            // The reference loads a meta that does not exist by creating it,
            // empty, and switching to it.
            let created = self.meta_profiles.create(
                client.stores(),
                Some(&file_name),
                false,
                &MetaProfile::default(),
            );
            match created {
                Ok((true, _)) => {}
                Ok((false, notice)) => {
                    Core::write_vtank(client, &notice);
                    return None;
                }
                Err(error) => {
                    command_failed(client, &error);
                    return None;
                }
            }
            let profile = self.load_meta_profile_current(client);
            match self.meta_profiles.selected(client.stores()) {
                Ok(selected) => Core::write_vtank(
                    client,
                    &format!("Meta profile {selected} did not exist; started it empty."),
                ),
                Err(error) => command_failed(client, &error),
            }
            return Some(profile);
        }
        let spells = super::profiles::SpellsSnapshot::read(client);
        match self
            .meta_profiles
            .try_import_legacy(client.stores(), Some(&bare_name), Some(&spells))
        {
            Ok((true, profile, notice)) => {
                Core::write_vtank(client, &notice);
                Some(profile)
            }
            Ok((false, _, notice)) => {
                Core::write_vtank(client, &notice);
                None
            }
            Err(error) => {
                command_failed(client, &error);
                None
            }
        }
    }
}

/// `/vt meta [save/load] name`, and `/vt metaaf`.
fn meta_profile_command(cx: &mut TankCx<'_>, arguments: &str, af: bool) {
    let (operation, name) = split_head(arguments);
    let operation = casing::to_lower_invariant(&operation);
    let word = if af { "metaaf" } else { "meta" };
    if name.is_empty() || !matches!(operation.as_str(), "save" | "load") {
        Core::write_vtank(
            cx.client,
            &format!("Syntax: /vt {word} [save/load] [filename]"),
        );
        return;
    }
    if operation == "load" {
        if let Some(profile) = cx.core.load_meta_profile_command(cx.client, &name, af) {
            cx.with_engine(|engine, host, services| {
                engine.replace_profile(host, services, profile)
            });
            cx.after_meta_load();
        }
        return;
    }
    let name = strip_extension(&name, &[".json"]);
    let bare_name = strip_extension(&name, &[".met", ".af"]);
    let file_name = if af { format!("{bare_name}.af") } else { name };
    let current = cx.meta.profile().clone();
    let created =
        cx.core
            .meta_profiles
            .create(cx.client.stores(), Some(&file_name), true, &current);
    let notice = match created {
        Ok((_, notice)) => notice,
        Err(error) => return command_failed(cx.client, &error),
    };
    cx.load_meta_profile();
    Core::write_vtank(cx.client, &notice);
}

/// `/vt opt [list/get/set/setinall/toggle]`.
fn option_command(cx: &mut TankCx<'_>, arguments: &str) {
    let (operation, tail) = split_head(arguments);
    match casing::to_lower_invariant(&operation).as_str() {
        "list" => {
            if !tail.is_empty() {
                Core::write_vtank(cx.client, "Syntax: /vt opt list");
                return;
            }
            let names = &*options::COMMAND_NAMES;
            Core::write_vtank(cx.client, &format!("Available options: ({})", names.len()));
            for chunk in names.chunks(4) {
                Core::write_vtank(cx.client, &format!("   {}", chunk.join("   ")));
            }
        }
        "toggle" => toggle_option(cx, &tail),
        "get" => {
            let Some(canonical) = options::canonical(&tail) else {
                Core::write_vtank(cx.client, "Option get: Invalid option specified.");
                return;
            };
            let value = cx.get_meta_option(canonical).to_display_string();
            Core::write_vtank(cx.client, &format!("Option {canonical} = {value}"));
        }
        "set" | "setinall" => {
            let (name, raw_value) = split_head(&tail);
            let Some(canonical) = options::canonical(&name) else {
                Core::write_vtank(cx.client, "Option set: Invalid option specified.");
                return;
            };
            let declared = options::declared_type(canonical);
            let Some(value) = try_parse_option_value(&raw_value, Some(declared)) else {
                Core::write_vtank(
                    cx.client,
                    &format!(
                        "Option set: bad value. {canonical} takes a {}.",
                        declared_clr_type_name(declared)
                    ),
                );
                return;
            };
            if let Err(error) = cx.set_meta_option(canonical, &value) {
                command_failed(cx.client, &error.message());
                return;
            }
            if ordinal::equals_ignore_case(&operation, "setinall") {
                let count = cx.core.profiles.set_option_in_all(
                    cx.client.stores(),
                    canonical,
                    &cx.core.settings,
                );
                match count {
                    Ok(count) => Core::write_vtank(
                        cx.client,
                        &format!(
                            "Done saving setting {canonical} to all profiles. (Changed {count} profiles)"
                        ),
                    ),
                    Err(error) => command_failed(cx.client, &error),
                }
            } else {
                let value = cx.get_meta_option(canonical).to_display_string();
                Core::write_vtank(cx.client, &format!("Set option {canonical} = {value}"));
            }
        }
        _ => Core::write_vtank(cx.client, "Syntax: /vt opt [list/get/set/setinall/toggle]"),
    }
}

/// `ToggleVtankOption`.
fn toggle_option(cx: &mut TankCx<'_>, name: &str) {
    let requested = name.trim();
    if requested.is_empty() {
        Core::write_vtank(cx.client, "Syntax: /vt opt toggle <option>");
        return;
    }
    let Some(canonical) = options::canonical(requested) else {
        Core::write_vtank(cx.client, "Option toggle: Invalid option specified.");
        return;
    };
    if options::declared_type(canonical) != options::VtankSettingValueType::Bool {
        Core::write_vtank(
            cx.client,
            &format!("Unable to toggle setting {canonical}: it is not a switch."),
        );
        return;
    }
    let flipped = !cx.get_meta_option(canonical).is_truthy();
    if let Err(error) = cx.set_meta_option(canonical, &ExpressionValue::Boolean(flipped)) {
        command_failed(cx.client, &error.message());
        return;
    }
    let value = cx.get_meta_option(canonical).to_display_string();
    Core::write_vtank(cx.client, &format!("Set option {canonical} = {value}"));
}

/// A count of 1 or more, by default 1.
fn waypoint_count(arguments: &str) -> Option<i32> {
    let value = arguments.trim();
    if value.is_empty() {
        return Some(1);
    }
    number::parse_i32(value, NumberStyles::INTEGER).filter(|count| *count >= 1)
}

/// `/vt nextwp [count]` (`SkipRouteWaypoints`).
fn next_waypoint_command(cx: &mut TankCx<'_>, arguments: &str) {
    let Some(count) = waypoint_count(arguments) else {
        Core::write_vtank(
            cx.client,
            "Syntax: /vt nextwp [number of waypoints, default 1]",
        );
        return;
    };
    let line = if cx.core.settings.navigation.mode == RouteMode::Target {
        "A follow route has no waypoints to skip.".to_owned()
    } else {
        let core = &mut *cx.core;
        let mut turn = route_turn!(cx.client, core);
        let skipped = core.navigation.skip_waypoints(&mut turn, count);
        if skipped == 0 {
            "The route has no waypoint left to skip.".to_owned()
        } else {
            let noun = if skipped == 1 {
                "waypoint"
            } else {
                "waypoints"
            };
            if core
                .navigation
                .has_nothing_left_to_walk(&core.settings.navigation)
            {
                format!("Skipped {skipped} {noun}; the route is complete.")
            } else {
                format!(
                    "Skipped {skipped} {noun}; now heading for waypoint {}/{}.",
                    core.navigation.current_waypoint_index() + 1,
                    core.settings.navigation.waypoints.len()
                )
            }
        }
    };
    Core::write_vtank(cx.client, &line);
}

/// `/vt prevwp [count]` (`StepBackRouteWaypoints`).
fn previous_waypoint_command(cx: &mut TankCx<'_>, arguments: &str) {
    let Some(count) = waypoint_count(arguments) else {
        Core::write_vtank(
            cx.client,
            "Syntax: /vt prevwp [number of waypoints, default 1]",
        );
        return;
    };
    let core = &mut *cx.core;
    let line = match core.navigation.step_back_block(&core.settings.navigation) {
        RouteStepBackBlock::FollowRoute => {
            "A follow route has no waypoints to step back to.".to_owned()
        }
        RouteStepBackBlock::OnceRoute => "A once-through route cannot step back.".to_owned(),
        RouteStepBackBlock::EmptyRoute => "The route has no waypoints to step back to.".to_owned(),
        RouteStepBackBlock::None => {
            let mut turn = route_turn!(cx.client, core);
            let stepped = core.navigation.step_back_waypoints(&mut turn, count);
            if stepped == 0 {
                if core.navigation.reversing() {
                    "The route is already at its last waypoint, where its walk back began."
                        .to_owned()
                } else {
                    "The route is already at its first waypoint.".to_owned()
                }
            } else {
                let noun = if stepped == 1 {
                    "waypoint"
                } else {
                    "waypoints"
                };
                format!(
                    "Stepped back {stepped} {noun}; now heading for waypoint {}/{}.",
                    core.navigation.current_waypoint_index() + 1,
                    core.settings.navigation.waypoints.len()
                )
            }
        }
    };
    Core::write_vtank(cx.client, &line);
}

/// `/vt metainterval [ms]`.
fn meta_interval_command(cx: &mut TankCx<'_>, arguments: &str) {
    let value = arguments.trim();
    if !value.is_empty() {
        let Some(milliseconds) = number::parse_i32(value, NumberStyles::INTEGER) else {
            Core::write_vtank(
                cx.client,
                &format!(
                    "Syntax: /vt metainterval [{MINIMUM_INTERVAL_MILLISECONDS}-{MAXIMUM_INTERVAL_MILLISECONDS} ms]"
                ),
            );
            return;
        };
        cx.core
            .profiles
            .set_meta_interval_milliseconds(cx.client.stores(), milliseconds);
        cx.apply_meta_interval();
    }
    #[allow(clippy::cast_possible_truncation)]
    let vtank = (DECISION_INTERVAL_SECONDS * 1000.0).round() as i32;
    let interval = cx.meta.interval_milliseconds();
    Core::write_vtank(
        cx.client,
        &format!("Meta is checked every {interval} ms (VTank: {vtank} ms)."),
    );
}

/// `/vt setmetastate <state>`.
pub(crate) fn set_meta_state_from_command(cx: &mut TankCx<'_>, state: &str) {
    if state.is_empty() {
        Core::write_vtank(cx.client, "Syntax: /vt setmetastate [somestate]");
        Core::write_vtank(cx.client, "State names are case sensitive.");
        return;
    }
    let target = cx
        .meta
        .states()
        .into_iter()
        .find(|value| value == state)
        .unwrap_or_else(|| DEFAULT_STATE_NAME.to_owned());
    if target != state {
        Core::write_vtank(
            cx.client,
            "That state is not used by this meta; using the default instead.",
        );
    }
    cx.with_engine(|engine, host, services| engine.transition(host, services, &target));
    cx.meta
        .current_state()
        .clone_into(&mut cx.core.settings.combat.meta_state);
    let current = cx.meta.current_state().to_owned();
    Core::write_vtank(cx.client, &format!("Meta state is now {current}."));
}

/// `/vt mexec <expression>`.
pub(crate) fn execute_expression(cx: &mut TankCx<'_>, source: &str) {
    Core::write_vtank(
        cx.client,
        &format!("MExec evaluating expression: \"{}\"", source.trim()),
    );
    let result = cx.with_engine(|engine, host, services| {
        engine
            .evaluate(host, services, source)
            .map(|value| engine.display(host, &value))
    });
    match result {
        Ok(text) => Core::write_vtank(cx.client, &format!("Result: {text}")),
        Err(error) => {
            Core::write_vtank(cx.client, &format!("Expression error: {}", error.message()))
        }
    }
}

/// `/vt setattackbar <0..1>`.
fn set_attack_bar(cx: &mut TankCx<'_>, value: &str) {
    let power = number::parse_f32(value, NumberStyles::FLOAT)
        // As `power is < 0f or > 1f`: NaN is neither, so it is taken.
        .filter(|power| !(*power < 0.0 || *power > 1.0));
    let Some(power) = power else {
        Core::write_vtank(cx.client, "Syntax: /vt setattackbar [0 to 1]");
        return;
    };
    cx.core.settings.combat.attack_power = power;
    cx.core.save_profile(cx.client);
    let text = number::format_f32(power, "0.###").unwrap_or_default();
    Core::write_vtank(cx.client, &format!("Attack bar set to {text}."));
}

/// `DeleteSelectedMonster`.
fn delete_selected_monster(client: &mut dyn TankClient) {
    let automation = client.rules();
    let selected = automation.selected_object().unwrap_or(0);
    let target = automation
        .hostile_targets(f32::MAX)
        .into_iter()
        .find(|value| value.object_id == selected);
    let Some(target) = target.filter(|target| target.object_id != 0) else {
        Core::write_vtank(client, "Pick a monster first, then /vt deletemonster");
        return;
    };
    let result = automation.dismiss_ghost(selected);
    let line = if result.accepted() {
        format!(
            "Forcing the client to delete {} ({})!!",
            target.name, target.object_id
        )
    } else {
        format!("Unable to delete {}: {:?}", target.name, result.status)
    };
    Core::write_vtank(client, &line);
}

/// `/vt fakedeath`: the meta's death edge and the death handler.
pub(crate) fn fake_death(cx: &mut TankCx<'_>) {
    cx.with_engine(|engine, host, services| engine.trigger_fake_death(host, services));
    let running = cx.core.combat.enabled();
    cx.handle_death(running);
    Core::write_vtank(cx.client, "Fake character death trigger fired.");
}

/// `FakeImperil`.
fn fake_imperil(cx: &mut TankCx<'_>) {
    let automation = cx.client.rules();
    let target = automation.selected_object().unwrap_or(0);
    let is_monster = target != 0
        && automation
            .object(target)
            .is_some_and(|value| value.object_class == ac_plugin_abi::world::ObjectClass::Monster);
    if !is_monster {
        Core::write_vtank(cx.client, "Select a monster first.");
        return;
    }
    cx.core.combat.record_fake_imperil(target);
    Core::write_vtank(cx.client, "Fake cast complete.");
}

/// `DumpObjectTracker`.
fn dump_object_tracker(client: &mut dyn TankClient) {
    let objects = client.looter().objects();
    Core::write_vtank(client, &format!("Object tracker count: {}", objects.len()));
    for item in objects.iter().take(100) {
        Core::write_vtank(
            client,
            &format!(
                "0x{:08X} {:?} {}",
                item.object_id, item.object_class, item.name
            ),
        );
    }
    if objects.len() > 100 {
        Core::write_vtank(
            client,
            &format!(
                "... {} more objects omitted from chat.",
                objects.len() - 100
            ),
        );
    }
}

/// `DumpMaterials`.
fn dump_materials(client: &mut dyn TankClient) {
    let mut materials: Vec<(u32, String)> = Vec::new();
    for item in client.rules().owned_items() {
        if item.material_type != 0 && !materials.iter().any(|(id, _)| *id == item.material_type) {
            materials.push((item.material_type, item.name.clone()));
        }
    }
    materials.sort_by_key(|(id, _)| *id);
    Core::write_vtank(client, "Materials present in owned inventory:");
    for (id, name) in materials {
        Core::write_vtank(client, &format!("{id}\t{name}"));
    }
}

/// The channel name as the emitter spells it (`CanonicalLogChannelName`);
/// a name that is not one of ours is kept as typed.
fn canonical_log_channel_name(requested: &str) -> String {
    CHANNELS
        .iter()
        .map(|channel| channel_name(*channel))
        .find(|name| ordinal::equals_ignore_case(name, requested))
        .map_or_else(|| requested.to_owned(), str::to_owned)
}

/// `/vt log [type] [on/off]`.
fn log_command(cx: &mut TankCx<'_>, arguments: &str) {
    let mut parts: Vec<String> = arguments
        .split(' ')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect();
    if parts.is_empty() {
        let mut logging: Vec<String> = cx
            .core
            .command_log_types
            .iter()
            .chain(cx.core.session_log_types.iter())
            .cloned()
            .collect();
        logging.sort_by(|a, b| ordinal::compare_ignore_case(a, b));
        logging.dedup_by(|a, b| ordinal::equals_ignore_case(a, b));
        let line = if logging.is_empty() {
            "Not currently logging.".to_owned()
        } else {
            format!("Log state:  {}", logging.join(" "))
        };
        Core::write_vtank(cx.client, &line);
        Core::write_vtank(cx.client, LOG_TYPES);
        return;
    }
    if parts.len() == 2 {
        parts[1] = casing::to_lower_invariant(&parts[1]);
    }
    if parts.len() != 2 || !matches!(parts[1].as_str(), "on" | "off") {
        Core::write_vtank(cx.client, "Syntax: /vt log [type] [on/off]");
        return;
    }
    let channel = canonical_log_channel_name(&parts[0]);
    let on = parts[1] == "on";
    let changed = if on {
        cx.core.command_log_types.insert(channel.clone())
    } else {
        let profile = cx.core.command_log_types.remove(&channel);
        let session = cx.core.session_log_types.remove(&channel);
        profile | session
    };
    let verb = if on { "Set " } else { "Reset " };
    Core::write_vtank(cx.client, &format!("{verb}{channel}"));
    if changed {
        cx.core.save_profile(cx.client);
    }
}

impl Core {
    /// Turns on the named log channels for the session (`ApplyLogChannels`).
    pub(crate) fn apply_log_channels(&mut self, requested: &str) {
        for name in requested
            .split([',', ' ', ';'])
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            if ordinal::equals_ignore_case(name, "All") {
                for channel in CHANNELS {
                    self.session_log_types
                        .insert(channel_name(channel).to_owned());
                }
                continue;
            }
            self.session_log_types
                .insert(canonical_log_channel_name(name));
        }
    }
}

impl TankCx<'_> {
    /// MossTank's own pace for looking at the meta, on the engine and the
    /// pass alike (`ApplyMetaInterval`).
    pub(crate) fn apply_meta_interval(&mut self) {
        let milliseconds = self.core.profiles.meta_interval_milliseconds();
        self.meta.set_interval_milliseconds(milliseconds);
        self.core.meta_interval_pending =
            Some(f64::from(self.meta.interval_milliseconds()) / 1000.0);
    }

    /// What follows a meta load the engine took: the route a meta carried
    /// goes, the options come back.
    pub(crate) fn after_meta_load(&mut self) {
        self.discard_embedded_route();
        if self.core.initialized {
            self.apply_persisted_option_overrides();
        }
    }
}

/// Runs one line typed under `/ub` through the UB tab's dispatcher.
pub(crate) fn execute_ub(cx: &mut TankCx<'_>, input: &str) {
    super::ub::execute(cx, input);
}
