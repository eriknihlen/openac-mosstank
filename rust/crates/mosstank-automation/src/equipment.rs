//! What the character wields, as the server last confirmed it
//! (`EquipmentTracker`): the weapon, the ammunition and the shield, and a
//! short cooldown after each swap so two swaps do not cross.
//!
//! The C# listens to the equipment surface's placement event; here the
//! tracker drains the client's queue of placements before every read and
//! every advance, so a placement is applied in the order it arrived.

use ac_plugin_abi::combat::CombatMode;
use ac_plugin_abi::items::EquipmentObservation;

use crate::surface::Automation;

/// Seconds after a placement before the tracker reports the gear settled.
const SETTLEMENT_SECONDS: f64 = 0.1;

/// Seconds a swap holds the next one off.
const SWAP_COOLDOWN_SECONDS: f64 = 0.8;

/// The slots a weapon is held in (`IsWeaponSlot`).
#[must_use]
pub fn is_weapon_slot(location: u32) -> bool {
    matches!(
        location,
        0x0100_0000 | 0x0010_0000 | 0x0040_0000 | 0x0200_0000
    )
}

/// The ammunition slot.
pub const AMMUNITION_SLOT: u32 = 0x0080_0000;

/// The shield slot.
pub const SHIELD_SLOT: u32 = 0x0020_0000;

/// The gear the server has the character holding.
#[derive(Debug, Clone, Default)]
pub struct EquipmentTracker {
    equipped_ids: Vec<u32>,
    settlements: Vec<f64>,
    weapon_id: u32,
    ammo_id: u32,
    shield_id: u32,
    now: f64,
    swap_cooldown_until: f64,
    recently_swapped: bool,
    swap_target_id: u32,
    was_available: bool,
    started: bool,
    settled: bool,
}

impl EquipmentTracker {
    /// A tracker seeded from what the world shows worn, when the client
    /// can say.
    pub fn new(automation: &mut dyn Automation) -> Self {
        let mut tracker = Self::default();
        tracker.start(automation);
        tracker
    }

    fn start(&mut self, automation: &mut dyn Automation) {
        if self.started {
            return;
        }
        self.started = true;
        self.was_available = automation.equipment_available();
        if self.was_available {
            self.seed_from_world(automation);
        }
    }

    /// Applies the placements the client queued since the last look.
    pub fn observe(&mut self, automation: &mut dyn Automation) {
        self.start(automation);
        for observation in automation.take_equipment_observations() {
            self.on_placement_observed(observation);
        }
    }

    /// The wielded weapon, 0 for none (or one the client no longer knows).
    pub fn weapon_id(&mut self, automation: &mut dyn Automation) -> u32 {
        self.observe(automation);
        let id = self.weapon_id;
        valid_or_zero(automation, id)
    }

    /// The wielded ammunition.
    pub fn ammo_id(&mut self, automation: &mut dyn Automation) -> u32 {
        self.observe(automation);
        let id = self.ammo_id;
        valid_or_zero(automation, id)
    }

    /// The worn shield.
    pub fn shield_id(&mut self, automation: &mut dyn Automation) -> u32 {
        self.observe(automation);
        let id = self.shield_id;
        valid_or_zero(automation, id)
    }

    /// Everything worn, in the order it went on.
    pub fn equipped_ids(&mut self, automation: &mut dyn Automation) -> &[u32] {
        self.observe(automation);
        &self.equipped_ids
    }

    /// The item the last swap went for.
    #[must_use]
    pub fn swap_target_id(&self) -> u32 {
        self.swap_target_id
    }

    /// Whether a swap is still cooling down.
    pub fn recently_swapped(&mut self) -> bool {
        if self.swap_cooldown_until < self.now {
            self.recently_swapped = false;
        }
        self.recently_swapped
    }

    /// Moves the clock on; a placement that has settled says so through
    /// [`Self::take_settled`] (the C#'s poke).
    pub fn advance(&mut self, automation: &mut dyn Automation, elapsed_seconds: f64) {
        self.observe(automation);
        let available = automation.equipment_available();
        if available != self.was_available {
            self.was_available = available;
            self.clear();
            if available {
                self.seed_from_world(automation);
            }
        }
        if elapsed_seconds.is_finite() && elapsed_seconds > 0.0 {
            self.now += elapsed_seconds;
        }
        let mut index = 0;
        while index < self.settlements.len() {
            if self.settlements[index] > self.now {
                index += 1;
                continue;
            }
            self.settlements.remove(index);
            self.recently_swapped = false;
            self.settled = true;
        }
    }

    /// Whether gear settled since the last ask.
    pub fn take_settled(&mut self) -> bool {
        std::mem::take(&mut self.settled)
    }

    /// Starts a swap when none is cooling down (and, when asked, only in
    /// peace).
    pub fn try_arm_swap(
        &mut self,
        object_id: u32,
        require_peace: bool,
        effective_mode: CombatMode,
    ) -> bool {
        if self.recently_swapped() || (require_peace && effective_mode != CombatMode::Peace) {
            return false;
        }
        self.swap_cooldown_until = self.now + SWAP_COOLDOWN_SECONDS;
        self.swap_target_id = object_id;
        self.recently_swapped = true;
        true
    }

    fn seed_from_world(&mut self, automation: &mut dyn Automation) {
        for placement in automation.equipment_placements() {
            self.track_equipped(placement.object_id, placement.equipped_location, false);
        }
    }

    fn on_placement_observed(&mut self, observation: EquipmentObservation) {
        if observation.is_removal {
            self.track_removed(observation.object_id);
        } else {
            self.track_equipped(
                observation.object_id,
                observation.equipped_location,
                !observation.is_initial_placement,
            );
        }
    }

    fn track_equipped(&mut self, object_id: u32, location: u32, schedule_settlement: bool) {
        if is_weapon_slot(location) {
            self.weapon_id = object_id;
        }
        if location == AMMUNITION_SLOT {
            self.ammo_id = object_id;
        }
        if location == SHIELD_SLOT {
            self.shield_id = object_id;
        }
        if location != 0 && !self.equipped_ids.contains(&object_id) {
            self.equipped_ids.push(object_id);
        }
        if schedule_settlement {
            self.settlements.push(self.now + SETTLEMENT_SECONDS);
        }
    }

    fn track_removed(&mut self, object_id: u32) {
        if self.weapon_id == object_id {
            self.weapon_id = 0;
        }
        if self.ammo_id == object_id {
            self.ammo_id = 0;
        }
        if self.shield_id == object_id {
            self.shield_id = 0;
        }
        if let Some(index) = self.equipped_ids.iter().position(|&id| id == object_id) {
            self.equipped_ids.remove(index);
        }
        self.settlements.push(self.now + SETTLEMENT_SECONDS);
    }

    fn clear(&mut self) {
        self.weapon_id = 0;
        self.ammo_id = 0;
        self.shield_id = 0;
        self.equipped_ids.clear();
        self.settlements.clear();
        self.recently_swapped = false;
        self.swap_target_id = 0;
    }
}

/// The id while the client still knows the object, else 0.
fn valid_or_zero(automation: &mut dyn Automation, object_id: u32) -> u32 {
    if object_id == 0 {
        return 0;
    }
    if automation.objects_available() {
        return if automation.object(object_id).is_some() {
            object_id
        } else {
            0
        };
    }
    if automation
        .owned_equipment()
        .iter()
        .any(|item| item.object_id == object_id)
    {
        object_id
    } else {
        0
    }
}
