//! The main window's frame (title, tabs, debug-log and remote buttons), the
//! Options and Vitals pages, the compact remote and the action history
//! window.

use ac_plugin_abi::ui::UiValue;
use mosstank_buffs::recharge_controller::IDLE_STATUS;
use mosstank_settings::model::PetRangeMode;
use openac_plugin_compat::number::{self, NumberStyles};

use super::{Arg, TankTab, b, f, i, t, tl};
use crate::tank::TankCx;
use crate::tank::options::boolean;

/// How often the coverage line is worked out again, seconds.
const COVERAGE_REFRESH_INTERVAL_SECONDS: f64 = 1.0;

/// The vitals and coverage lines, rebuilt only when what they show changed
/// (`RefreshDisplayBindings`).
#[derive(Debug, Clone, Default)]
pub(crate) struct VitalsLine {
    pub(crate) vitals: String,
    pub(crate) coverage: String,
    initialized: bool,
    last: [u32; 6],
    coverage_spells: Option<Vec<u32>>,
    coverage_buff_line_count: usize,
    coverage_refresh_remaining: f64,
}

impl VitalsLine {
    /// Forgets the coverage's spellbook, so the next frame works it out.
    pub(crate) fn forget_coverage(&mut self) {
        self.coverage_spells = None;
        self.coverage_refresh_remaining = 0.0;
    }
}

/// `Percent`.
fn percent(fraction: f64) -> String {
    number::format_f64(fraction * 100.0, "0").unwrap_or_default() + "%"
}

/// `ToString("0.#")`.
fn one_decimal(value: f64) -> String {
    number::format_f64(value, "0.#").unwrap_or_default()
}

pub(super) fn value(cx: &mut TankCx<'_>, name: &str) -> Option<UiValue> {
    let panel = &cx.core.panel;
    let s = &cx.core.settings;
    let tab = panel.active_tab;
    match name {
        "WindowAvailable" => b(cx.client.rules().is_available()),
        "WindowTitle" => t(format!("MossTank  v{}", crate::VERSION)),
        "DebugLogButtonText" => t("Debug log: off"),
        "DebugLogLocation" => t("Save detailed macro activity (newest 2 MiB)."),
        "OptionsSelected" | "OptionsVisible" => b(tab == TankTab::Options),
        "ProfilesSelected" | "ProfilesVisible" => b(tab == TankTab::Profiles),
        "VitalsSelected" | "VitalsVisible" => b(tab == TankTab::Vitals),
        "MonstersSelected" | "MonstersVisible" => b(tab == TankTab::Monsters),
        "ItemsSelected" | "ItemsVisible" => b(tab == TankTab::Items),
        "ConsumablesSelected" | "ConsumablesVisible" => b(tab == TankTab::Consumables),
        "BuffsSelected" | "BuffsVisible" => b(tab == TankTab::Buffs),
        "RouteSelected" | "RouteVisible" => b(tab == TankTab::Route),
        "MetaSelected" | "MetaVisible" => b(tab == TankTab::Meta),
        "UbSelected" | "UbVisible" => b(tab == TankTab::Ub),
        "UbSettingsSelected" | "UbSettingsVisible" => b(tab == TankTab::UbSettings),
        "TinkerSelected" | "TinkerVisible" => b(tab == TankTab::Tinker),
        "OptionsTabEnabled"
        | "ProfilesTabEnabled"
        | "VitalsTabEnabled"
        | "MonstersTabEnabled"
        | "ItemsTabEnabled"
        | "ConsumablesTabEnabled"
        | "BuffsTabEnabled"
        | "RouteTabEnabled"
        | "MetaTabEnabled"
        | "UbTabEnabled"
        | "UbSettingsTabEnabled"
        | "TinkerTabEnabled" => b(true),

        // The Options page.
        "MonsterRangeValueText" => t(one_decimal(s.combat.maximum_range)),
        "RingRangeValueText" => t(one_decimal(s.combat.ring_distance)),
        "ApproachRangeValueText" => t(one_decimal(s.combat.approach_distance)),
        "FollowNavMinimumValueText" => t(one_decimal(s.navigation.minimum_distance_meters)),
        "BuffingEnabled" => b(s.buffs.enabled),
        "CombatEnabled" => b(s.combat.enabled),
        "NavigationEnabled" => b(s.navigation.enabled),
        "LootEnabled" => b(s.inventory.loot.enabled),
        "MetaEnabled" => b(cx.meta.enabled()),
        "AutoStackEnabled" => b(s.inventory.auto_stack),
        "AutoCramEnabled" => b(s.inventory.auto_cram),
        "AutoCraftItemsEnabled" => b(s.inventory.auto_craft_items),
        "UseDispelItemsEnabled" => b(s.vitals.use_dispel_items),
        "CastDispelSelfEnabled" => b(s.vitals.cast_dispel_self),
        "ShowNavLinesEnabled" => b(s.navigation.show_nav_lines),
        "WalkToOwnRareCorpsesEnabled" => b(s.inventory.loot.walk_to_own_rare_corpses),
        "AutoFellowManagementEnabled" => b(s.combat.auto_fellow_management),
        "ManaChargesWhenOffEnabled" => b(s.inventory.mana_charges_when_off),
        "NavigationPriorityEnabled" => b(s.navigation.priority),
        "LootPriorityBoostEnabled" => b(s.inventory.loot.priority_boost),
        "LootOnlyRareCorpsesEnabled" => b(s.inventory.loot.loot_only_rare_corpses),
        "IdlePeaceModeEnabled" => b(s.combat.idle_peace_mode),
        "IdleBuffTopoffEnabled" => b(s.buffs.idle_buff_topoff),
        "FastCastBuffsEnabled" => b(s.buffs.fast_cast_buffs),
        "DontShootAtWallsEnabled" => b(s.combat.use_projectile_awareness),
        "DebuffFallbackEnabled" => b(s.combat.allow_debuff_fallback),
        "SummonPetsEnabled" => b(s.combat.summon_pets),
        "CustomPetRangeEnabled" => b(s.combat.pet_range_mode == PetRangeMode::CUSTOM),
        "PetCustomRangeValueText" => t(one_decimal(s.combat.pet_custom_range)),
        "PetDensityValueText" => t(s.combat.pet_monster_density.to_string()),
        "TargetLockEnabled" => b(s.combat.target_lock),
        "FollowAroundCornersEnabled" => b(s.navigation.follow_around_corners),
        "CombatMacroRunning" => b(cx.core.combat.enabled()),
        "CombatButtonText" => t(cx.core.combat.button_text()),
        "CombatStatus" => t(cx.core.combat.status()),
        "CombatTarget" => t(cx.core.combat.target_text()),
        "CombatMode" => t(cx.core.combat.mode_text()),
        "BuffStatus" => t(cx.core.status.clone()),
        "DispelStatus" => t(cx.core.dispel.status()),
        "CraftingStatus" => t(cx.core.crafting.status()),
        "ItemManaRechargeStatus" => t(cx.core.item_mana.status()),
        "LootStatus" => t(cx.core.loot.status.clone()),
        "FellowshipManagerStatus" => t(cx.core.fellowship.status()),
        "NavigationStatus" => t(cx.core.navigation.status()),

        // The Vitals page.
        "NormalHealthPercent" => f(s.vitals.normal_health as f32 * 100.0),
        "NormalStaminaPercent" => f(s.vitals.normal_stamina as f32 * 100.0),
        "NormalManaPercent" => f(s.vitals.normal_mana as f32 * 100.0),
        "NoTargetHealthPercent" => f(s.vitals.no_target_health as f32 * 100.0),
        "NoTargetStaminaPercent" => f(s.vitals.no_target_stamina as f32 * 100.0),
        "NoTargetManaPercent" => f(s.vitals.no_target_mana as f32 * 100.0),
        "HelperHealthPercent" => f(s.vitals.helper_health as f32 * 100.0),
        "HelperStaminaPercent" => f(s.vitals.helper_stamina as f32 * 100.0),
        "HelperManaPercent" => f(s.vitals.helper_mana as f32 * 100.0),
        "NormalHealthText" => t(percent(s.vitals.normal_health)),
        "NormalStaminaText" => t(percent(s.vitals.normal_stamina)),
        "NormalManaText" => t(percent(s.vitals.normal_mana)),
        "NoTargetHealthText" => t(percent(s.vitals.no_target_health)),
        "NoTargetStaminaText" => t(percent(s.vitals.no_target_stamina)),
        "NoTargetManaText" => t(percent(s.vitals.no_target_mana)),
        "HelperHealthText" => t(percent(s.vitals.helper_health)),
        "HelperStaminaText" => t(percent(s.vitals.helper_stamina)),
        "HelperManaText" => t(percent(s.vitals.helper_mana)),
        "VitalUpkeepEnabled" => b(s.vitals.enabled),
        "HelpOthersEnabled" => b(s.vitals.help_others),
        "HelpNetworkPeersEnabled" => b(s.vitals.help_network_peers),
        "VitalStatus" => t(vital_status(cx)),
        "Vitals" => t(cx.core.panel.vitals.vitals.clone()),
        "Coverage" => t(cx.core.panel.vitals.coverage.clone()),

        // The action history window.
        "ActionHistoryVisible" => b(cx.core.panel.action_history_visible),
        "ActionHistoryLines" => tl(Vec::new()),
        "ActionHistoryFirstIndex" => i(0),
        _ => None,
    }
}

/// One status line over the two recharge owners (`VitalStatus`).
pub(crate) fn vital_status(cx: &TankCx<'_>) -> String {
    if cx.core.vital_recharge.status() == IDLE_STATUS {
        cx.core.vital_helper_recharge.status().to_owned()
    } else {
        cx.core.vital_recharge.status().to_owned()
    }
}

/// `SetMetaOption(name, Boolean(value))`; a switch never faults.
fn set_switch(cx: &mut TankCx<'_>, name: &str, value: bool) {
    let _ = cx.set_meta_option(name, &boolean(value));
}

/// `SetDistanceText`: a typed distance, clamped; anything else is ignored.
fn set_distance_text(
    cx: &mut TankCx<'_>,
    text: &str,
    minimum: f32,
    maximum: f32,
    apply: impl FnOnce(&mut TankCx<'_>, f64),
    save_profile: bool,
) {
    // The invariant culture first, then the current one (register COMPAT-1:
    // both are the invariant culture here).
    let Some(distance) = number::parse_f32(text, NumberStyles::FLOAT) else {
        return;
    };
    apply(cx, f64::from(distance.clamp(minimum, maximum)));
    if save_profile {
        cx.core.save_profile(cx.client);
    }
}

/// `UpdateProfile`.
fn update_profile(cx: &mut TankCx<'_>, update: impl FnOnce(&mut TankCx<'_>)) {
    update(cx);
    cx.core.save_profile(cx.client);
}

/// `Clamp`: a slider's fraction kept inside 0..=1.
fn clamp_fraction(value: f32) -> f64 {
    f64::from(value).clamp(0.0, 1.0)
}

#[allow(clippy::too_many_lines)]
pub(super) fn act(cx: &mut TankCx<'_>, name: &str, arg: &Arg) -> bool {
    match name {
        "ShowOptions" => cx.select_tab(TankTab::Options),
        "ShowProfiles" => cx.select_tab(TankTab::Profiles),
        "ShowVitals" => cx.select_tab(TankTab::Vitals),
        "ShowMonsters" => cx.select_tab(TankTab::Monsters),
        "ShowItems" => cx.select_tab(TankTab::Items),
        "ShowConsumables" => cx.select_tab(TankTab::Consumables),
        "ShowBuffs" => cx.select_tab(TankTab::Buffs),
        "ShowRoute" => cx.select_tab(TankTab::Route),
        "ShowMeta" => cx.select_tab(TankTab::Meta),
        "ShowUb" => cx.select_tab(TankTab::Ub),
        "ShowUbSettings" => cx.select_tab(TankTab::UbSettings),
        "ShowTinker" => cx.select_tab(TankTab::Tinker),
        // The debug log is its own slice; the button reads "off" until it
        // lands.
        "ToggleDebugLog" => {}
        "ShowRemote" => switch_panel(cx, "main", "remote"),
        "ShowMainWindow" => switch_panel(cx, "remote", "main"),
        "ShowActionHistory" => cx.core.panel.action_history_visible = true,
        "HideActionHistory" => cx.core.panel.action_history_visible = false,
        "ClearActionHistory" => {}

        "ForceBuff" => cx.core.force_buff(cx.client),
        "CancelForceBuff" => cx.core.cancel_force_buff(cx.client),
        "ToggleCombat" => {
            let running = cx.core.combat.enabled();
            cx.core.set_macro_running(cx.client, !running);
        }
        "ToggleCombatEnabled" => {
            cx.core.settings.combat.enabled = !cx.core.settings.combat.enabled;
            cx.core.save_profile(cx.client);
        }
        "ToggleAutoFellowManagement" => {
            let on = cx.core.settings.combat.auto_fellow_management;
            set_switch(cx, "AutoFellowManagement", !on);
        }
        "ToggleBuffing" => {
            let on = cx.core.settings.buffs.enabled;
            set_switch(cx, "EnableBuffing", !on);
        }
        "ToggleIdlePeaceMode" => {
            let on = cx.core.settings.combat.idle_peace_mode;
            set_switch(cx, "IdlePeaceMode", !on);
        }
        "ToggleIdleBuffTopoff" => {
            let on = cx.core.settings.buffs.idle_buff_topoff;
            set_switch(cx, "IdleBuffTopoff", !on);
        }
        "ToggleManaChargesWhenOff" => {
            let on = cx.core.settings.inventory.mana_charges_when_off;
            set_switch(cx, "ManaChargesWhenOff", !on);
        }
        "ToggleFastCastBuffs" => {
            let on = cx.core.settings.buffs.fast_cast_buffs;
            set_switch(cx, "FastCastBuffs", !on);
        }
        "ToggleDontShootAtWalls" => {
            let on = cx.core.settings.combat.use_projectile_awareness;
            set_switch(cx, "UseProjectileAwareness", !on);
        }
        "ToggleDebuffFallback" => {
            let on = cx.core.settings.combat.allow_debuff_fallback;
            set_switch(cx, "AllowDebuffFallback", !on);
        }
        "ToggleCastDispelSelf" => {
            let on = cx.core.settings.vitals.cast_dispel_self;
            set_switch(cx, "CastDispelSelf", !on);
        }
        "ToggleUseDispelItems" => {
            let on = cx.core.settings.vitals.use_dispel_items;
            set_switch(cx, "UseDispelItems", !on);
        }
        "ToggleShowNavLines" => {
            let on = cx.core.settings.navigation.show_nav_lines;
            set_switch(cx, "ShowNavLines", !on);
        }
        "ToggleWalkToOwnRareCorpses" => {
            let on = cx.core.settings.inventory.loot.walk_to_own_rare_corpses;
            set_switch(cx, mosstank_settings::options::WALK_TO_RARE_CORPSE, !on);
        }
        "ToggleAutoStack" => {
            cx.core.settings.inventory.auto_stack = !cx.core.settings.inventory.auto_stack;
            cx.core.inventory_maintenance.reset();
            cx.core.save_profile(cx.client);
        }
        "ToggleAutoCram" => {
            cx.core.settings.inventory.auto_cram = !cx.core.settings.inventory.auto_cram;
            cx.core.inventory_maintenance.reset();
            cx.core.save_profile(cx.client);
        }
        "ToggleAutoCraftItems" => {
            cx.core.settings.inventory.auto_craft_items =
                !cx.core.settings.inventory.auto_craft_items;
            cx.core.crafting.reset();
            cx.core.save_profile(cx.client);
        }
        "ToggleLooting" => {
            cx.core.settings.inventory.loot.enabled = !cx.core.settings.inventory.loot.enabled;
            cx.core.reset_loot(cx.client);
            cx.core.save_profile(cx.client);
        }
        "ToggleLootPriorityBoost" => {
            let loot = &mut cx.core.settings.inventory.loot;
            loot.priority_boost = !loot.priority_boost;
            cx.core.save_profile(cx.client);
        }
        "ToggleLootOnlyRareCorpses" => {
            let loot = &mut cx.core.settings.inventory.loot;
            loot.loot_only_rare_corpses = !loot.loot_only_rare_corpses;
            cx.core.save_profile(cx.client);
        }
        // Flipping a setting is not a route change: the round stays where
        // it was.
        "ToggleNavigation" => {
            cx.core.settings.navigation.enabled = !cx.core.settings.navigation.enabled;
            cx.core.save_route_profile(cx.client);
        }
        "ToggleNavigationPriority" => {
            cx.core.settings.navigation.priority = !cx.core.settings.navigation.priority;
            cx.core.save_route_profile(cx.client);
        }
        "ToggleFollowAroundCorners" => {
            let navigation = &mut cx.core.settings.navigation;
            navigation.follow_around_corners = !navigation.follow_around_corners;
            cx.core.save_route_profile(cx.client);
        }
        "ToggleMeta" => {
            // The checkbox writes the stored setting, not just the engine.
            let on = cx.meta.enabled();
            set_switch(cx, "EnableMeta", !on);
            cx.meta
                .current_state()
                .clone_into(&mut cx.core.settings.combat.meta_state);
        }
        "ToggleTargetLock" => update_profile(cx, |cx| {
            cx.core.settings.combat.target_lock = !cx.core.settings.combat.target_lock;
        }),
        "ToggleSummonPets" => update_profile(cx, |cx| {
            cx.core.settings.combat.summon_pets = !cx.core.settings.combat.summon_pets;
        }),
        "TogglePetRangeMode" => update_profile(cx, |cx| {
            let combat = &mut cx.core.settings.combat;
            combat.pet_range_mode = if combat.pet_range_mode == PetRangeMode::CUSTOM {
                PetRangeMode::ATTACK_DISTANCE
            } else {
                PetRangeMode::CUSTOM
            };
        }),
        "SetPetCustomRangeText" => set_distance_text(
            cx,
            arg.text(),
            1.0,
            100.0,
            |cx, distance| cx.core.settings.combat.pet_custom_range = distance,
            true,
        ),
        "SetPetDensityText" => {
            if let Some(density) = number::parse_i32(arg.text(), NumberStyles::INTEGER) {
                cx.core.settings.combat.pet_monster_density = density.clamp(1, 25);
                cx.core.save_profile(cx.client);
            }
        }
        "SetMonsterRangeText" => set_distance_text(
            cx,
            arg.text(),
            1.0,
            100.0,
            |cx, distance| cx.core.settings.combat.maximum_range = distance,
            true,
        ),
        "SetRingRangeText" => set_distance_text(
            cx,
            arg.text(),
            1.0,
            100.0,
            |cx, distance| cx.core.settings.combat.ring_distance = distance,
            true,
        ),
        // The approach box sets both approach distances, as the reference
        // does: the monster's and the corpse's.
        "SetApproachRangeText" => set_distance_text(
            cx,
            arg.text(),
            0.0,
            100.0,
            |cx, distance| {
                cx.core.settings.combat.approach_distance = distance;
                cx.core.settings.inventory.loot.corpse_approach_range = distance;
            },
            true,
        ),
        "SetFollowNavMinimumText" => set_distance_text(
            cx,
            arg.text(),
            0.5,
            50.0,
            |cx, distance| {
                cx.core.settings.navigation.minimum_distance_meters = distance;
                cx.core.save_route_profile(cx.client);
            },
            false,
        ),
        "ToggleAdvancedOptionsVisible" => cx.toggle_advanced_options_visible(),

        // The Vitals page.
        "SetNormalHealthPercent" => set_vital(cx, arg, |v, x| v.normal_health = x),
        "SetNormalStaminaPercent" => set_vital(cx, arg, |v, x| v.normal_stamina = x),
        "SetNormalManaPercent" => set_vital(cx, arg, |v, x| v.normal_mana = x),
        "SetNoTargetHealthPercent" => set_vital(cx, arg, |v, x| v.no_target_health = x),
        "SetNoTargetStaminaPercent" => set_vital(cx, arg, |v, x| v.no_target_stamina = x),
        "SetNoTargetManaPercent" => set_vital(cx, arg, |v, x| v.no_target_mana = x),
        "SetHelperHealthPercent" => set_vital(cx, arg, |v, x| v.helper_health = x),
        "SetHelperStaminaPercent" => set_vital(cx, arg, |v, x| v.helper_stamina = x),
        "SetHelperManaPercent" => set_vital(cx, arg, |v, x| v.helper_mana = x),
        "ToggleVitalUpkeep" => {
            let vitals = &mut cx.core.settings.vitals;
            vitals.enabled = !vitals.enabled;
            // As written in the C#: only the self half is guarded by the
            // switch; the helper half is reset either way.
            if !cx.core.settings.vitals.enabled {
                cx.core.vital_recharge.reset();
            }
            cx.core.vital_helper_recharge.reset();
            cx.core.save_profile(cx.client);
        }
        "ToggleHelpOthers" => update_profile(cx, |cx| {
            cx.core.settings.vitals.help_others = !cx.core.settings.vitals.help_others;
        }),
        "ToggleHelpNetworkPeers" => update_profile(cx, |cx| {
            let vitals = &mut cx.core.settings.vitals;
            vitals.help_network_peers = !vitals.help_network_peers;
        }),
        "RemoteFollowSelected" => cx.follow_selected_player(),
        _ => return false,
    }
    true
}

/// A Vitals slider (`Set*Percent`): the percentage as a fraction, kept
/// inside 0..=1, then saved.
fn set_vital(
    cx: &mut TankCx<'_>,
    arg: &Arg,
    apply: impl FnOnce(&mut mosstank_settings::model::VitalSettings, f64),
) {
    apply(
        &mut cx.core.settings.vitals,
        clamp_fraction(arg.float() / 100.0),
    );
    cx.core.save_profile(cx.client);
}

/// `SwitchPanel`: the destination opens first, so the current controls
/// stay when it cannot.
fn switch_panel(cx: &mut TankCx<'_>, source: &str, destination: &str) {
    if cx.client.show_window(destination, true) {
        cx.client.show_window(source, false);
    }
}

impl TankCx<'_> {
    /// `RefreshDisplayBindings`: the vitals line when a vital changed, the
    /// coverage line when the spellbook changed or once a second.
    pub(crate) fn refresh_display_bindings(&mut self, elapsed_seconds: f64) {
        let available = self.client.rules().is_available();
        let line = &mut self.core.panel.vitals;
        if !available {
            *line = VitalsLine::default();
            return;
        }
        let character = self.client.rules().character();
        let now = [
            character.health.0,
            character.health.1,
            character.stamina.0,
            character.stamina.1,
            character.mana.0,
            character.mana.1,
        ];
        let line = &mut self.core.panel.vitals;
        if !line.initialized || now != line.last {
            line.last = now;
            line.vitals = format!(
                "Health {}/{}   Stam {}/{}   Mana {}/{}",
                now[0], now[1], now[2], now[3], now[4], now[5]
            );
            line.initialized = true;
        }
        let spells = self.client.rules().spells().known_self_buffs();
        let ids: Vec<u32> = spells.iter().map(|spell| spell.spell_id).collect();
        let line = &mut self.core.panel.vitals;
        let changed = line.coverage_spells.as_ref() != Some(&ids);
        line.coverage_refresh_remaining -= elapsed_seconds.max(0.0);
        if !changed && line.coverage_refresh_remaining > 0.0 {
            return;
        }
        if changed {
            line.coverage_spells = Some(ids);
            line.coverage_buff_line_count = mosstank_buffs::profile::build(&spells).len();
        }
        let trained = self
            .client
            .rules()
            .skills()
            .iter()
            .filter(|skill| {
                matches!(
                    skill.training,
                    ac_plugin_abi::character::Training::Trained
                        | ac_plugin_abi::character::Training::Specialized
                )
            })
            .count();
        let attributes = self.client.rules().attributes().len();
        let line = &mut self.core.panel.vitals;
        line.coverage = format!(
            "{attributes} attributes, {trained} trained skills, {} buff lines",
            line.coverage_buff_line_count
        );
        line.coverage_refresh_remaining = COVERAGE_REFRESH_INTERVAL_SECONDS;
    }

    /// `FollowSelectedPlayer` (the remote's FC button).
    pub(crate) fn follow_selected_player(&mut self) {
        let selected = self.client.rules().selected_object().unwrap_or(0);
        let me = self.client.rules().character().id;
        let player = self
            .client
            .rules()
            .capture_objects()
            .into_iter()
            .find(|candidate| {
                candidate.object_id == selected
                    && candidate.object_id != me
                    && super::ub_pages::is_player_class(candidate.object_class)
            });
        let Some(target) = player else {
            self.write_ub_error("Select a live player to follow first.");
            return;
        };
        if !self.follow_on_follow_route(target.object_id, &target.name) {
            self.write_ub_error(&format!(
                "Failed to follow {}[0x{:08X}]",
                target.name, target.object_id
            ));
            return;
        }
        self.write_ub(&format!(
            "Following {}[0x{:08X}]",
            target.name, target.object_id
        ));
        if !self.core.settings.navigation.enabled {
            self.write_ub("Turn Enable Navigation on to start.");
        }
    }
}
