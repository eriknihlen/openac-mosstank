//! MossTank's own rule for this character's rare (`Looting.OwnRare.cs`),
//! behind the `walk_to_own_rare_corpses` option, off unless turned on: the
//! corpse whose description says this character's kill generated a rare is
//! walked to out to the rare reach, a corpse that appeared with this
//! character's rare announcement holds the walks off out to the same reach,
//! and while that rare corpse waits where the walk or the open will reach
//! it the route stands down. A walk to it that stops covering ground or
//! runs out of time is given up and the corpse set aside for the corpse
//! blacklist's time. With the option off everything is as the reference
//! has it.

use openac_plugin_compat::text::ordinal;

use crate::controller::LootController;
use crate::corpse_scan::{CORPSE_OPEN_RANGE_METERS, is_rare, killer_name};
use crate::host::{LootCorpse, LootHost};

/// How far this character's own rare is walked to: the longest corpse
/// approach range a MossTank profile can hold.
pub const RARE_CORPSE_REACH_METERS: f64 = 100.0;

impl LootController {
    /// How far the walk reaches for this corpse: the profile's range, or
    /// the rare reach when the option is on and the corpse holds this
    /// character's own rare.
    pub fn approach_reach_for(
        &self,
        host: &mut dyn LootHost,
        corpse: &LootCorpse,
        range_meters: f64,
    ) -> f64 {
        if self.is_own_rare_walk_goal(host, corpse) {
            range_meters.max(RARE_CORPSE_REACH_METERS)
        } else {
            range_meters
        }
    }

    /// Whether this corpse is one the option walks to and holds the route
    /// for.
    pub fn is_own_rare_walk_goal(&self, host: &mut dyn LootHost, corpse: &LootCorpse) -> bool {
        self.settings.walk_to_own_rare_corpses && self.is_own_rare_corpse(host, corpse)
    }

    /// Whether the walk's next pick is a corpse holding this character's
    /// own rare that the walk or the open will reach. Always false with the
    /// option off.
    pub fn has_own_rare_corpse_to_fetch(
        &mut self,
        host: &mut dyn LootHost,
        range_meters: f64,
    ) -> bool {
        if !self.settings.walk_to_own_rare_corpses {
            return false;
        }
        let Some(corpse) = self.try_select_approach_corpse(host, range_meters) else {
            return false;
        };
        let distance = f64::from(corpse.distance);
        corpse.has_position()
            && self.is_own_rare_corpse(host, &corpse)
            && (distance > self.settings.corpse_minimum_approach_range
                || distance <= CORPSE_OPEN_RANGE_METERS)
    }

    /// Gives up walking to this character's rare corpse: it is set aside
    /// for the corpse blacklist's time, which releases the route.
    pub fn give_up_own_rare_walk(&mut self, corpse_id: u32, reason: &str) {
        if corpse_id == 0 {
            return;
        }
        self.corpse_blacklisted_at.insert(corpse_id, self.lifetime);
        self.write_log(&format!(
            "LootCorpse: giving up the walk to this character's rare 0x{corpse_id:08X}: {reason}"
        ));
    }

    /// A described corpse whose description says this character killed it
    /// and the kill generated a rare.
    pub(crate) fn is_own_rare_corpse(&self, host: &mut dyn LootHost, corpse: &LootCorpse) -> bool {
        corpse.is_identified && is_rare(corpse) && self.is_own_kill(host, corpse)
    }

    /// Whether the corpse's description names this character as its killer.
    /// The local server writes the killer's name without the plus sign an
    /// admin character carries, so the character's own plus is not part of
    /// the comparison.
    pub(crate) fn is_own_kill(&self, host: &mut dyn LootHost, corpse: &LootCorpse) -> bool {
        let killer = killer_name(&corpse.long_description);
        let name = host.character().name;
        let character = name.trim_start_matches('+');
        !killer.is_empty()
            && !character.is_empty()
            && ordinal::equals_ignore_case(&killer, character)
    }
}
