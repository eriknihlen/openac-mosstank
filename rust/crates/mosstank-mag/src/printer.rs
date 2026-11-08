//! The identify chat line: an identified item's info with the loaded loot
//! profile's verdict (`+(Rule) info`), from Mag-Tools'
//! `ItemInfo/ItemInfoPrinter.cs` and its two triggers,
//! `DetectItemsIdentifiedByUser.cs` and `DetectItemsInContainers.cs`
//! (43ee2ec), with VTank's part (`VirindiTankLootRuleProcessor.cs`) played
//! by MossTank's own profile ([`LootVerdict`]).
//!
//! - **The player's identifies.** An appraisal that arrives within 10 s of
//!   selecting the object prints once, whoever asked for it; corpses,
//!   doors, foci, housing, lifestones, NPCs, portals and vendors are
//!   skipped. Without a profile the line is the info alone.
//! - **An opened container's items** (not a housing `Storage`): every
//!   100 ms for 5 s each item directly in it is weighed once a session;
//!   it prints only when a rule decided it, and a chest's, vault's or
//!   reliquary's salvage rules are not shown. An item the profile must
//!   appraise first is queued for an appraisal and printed when it
//!   arrives, while the container is open.
//!
//! Not ported: the `<Tell:IIDString:...>` wrapper around the verdict (a
//! plugin's chat line cannot carry it), the left-click identify and the
//! clipboard. Two changes for the per-call budget: the info text is built
//! only for a line that prints, and the container scan weighs
//! [`ITEMS_PER_THINK`] items a think (Mag-Tools: all at once) and keeps
//! thinking past the five seconds until every item listed is weighed.

use std::collections::{HashMap, HashSet};

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::{ItemProperties, ObjectClass, WorldObject};

use crate::host::{LootVerdict, MagHost, Verdict};
use crate::identify::IdentifyQueue;
use crate::item_info::{ItemInfoInput, item_info};

/// The chat text type of the line (`SendChatTextCategorized("IDs", text,
/// 14, ...)`; 0x0E, the client's Abuse colour).
pub const TEXT_TYPE: u32 = 14;
/// How long a selection waits for its appraisal, milliseconds.
pub const SELECTION_WINDOW_MS: f64 = 10_000.0;
/// The container scan's pace, milliseconds.
pub const THINK_INTERVAL_MS: f64 = 100.0;
/// How long the container scan runs once the container lists items,
/// milliseconds.
pub const SCAN_MS: f64 = 5_000.0;
/// Container items weighed a think (the looter weighs one a frame: an item
/// against a large profile is a call's worth of work).
pub const ITEMS_PER_THINK: usize = 1;

/// The `Mag.ItemInfoOnIdent` settings, as the line reads them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct PrintSettings {
    /// `Enabled`: print item info on identify.
    pub enabled: bool,
    /// `ShowBuffedValues`.
    pub show_buffed_values: bool,
    /// `ShowValueAndBurden`.
    pub show_value_and_burden: bool,
}

/// The client and the loot profile for one call, reached one at a time (in
/// MossTank both answer from the same client).
pub trait IdentCall {
    /// The client.
    fn host(&mut self) -> &mut dyn MagHost;
    /// The profile's verdict ([`LootVerdict::verdict`]).
    fn verdict(&mut self, item: &InventoryItem, properties: &ItemProperties) -> Option<Verdict>;
    /// The profile must appraise the item first
    /// ([`LootVerdict::needs_identify`]).
    fn needs_identify(&mut self, item: &InventoryItem, properties: &ItemProperties) -> bool;
}

/// A client and a loot profile held apart.
#[allow(missing_debug_implementations)]
pub struct Split<'a> {
    /// The client.
    pub host: &'a mut dyn MagHost,
    /// The profile.
    pub verdict: &'a mut dyn LootVerdict,
}

impl IdentCall for Split<'_> {
    fn host(&mut self) -> &mut dyn MagHost {
        self.host
    }

    fn verdict(&mut self, item: &InventoryItem, properties: &ItemProperties) -> Option<Verdict> {
        self.verdict.verdict(item, properties)
    }

    fn needs_identify(&mut self, item: &InventoryItem, properties: &ItemProperties) -> bool {
        self.verdict.needs_identify(item, properties)
    }
}

/// `+(Rule) info`, `-(Rule) info`, `+ info` / `- info` when no rule is
/// named, or `info` when no profile is loaded.
#[must_use]
pub fn line(verdict: Option<&Verdict>, info: &str) -> String {
    let Some(verdict) = verdict else {
        return info.to_owned();
    };
    let mut text = String::with_capacity(info.len() + verdict.rule_name.len() + 4);
    text.push(if verdict.keep { '+' } else { '-' });
    if !verdict.rule_name.is_empty() {
        text.push('(');
        text.push_str(&verdict.rule_name);
        text.push(')');
    }
    text.push(' ');
    text.push_str(info);
    text
}

/// The classes an identify by the player does not print for
/// (`DetectItemsIdentifiedByUser.cs:148-156`).
const SKIPPED_CLASSES: [ObjectClass; 8] = [
    ObjectClass::Corpse,
    ObjectClass::Door,
    ObjectClass::Foci,
    ObjectClass::Housing,
    ObjectClass::Lifestone,
    ObjectClass::Npc,
    ObjectClass::Portal,
    ObjectClass::Vendor,
];

/// The container names whose salvage rules are not shown.
const NO_SALVAGE_CONTAINERS: [&str; 3] = ["Chest", "Vault", "Reliquary"];

/// The two detectors' state.
#[derive(Debug, Default)]
pub struct Printer {
    /// The objects selected, with when (`itemsSelected`).
    selected: Vec<(u32, f64)>,
    /// When the container scan started; `None` when it is not running.
    scan_started_ms: Option<f64>,
    /// When it last thought (`lastThought`).
    last_think_ms: Option<f64>,
    /// The container items weighed this session (`itemsProcessed`).
    processed: HashSet<u32>,
    /// The container items waiting for their appraisal, with whether
    /// their container hides salvage rules.
    waiting_for_id: HashMap<u32, bool>,
}

impl Printer {
    /// How many selections wait for an appraisal.
    #[must_use]
    pub fn selected_count(&self) -> usize {
        self.selected.len()
    }

    /// Neither the container scan nor an awaited appraisal needs a turn.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.scan_started_ms.is_none() && self.waiting_for_id.is_empty()
    }

    /// An appraisal of this object would be read (selected, or awaited by
    /// the container scan).
    #[must_use]
    pub fn wants(&self, object_id: u32) -> bool {
        self.waiting_for_id.contains_key(&object_id)
            || self.selected.iter().any(|(id, _)| *id == object_id)
    }

    /// Forgets everything (the character left the world).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The client's selection changed (`ItemSelected`).
    pub fn on_selection(&mut self, selected: Option<u32>, now_ms: f64, enabled: bool) {
        if !enabled {
            return;
        }
        let Some(object_id) = selected.filter(|id| *id != 0) else {
            return;
        };
        match self.selected.iter_mut().find(|(id, _)| *id == object_id) {
            Some(entry) => entry.1 = now_ms,
            None => self.selected.push((object_id, now_ms)),
        }
    }

    /// An appraisal arrived (`ChangeObject` with `IdentReceived`).
    pub fn on_ident(
        &mut self,
        call: &mut dyn IdentCall,
        queue: &mut IdentifyQueue,
        object: &WorldObject,
        now_ms: f64,
        settings: PrintSettings,
    ) {
        let id = object.object_id;
        if settings.enabled {
            // Selections older than ten seconds go first.
            self.selected
                .retain(|(_, at)| at + SELECTION_WINDOW_MS >= now_ms);
            if let Some(index) = self.selected.iter().position(|(sel, _)| *sel == id) {
                self.selected.remove(index);
                if !SKIPPED_CLASSES.contains(&object.object_class) {
                    print_identified(call, object, settings);
                }
            }
        }
        // The profile's own wait (VTank's classify callback).
        if let Some(no_salvage) = self.waiting_for_id.remove(&id) {
            queue.remove(id);
            let host = call.host();
            if host.open_container() == 0 {
                return;
            }
            if let Some(item) = host.loot_contents().into_iter().find(|i| i.object_id == id) {
                let properties = host.properties(id).unwrap_or_default();
                print_container_item(call, &item, &properties, no_salvage, settings);
            }
        }
    }

    /// A container was opened (`ContainerOpened`), named `name`.
    pub fn on_container_opened(&mut self, name: &str, now_ms: f64, enabled: bool) {
        // Housing storage is not scanned.
        if !enabled || name == "Storage" {
            return;
        }
        // A second container does not restart the clock.
        if self.scan_started_ms.is_none() {
            self.scan_started_ms = Some(now_ms);
        }
    }

    /// One frame: the container scan's think, at most every 100 ms.
    pub fn tick(
        &mut self,
        call: &mut dyn IdentCall,
        queue: &mut IdentifyQueue,
        now_ms: f64,
        settings: PrintSettings,
    ) {
        if self.is_idle() {
            return;
        }
        if self
            .last_think_ms
            .is_some_and(|at| now_ms - at < THINK_INTERVAL_MS)
        {
            return;
        }
        self.last_think_ms = Some(now_ms);
        let open = call.host().open_container();
        if open == 0 {
            // Closed: the scan stops and the awaited items are forgotten.
            self.scan_started_ms = None;
            for (id, _) in self.waiting_for_id.drain() {
                queue.remove(id);
            }
            return;
        }
        if let Some(started) = self.scan_started_ms {
            self.think(call, queue, open, started, now_ms, settings);
        }
    }

    /// `Think`: weighs the open container's new items.
    fn think(
        &mut self,
        call: &mut dyn IdentCall,
        queue: &mut IdentifyQueue,
        open: u32,
        started_ms: f64,
        now_ms: f64,
        settings: PrintSettings,
    ) {
        let host = call.host();
        let contents: Vec<InventoryItem> = host
            .loot_contents()
            .into_iter()
            .filter(|item| item.container_id == open)
            .collect();
        // The contents can take a moment to arrive: no timeout meanwhile.
        if contents.is_empty() {
            return;
        }
        let no_salvage = host.object(open).is_some_and(|container| {
            NO_SALVAGE_CONTAINERS
                .iter()
                .any(|word| container.name.contains(word))
        });
        let mut weighed = 0;
        for item in &contents {
            if self.processed.contains(&item.object_id) {
                continue;
            }
            if weighed == ITEMS_PER_THINK {
                // More to weigh: the next think goes on.
                return;
            }
            weighed += 1;
            self.processed.insert(item.object_id);
            let host = call.host();
            let properties = host.properties(item.object_id).unwrap_or_default();
            let appraised = host
                .object(item.object_id)
                .is_some_and(|object| object.has_appraisal_data);
            if !appraised && call.needs_identify(item, &properties) {
                self.waiting_for_id.insert(item.object_id, no_salvage);
                queue.push(item.object_id);
                continue;
            }
            print_container_item(call, item, &properties, no_salvage, settings);
        }
        if now_ms - started_ms >= SCAN_MS {
            self.scan_started_ms = None;
        }
    }
}

/// The item record of an identified object: the owned item's, the open
/// container's, or what the world object itself says (a monster, an item
/// on the ground).
fn item_of(host: &mut dyn MagHost, object: &WorldObject) -> InventoryItem {
    let id = object.object_id;
    if object.is_owned
        && let Some(item) = host.owned_items().into_iter().find(|i| i.object_id == id)
    {
        return item;
    }
    if host.open_container() != 0
        && let Some(item) = host.loot_contents().into_iter().find(|i| i.object_id == id)
    {
        return item;
    }
    InventoryItem {
        object_id: id,
        weenie_class_id: object.weenie_class_id,
        name: object.name.clone(),
        item_type: object.item_type,
        container_id: object.container_id,
        wielder_id: object.wielder_id,
        stack_size: object.stack_size,
        items_capacity: object.items_capacity,
        containers_capacity: object.containers_capacity,
        appraised_spell_ids: object.spell_ids.clone(),
        object_class: object.object_class,
        icon_id: object.icon_id,
        coverage_mask: object.coverage_mask,
        ..InventoryItem::default()
    }
}

/// The player's identify: the line with the verdict, or the info alone
/// without a profile.
fn print_identified(call: &mut dyn IdentCall, object: &WorldObject, settings: PrintSettings) {
    let host = call.host();
    let item = item_of(host, object);
    let properties = host.properties(object.object_id).unwrap_or_default();
    let verdict = call.verdict(&item, &properties);
    post(call.host(), verdict.as_ref(), &item, &properties, settings);
}

/// A container item: printed only when a rule decided it, and not for a
/// hidden salvage rule.
fn print_container_item(
    call: &mut dyn IdentCall,
    item: &InventoryItem,
    properties: &ItemProperties,
    no_salvage: bool,
    settings: PrintSettings,
) {
    let Some(verdict) = call.verdict(item, properties) else {
        return;
    };
    if (no_salvage && verdict.salvage) || verdict.rule_name.is_empty() {
        return;
    }
    post(call.host(), Some(&verdict), item, properties, settings);
}

/// Builds the info text and posts the line.
fn post(
    host: &mut dyn MagHost,
    verdict: Option<&Verdict>,
    item: &InventoryItem,
    properties: &ItemProperties,
    settings: PrintSettings,
) {
    let input = ItemInfoInput {
        item,
        properties,
        show_buffed_values: settings.show_buffed_values,
        show_value_and_burden: settings.show_value_and_burden,
    };
    let info = item_info(&input, &mut |id| host.spell(id));
    host.chat_post(&line(verdict, &info), TEXT_TYPE);
}
