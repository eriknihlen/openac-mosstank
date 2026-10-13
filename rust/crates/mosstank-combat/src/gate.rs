//! Readies a combat stance before a rule uses it (`CombatModeGate`): the
//! right caster or weapon wielded (and the shield or second weapon beside
//! it), the stance entered, and the change confirmed by the server before
//! anything is cast in it. A character stuck out of peace is unstuck by
//! using the first profiled wand.
//!
//! The C#'s callbacks become calls and queues: the ammunition question is
//! an [`AmmunitionHooks`] handed in by the fight (and the wield it calls for
//! is the fight's to do once the call returns), the macro stop the gate asks
//! for is queued for its owner ([`CombatModeGate::take_stop_request`]), and
//! settled gear is read with [`CombatModeGate::take_settled`].

use std::collections::HashSet;

use ac_plugin_abi::combat::{CombatMode, CombatStatus};
use ac_plugin_abi::items::{EquipmentItem, EquipmentStatus, ItemStatus};
use ac_plugin_abi::world::ObjectClass;
use mosstank_automation::Automation;
use mosstank_automation::equipment::{self, EquipmentTracker};
use mosstank_automation::locks::{
    ActionLockKind, ActionLockTable, SharedActionLocks, item_use_lock,
};
use mosstank_data::damage::MonsterDamageType;
use mosstank_meta::scheduler::MacroLogChannel;
use mosstank_settings::model::{CombatSettings, VitalSettings};

use crate::{MacroLog, mode_name, post};

/// What the gate says when the profile names no wand to fall back on.
pub const NO_WAND_NOTICE: &str = "Add at least one wand to your profile first.";

/// The warning the stuck-state recovery posts.
pub const BUGGED_COMBAT_STATE_WARNING: &str =
    "Warning: Macro detected bugged combat state. Attempting to wield an item to clear it.";

/// How long a stance change is taken on the client's word alone, seconds.
const MODE_CONFIRMATION_SECONDS: f64 = 0.6;

/// The ammunition questions the fight answers for the gate.
pub trait AmmunitionHooks {
    /// Whether the weapon about to be readied needs other ammunition for
    /// this element (and remembers what to wield).
    fn ammunition_stale(
        &mut self,
        automation: &mut dyn Automation,
        weapon: u32,
        element: MonsterDamageType,
    ) -> bool;
}

/// What one call works with.
#[allow(missing_debug_implementations)]
pub struct GateTurn<'a> {
    /// The client.
    pub automation: &'a mut dyn Automation,
    /// The combat settings (the item list).
    pub combat: &'a CombatSettings,
    /// The vital settings (the peace retry budget).
    pub vitals: &'a VitalSettings,
}

/// One request of [`CombatModeGate::try_prepare`].
#[derive(Debug, Clone, Copy)]
pub struct Prepare<'e> {
    /// The stance wanted.
    pub wanted: CombatMode,
    /// The item to wield, 0 for the gate's own pick.
    pub override_item_id: u32,
    /// Whether the wielded item may stand in for the override.
    pub auto_select: bool,
    /// The element the fight is after (for the ammunition).
    pub element: MonsterDamageType,
    /// The caller's equipment for this pass, when it has one.
    pub captured: Option<&'e [EquipmentItem]>,
    /// The off-hand item, when the caller picked one.
    pub secondary_item_id: Option<u32>,
}

impl Prepare<'_> {
    /// The stance, with every other choice the gate's.
    #[must_use]
    pub fn mode(wanted: CombatMode) -> Self {
        Self {
            wanted,
            override_item_id: 0,
            auto_select: true,
            element: MonsterDamageType::None,
            captured: None,
            secondary_item_id: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WeaponKind {
    None,
    Melee,
    Bow,
    Crossbow,
    Atlatl,
    Thrown,
    Caster,
    Shield,
}

/// The stance gate.
#[allow(missing_debug_implementations)]
pub struct CombatModeGate {
    tracker: Option<EquipmentTracker>,
    drop_to_peace_retries: i32,
    since_mode_request: f64,
    mode_before_request: CombatMode,
    mode_request_in_flight: bool,
    ack_revision_at_request: i64,
    no_wand_notice_posted: bool,
    diagnostic_time: f64,
    next_busy_diagnostic: f64,
    posted_warnings: HashSet<String>,
    status: String,
    action_locks: SharedActionLocks,
    log: Option<MacroLog>,
    stop_requests: Vec<String>,
    ammunition_wield: Option<MonsterDamageType>,
}

impl Default for CombatModeGate {
    fn default() -> Self {
        Self::new()
    }
}

impl CombatModeGate {
    /// A gate with a cooldown table of its own.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tracker: None,
            drop_to_peace_retries: 0,
            since_mode_request: MODE_CONFIRMATION_SECONDS,
            mode_before_request: CombatMode::Unknown,
            mode_request_in_flight: false,
            ack_revision_at_request: 0,
            no_wand_notice_posted: false,
            diagnostic_time: 0.0,
            next_busy_diagnostic: 0.0,
            posted_warnings: HashSet::new(),
            status: String::new(),
            action_locks: ActionLockTable::shared(),
            log: None,
            stop_requests: Vec::new(),
            ammunition_wield: None,
        }
    }

    /// What the gate is doing.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Seconds since the last stance request (for tests).
    #[must_use]
    pub fn since_mode_request_seconds(&self) -> f64 {
        self.since_mode_request
    }

    /// Shares the macro's cooldown table: the stuck-state recovery holds
    /// the item slot like any other use.
    pub fn bind_action_locks(&mut self, locks: SharedActionLocks) {
        self.action_locks = locks;
    }

    /// Where the gate's macro log lines go.
    pub fn set_log(&mut self, log: Option<MacroLog>) {
        self.log = log;
    }

    /// The stop the gate asked the macro for (the C#'s `stopMacro`).
    pub fn take_stop_request(&mut self) -> Option<String> {
        if self.stop_requests.is_empty() {
            None
        } else {
            Some(self.stop_requests.remove(0))
        }
    }

    /// The ammunition wield the last call asked the fight for, once.
    pub fn take_ammunition_wield(&mut self) -> Option<MonsterDamageType> {
        self.ammunition_wield.take()
    }

    /// Whether gear settled since the last ask (the C#'s
    /// `EquipmentSettled`, which pokes the macro).
    pub fn take_settled(&mut self) -> bool {
        self.tracker
            .as_mut()
            .is_some_and(EquipmentTracker::take_settled)
    }

    fn tracker(&mut self, automation: &mut dyn Automation) -> &mut EquipmentTracker {
        self.tracker
            .get_or_insert_with(|| EquipmentTracker::new(automation))
    }

    /// The weapon the gate's tracker has the character holding.
    pub fn wielded_weapon_id(&mut self, automation: &mut dyn Automation) -> u32 {
        self.tracker(automation).weapon_id(automation)
    }

    /// The ammunition the tracker has the character holding.
    pub fn wielded_ammo_id(&mut self, automation: &mut dyn Automation) -> u32 {
        self.tracker(automation).ammo_id(automation)
    }

    /// The shield the tracker has the character wearing.
    pub fn worn_shield_id(&mut self, automation: &mut dyn Automation) -> u32 {
        self.tracker(automation).shield_id(automation)
    }

    /// Whether a caster (`IsCaster`).
    #[must_use]
    pub fn is_caster(item: &EquipmentItem) -> bool {
        item.object_class == ObjectClass::WandStaffOrb
    }

    /// The stance a weapon puts the character in, by its class.
    #[must_use]
    pub fn mode_for(item: &EquipmentItem) -> CombatMode {
        match item.object_class {
            ObjectClass::MeleeWeapon => CombatMode::Melee,
            ObjectClass::MissileWeapon => CombatMode::Missile,
            _ => CombatMode::Magic,
        }
    }

    /// Lets the once-per-run warnings be said again.
    pub fn reset_once_per_run_warnings(&mut self) {
        self.no_wand_notice_posted = false;
        self.posted_warnings.clear();
    }

    fn post_warning_once(&mut self, automation: &mut dyn Automation, text: &str) {
        if !self.posted_warnings.insert(text.to_owned()) {
            return;
        }
        post(automation, &format!("[MossTank] {text}"));
    }

    /// Back to the start.
    pub fn reset(&mut self) {
        self.drop_to_peace_retries = 0;
        self.since_mode_request = MODE_CONFIRMATION_SECONDS;
        self.mode_before_request = CombatMode::Unknown;
        self.mode_request_in_flight = false;
        self.ack_revision_at_request = 0;
        self.no_wand_notice_posted = false;
        self.posted_warnings.clear();
        self.status.clear();
    }

    /// Moves the gate's clocks on.
    pub fn advance_pass(&mut self, automation: &mut dyn Automation, elapsed_seconds: f64) {
        self.tracker(automation)
            .advance(automation, elapsed_seconds);
        self.since_mode_request += elapsed_seconds.max(0.0);
        self.diagnostic_time += elapsed_seconds.max(0.0);
    }

    fn log_line(&self, channel: MacroLogChannel, text: &str) {
        if let Some(log) = &self.log {
            log(channel, text);
        }
    }

    /// Whether the stance is ready; when not, the gate has done the next
    /// step towards it.
    pub fn try_prepare(
        &mut self,
        turn: &mut GateTurn<'_>,
        request: Prepare<'_>,
        hooks: Option<&mut dyn AmmunitionHooks>,
    ) -> bool {
        let automation = &mut *turn.automation;
        // A dispatched inventory operation blocks the stance.
        let busy = automation.busy_state();
        if busy.pending_inventory {
            "Busy".clone_into(&mut self.status);
            if self.diagnostic_time >= self.next_busy_diagnostic {
                self.next_busy_diagnostic = self.diagnostic_time + 5.0;
                let equipment_busy = automation.equipment_busy();
                let casting = automation.is_casting();
                automation.log_info(&format!(
                    "Macro busy: count={}, inventory={}, appraisal=0x{:08X}, source=0x{:08X}, \
                     target=0x{:08X}, awaitingUse={}, equipment={}, magic={}",
                    busy.busy_count,
                    pascal(busy.pending_inventory),
                    busy.awaiting_appraisal,
                    busy.use_source,
                    busy.use_target,
                    pascal(busy.awaiting_use_completion),
                    pascal(equipment_busy),
                    pascal(casting),
                ));
            }
            return false;
        }

        if !automation.equipment_available() {
            return self.try_prepare_mode(automation, None, request.wanted);
        }

        let owned;
        let items: &[EquipmentItem] = if let Some(captured) = request.captured {
            captured
        } else {
            owned = automation.owned_equipment();
            &owned
        };
        let tracked_weapon = self.tracker(automation).weapon_id(automation);
        let wielded = find_by_id(items, tracked_weapon).cloned();

        let mut primary = request.override_item_id;
        if primary != 0 {
            let requested = find_by_id(items, primary).cloned();
            if !is_owned_known_object(automation, primary, requested.as_ref()) {
                if let Some(named) = requested {
                    self.status = format!(
                        "Warning: FCM ignoring item {} because it cannot currently be wielded.",
                        named.name
                    );
                    let text = self.status.clone();
                    self.post_warning_once(automation, &text);
                }
                primary = 0;
            }
        }

        if (request.auto_select || request.override_item_id == 0)
            && let Some(worn) = &wielded
            && Self::mode_for(worn) == request.wanted
        {
            primary = worn.object_id;
        }

        // Otherwise the first wand on the Items page.
        if primary == 0 {
            match find_first_profiled_wand(automation, turn.combat, items) {
                Some((object_id, _)) => primary = object_id,
                None => {
                    self.post_no_wand_notice_and_stop(automation);
                    return false;
                }
            }
        }

        let secondary = request
            .secondary_item_id
            .unwrap_or_else(|| find_first_profiled_shield(automation, turn.combat, items));
        let mut need_secondary = is_tracked_object_valid(automation, secondary, items);
        if secondary == primary {
            need_secondary = false;
        } else if is_tracked_object_valid(automation, primary, items) {
            let selected = find_by_id(items, primary);
            if selected.is_none_or(|weapon| {
                !matches!(kind_for(weapon), WeaponKind::Melee | WeaponKind::Thrown)
                    || (weapon.valid_locations & 0x0200_0000) != 0
            }) {
                need_secondary = false;
            }
        }

        let ammunition_stale = match hooks {
            Some(hooks) => hooks.ammunition_stale(automation, primary, request.element),
            None => false,
        };
        let weapon_differs = self.tracker(automation).weapon_id(automation) != primary;
        let secondary_differs =
            need_secondary && self.tracker(automation).shield_id(automation) != secondary;
        if ammunition_stale || weapon_differs || secondary_differs {
            let target = find_by_id(items, primary).cloned();
            let name = match &target {
                Some(item) => item.name.clone(),
                None => automation
                    .object(primary)
                    .map_or_else(|| "caster".to_owned(), |world| world.name),
            };
            if !self.try_drop_to_peace(automation, turn.combat, turn.vitals, items, &name) {
                return false;
            }

            if let Some(chosen) = &target
                && kind_for(chosen) == WeaponKind::Thrown
                && secondary_differs
            {
                let wielded_now = self.tracker(automation).weapon_id(automation);
                if wielded_now != 0 {
                    let character = automation.character().id;
                    let moved = automation.move_item(wielded_now, character);
                    self.status = moved
                        .notice
                        .unwrap_or_else(|| "Removing wielded weapon".to_owned());
                    return false;
                }
                return self.swap_secondary(automation, secondary, items);
            }

            if weapon_differs {
                let mode = self.effective_mode(automation);
                if !self.tracker(automation).try_arm_swap(primary, true, mode) {
                    "Waiting for equipment".clone_into(&mut self.status);
                    return false;
                }
                self.log_line(MacroLogChannel::BusyState, &format!("(FCM) equip {name}"));
                let equip = automation.equip(primary);
                // Only a started switch is progress.
                if equip.status != EquipmentStatus::Started {
                    self.status = equip
                        .notice
                        .unwrap_or_else(|| format!("Cannot equip {name} ({:?}).", equip.status));
                    let text = self.status.clone();
                    self.post_warning_once(automation, &text);
                    return false;
                }
                self.status = format!("Equipping {name}");
                return false;
            }

            if secondary_differs {
                return self.swap_secondary(automation, secondary, items);
            }

            if ammunition_stale {
                // The fight wields what it chose right after this call
                // returns (the C#'s `WieldAmmunition` callback).
                self.ammunition_wield = Some(request.element);
                return false;
            }
        }

        let weapon = self.tracker(automation).weapon_id(automation);
        let wielded = find_by_id(items, weapon).cloned();
        self.try_prepare_mode(automation, wielded.as_ref(), request.wanted)
    }

    fn swap_secondary(
        &mut self,
        automation: &mut dyn Automation,
        secondary: u32,
        items: &[EquipmentItem],
    ) -> bool {
        let mode = self.effective_mode(automation);
        if !self.tracker(automation).try_arm_swap(secondary, true, mode) {
            "Waiting for equipment".clone_into(&mut self.status);
            return false;
        }
        let result = automation.equip_secondary(secondary);
        let name = find_by_id(items, secondary)
            .map_or_else(|| "secondary item".to_owned(), |item| item.name.clone());
        self.status = if result.status == EquipmentStatus::Started {
            format!("Equipping {name}")
        } else {
            result
                .notice
                .unwrap_or_else(|| format!("Cannot equip {name} ({:?}).", result.status))
        };
        false
    }

    /// Starts the swap cooldown for ammunition (no peace needed).
    pub fn try_arm_ammunition_swap(
        &mut self,
        automation: &mut dyn Automation,
        object_id: u32,
    ) -> bool {
        let mode = self.effective_mode(automation);
        self.tracker(automation)
            .try_arm_swap(object_id, false, mode)
    }

    /// Asks for the stance the wielded item implies; true once it holds.
    fn try_prepare_mode(
        &mut self,
        automation: &mut dyn Automation,
        wielded: Option<&EquipmentItem>,
        wanted: CombatMode,
    ) -> bool {
        let implied = wielded.map_or(wanted, Self::mode_for);
        if automation.combat_snapshot().mode != implied && !self.request_mode(automation, implied) {
            return false;
        }
        if self.mode_change_unconfirmed(automation) {
            self.status = format!("Entering {} mode", mode_name(implied));
            return false;
        }
        "Ready".clone_into(&mut self.status);
        true
    }

    /// Whether the last stance change is still only the client's word.
    fn mode_change_unconfirmed(&mut self, automation: &mut dyn Automation) -> bool {
        let snapshot = automation.combat_snapshot();
        if snapshot.server_mode != CombatMode::Unknown {
            return snapshot.server_mode != snapshot.mode;
        }
        let _ = self.effective_mode(automation);
        self.mode_request_in_flight && self.since_mode_request < MODE_CONFIRMATION_SECONDS
    }

    /// Drops to peace before a swap; true once in peace. Past the retry
    /// budget the first profiled wand is used to clear a stuck state.
    pub fn try_drop_to_peace(
        &mut self,
        automation: &mut dyn Automation,
        combat: &CombatSettings,
        vitals: &VitalSettings,
        items: &[EquipmentItem],
        for_item_name: &str,
    ) -> bool {
        let mode = self.effective_mode(automation);
        if mode == CombatMode::Unknown {
            "Waiting for the combat mode".clone_into(&mut self.status);
            return false;
        }
        if mode == CombatMode::Peace {
            self.drop_to_peace_retries = 0;
            return true;
        }

        self.drop_to_peace_retries += 1;
        if self.drop_to_peace_retries >= vitals.drop_to_peace_mode_retry_count {
            self.action_locks
                .borrow_mut()
                .arm(ActionLockKind::ItemUse, item_use_lock::IMMEDIATE_SECONDS);
            let Some((recovery_id, recovery_name)) =
                find_first_profiled_wand(automation, combat, items)
            else {
                self.post_no_wand_notice_and_stop(automation);
                return false;
            };
            let used = automation.use_item(recovery_id);
            self.status = if used.status == ItemStatus::Started {
                BUGGED_COMBAT_STATE_WARNING.to_owned()
            } else {
                format!(
                    "Combat-state recovery with {recovery_name}: {:?}",
                    used.status
                )
            };
            if used.status == ItemStatus::Started {
                post(
                    automation,
                    &format!("[MossTank] {BUGGED_COMBAT_STATE_WARNING}"),
                );
            }
            self.drop_to_peace_retries = 0;
            return false;
        }

        if self.request_mode(automation, CombatMode::Peace) {
            self.drop_to_peace_retries = 0;
            return true;
        }
        if self.status.is_empty() || self.status.starts_with("Entering") {
            self.status = format!("Entering peace mode to equip {for_item_name}");
        }
        false
    }

    /// The stance as far as the gate trusts it: inside the confirmation
    /// window the one before the request, else the live one.
    pub fn effective_mode(&mut self, automation: &mut dyn Automation) -> CombatMode {
        let snapshot = automation.combat_snapshot();
        if self.mode_request_in_flight
            && snapshot.qualified_self_motion_revision > self.ack_revision_at_request
        {
            self.mode_request_in_flight = false;
            self.since_mode_request = snapshot.qualified_self_motion_age_seconds.max(0.0);
        }
        if self.since_mode_request < MODE_CONFIRMATION_SECONDS {
            self.mode_before_request
        } else {
            snapshot.mode
        }
    }

    /// Asks for a stance. Never true: the stance is ready only once
    /// confirmed.
    fn request_mode(&mut self, automation: &mut dyn Automation, mode: CombatMode) -> bool {
        let snapshot = automation.combat_snapshot();
        self.mode_before_request = snapshot.mode;
        self.ack_revision_at_request = snapshot.qualified_self_motion_revision;
        self.since_mode_request = 0.0;
        self.mode_request_in_flight = true;
        self.log_line(
            MacroLogChannel::BusyState,
            &format!("(FCM) requesting {}", mode_name(mode)),
        );
        let result = automation.enter_combat_mode(mode);
        if result.status == CombatStatus::Unavailable {
            self.mode_request_in_flight = false;
            "Combat mode is unavailable".clone_into(&mut self.status);
            return false;
        }
        self.status = if result.status == CombatStatus::Refused {
            result
                .notice
                .unwrap_or_else(|| format!("Cannot enter {} mode", mode_name(mode)))
        } else {
            format!("Entering {} mode", mode_name(mode))
        };
        false
    }

    fn post_no_wand_notice_and_stop(&mut self, automation: &mut dyn Automation) {
        NO_WAND_NOTICE.clone_into(&mut self.status);
        if !self.no_wand_notice_posted {
            post(automation, &format!("[MossTank] {NO_WAND_NOTICE}"));
            self.no_wand_notice_posted = true;
        }
        self.stop_requests.push(NO_WAND_NOTICE.to_owned());
    }
}

/// .NET's `bool.ToString()`.
fn pascal(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

fn kind_for(item: &EquipmentItem) -> WeaponKind {
    match item.object_class {
        ObjectClass::WandStaffOrb => WeaponKind::Caster,
        ObjectClass::MeleeWeapon => WeaponKind::Melee,
        ObjectClass::MissileWeapon => match item.weapon_type {
            0 => WeaponKind::Thrown,
            1 => WeaponKind::Bow,
            2 => WeaponKind::Crossbow,
            4 => WeaponKind::Atlatl,
            _ => WeaponKind::None,
        },
        _ if item.valid_locations == equipment::SHIELD_SLOT => WeaponKind::Shield,
        _ => WeaponKind::None,
    }
}

fn find_by_id(items: &[EquipmentItem], object_id: u32) -> Option<&EquipmentItem> {
    items.iter().find(|item| item.object_id == object_id)
}

/// The weapon in a weapon slot (`FindWielded`).
#[must_use]
pub fn find_wielded(items: &[EquipmentItem]) -> Option<&EquipmentItem> {
    items
        .iter()
        .find(|item| equipment::is_weapon_slot(item.equipped_location))
}

fn is_tracked_object_valid(
    automation: &mut dyn Automation,
    object_id: u32,
    items: &[EquipmentItem],
) -> bool {
    if object_id == 0 {
        return false;
    }
    if automation.objects_available() {
        automation.object(object_id).is_some()
    } else {
        find_by_id(items, object_id).is_some()
    }
}

fn is_owned_known_object(
    automation: &mut dyn Automation,
    object_id: u32,
    projected: Option<&EquipmentItem>,
) -> bool {
    if automation.objects_available() {
        return automation
            .object(object_id)
            .is_some_and(|value| value.is_owned);
    }
    projected.is_some()
}

fn find_first_profiled_shield(
    automation: &mut dyn Automation,
    combat: &CombatSettings,
    items: &[EquipmentItem],
) -> u32 {
    let objects = automation.objects_available();
    for &id in &combat.combat_item_order_ids {
        let Some(item) = find_by_id(items, id) else {
            continue;
        };
        if kind_for(item) != WeaponKind::Shield {
            continue;
        }
        if objects && !automation.object(id).is_some_and(|world| world.is_owned) {
            continue;
        }
        return id;
    }
    0
}

/// The first wand on the profile's Items page the character owns.
fn find_first_profiled_wand(
    automation: &mut dyn Automation,
    combat: &CombatSettings,
    items: &[EquipmentItem],
) -> Option<(u32, String)> {
    let player = automation.character().id;
    if automation.objects_available() {
        for &id in &combat.combat_item_order_ids {
            if id == 0 || id == u32::MAX || id == player {
                continue;
            }
            let Some(item) = automation.object(id) else {
                continue;
            };
            if !item.is_owned || item.object_class != ObjectClass::WandStaffOrb {
                continue;
            }
            return Some((item.object_id, item.name));
        }
        return None;
    }
    for &id in &combat.combat_item_order_ids {
        if id == 0 || id == u32::MAX || id == player {
            continue;
        }
        if let Some(item) = items
            .iter()
            .find(|item| item.object_id == id && CombatModeGate::is_caster(item))
        {
            return Some((item.object_id, item.name.clone()));
        }
    }
    None
}

/// Locks a caller shares with the gate.
#[must_use]
pub fn shared_locks(gate: &CombatModeGate) -> SharedActionLocks {
    gate.action_locks.clone()
}

/// The gate as the rules share it (one plugin, one thread).
pub type SharedCombatModeGate = std::rc::Rc<std::cell::RefCell<CombatModeGate>>;
