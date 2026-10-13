//! Reading the scrolls the looter picked up (`ScrollReading`,
//! `ReadScrollController`). A scroll is not read as a continuation of its
//! pickup: the looter records "this spell is waiting on that item", and
//! this rule reads it whenever it next wins a pass.

use ac_plugin_abi::items::{InventoryItem, ItemStatus};
use ac_plugin_abi::world::ObjectClass;
use mosstank_dotnet::collections::{DotNetDictionary, Ordinal};
use mosstank_settings::model::LootSettings;

use crate::controller::{LineSink, LootController};
use crate::host::LootHost;

/// How long a read waits for its answer.
const USE_TIMEOUT_SECONDS: f64 = 4.0;

/// Whether a scroll is worth reading: a scroll (by the client's class) the
/// profile reads, teaching a spell the character does not know and is
/// skilled enough to learn. The looter asks with `commit` set, so it will
/// not queue two scrolls teaching the same spell; the reading rule asks
/// with it clear, because by then the scroll is the queued one.
pub fn is_eligible(
    host: &mut dyn LootHost,
    settings: &LootSettings,
    item: &InventoryItem,
    queued: &DotNetDictionary<u32, u32, Ordinal<u32>>,
    commit: bool,
) -> bool {
    if item.object_class != ObjectClass::Scroll
        || !settings.read_unknown_scrolls
        || item.spell_id == 0
    {
        return false;
    }
    if host.is_spell_known(item.spell_id) {
        return false;
    }
    let Some(spell) = host.spells().try_get(item.spell_id) else {
        return false;
    };
    let Some(skill) = host.try_get_skill(spell.school) else {
        return false;
    };
    if i64::from(spell.difficulty) - 15 > i64::from(skill.current) {
        return false;
    }
    !commit || !queued.contains_key(&item.spell_id)
}

/// The rule that reads the queued scrolls.
#[derive(Default)]
pub struct ReadScrollController {
    /// The macro log's Loot channel.
    pub log: Option<LineSink>,
    pending_item: u32,
    pending_revision: i64,
    pending_age: f64,
}

impl std::fmt::Debug for ReadScrollController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadScrollController")
            .field("pending_item", &self.pending_item)
            .field("pending_revision", &self.pending_revision)
            .field("pending_age", &self.pending_age)
            .finish_non_exhaustive()
    }
}

impl ReadScrollController {
    /// A reader with nothing in flight.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The rule's turn; true when it holds the pass. No looting gate on
    /// purpose: the scrolls already queued are read whether looting stays
    /// on or not.
    pub fn tick(
        &mut self,
        host: &mut dyn LootHost,
        loot: &mut LootController,
        elapsed_seconds: f64,
        can_act: bool,
    ) -> bool {
        if !host.is_available() || !host.items_available() {
            self.clear();
            return false;
        }

        // The queue is the looter's, so anything that clears it drops an
        // in-flight read with it.
        let pending = self.pending_item;
        if pending != 0
            && !loot
                .pending_scroll_reads()
                .values()
                .any(|item| *item == pending)
        {
            self.clear();
        }
        if self.pending_item != 0 {
            return self.continue_read(host, elapsed_seconds);
        }
        if loot.pending_scroll_reads().is_empty() {
            return false;
        }
        if !can_act || host.items_busy() {
            return false;
        }

        let Some(scroll) = Self::choose(host, loot) else {
            return false;
        };
        let used = host.use_item(scroll.object_id);
        if !used.accepted() {
            return used.status == ItemStatus::Busy;
        }
        self.pending_item = scroll.object_id;
        self.pending_revision = host.last_item_completion().revision;
        self.pending_age = 0.0;
        if let Some(log) = self.log.as_mut() {
            log(&format!("ReadScroll: reading {}", scroll.name));
        }
        true
    }

    /// Waits out the read in flight. A read that fails or never answers
    /// changes nothing: the scroll is still held, so it comes round again.
    fn continue_read(&mut self, host: &mut dyn LootHost, elapsed_seconds: f64) -> bool {
        self.pending_age += elapsed_seconds.max(0.0);
        let completion = host.last_item_completion();
        if (completion.revision <= self.pending_revision
            || completion.source_id != self.pending_item)
            && self.pending_age < USE_TIMEOUT_SECONDS
        {
            return true;
        }
        self.clear();
        true
    }

    /// Drops the queued spells the character has since learned and the
    /// queued scrolls it no longer holds, then takes the first one left
    /// that is still worth reading.
    fn choose(host: &mut dyn LootHost, loot: &mut LootController) -> Option<InventoryItem> {
        loot.forget_known_scroll_reads(host);
        if loot.pending_scroll_reads().is_empty() {
            return None;
        }
        let owned = host.owned_items();
        loot.forget_unowned_scroll_reads(&owned);
        let queued: Vec<u32> = loot.pending_scroll_reads().values().copied().collect();
        for item_id in queued {
            if let Some(candidate) = owned
                .iter()
                .find(|candidate| candidate.object_id == item_id)
                && is_eligible(
                    host,
                    &loot.settings,
                    candidate,
                    loot.pending_scroll_reads(),
                    false,
                )
            {
                return Some(candidate.clone());
            }
        }
        None
    }

    fn clear(&mut self) {
        self.pending_item = 0;
        self.pending_revision = 0;
        self.pending_age = 0.0;
    }
}
