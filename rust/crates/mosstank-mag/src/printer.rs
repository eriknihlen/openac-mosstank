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
//! - **The left-click identify** (`LeftClickIdent`): a selection the mouse
//!   made asks for the object's appraisal ahead of everything waiting in
//!   the identify queue, so selecting it is enough to print its line. The
//!   client says which selections the mouse made (Mag-Tools took any
//!   selection within a second of a left button press). A right click that
//!   examines selects with the mouse too: the player's own appraisal holds
//!   the slot, the queue's request waits as busy, and the answer that
//!   arrives prints once and ends the wait.
//! - **An opened container's items** (not a housing `Storage`): every
//!   100 ms, until the first think at or after 5 s, each item directly in
//!   it is taken once a session; it prints only when a rule decided it,
//!   and a chest's, vault's or reliquary's salvage rules are not shown. An
//!   item the profile must appraise first is queued for an appraisal and
//!   printed when it arrives (wherever the item is by then; given up after
//!   [`WAIT_FOR_ID_MS`]).
//!
//! Not ported: the `<Tell:IIDString:...>` wrapper around the verdict (a
//! plugin's chat line cannot carry it) and the clipboard. Two changes for
//! the per-call budget: the info text is built only for a line that
//! prints, and a think only lists the new items into a backlog, weighed
//! [`ITEMS_PER_FRAME`] a frame from the record taken when it was listed
//! (Mag-Tools weighed them all in the think), so a full corpse can go on
//! printing past the five seconds; the items printed are the same.

use std::collections::{HashMap, HashSet, VecDeque};

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::{ItemProperties, ObjectClass, WorldObject, selection_source};

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
/// Listed container items weighed a frame (the looter weighs one a frame:
/// an item against a large profile is a call's worth of work).
pub const ITEMS_PER_FRAME: usize = 1;
/// How long a container item waits for the appraisal its profile needs,
/// milliseconds (VTank never gave up; this keeps the wait bounded).
pub const WAIT_FOR_ID_MS: f64 = 30_000.0;

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
    /// `LeftClickIdent`: a selection the mouse made asks for the object's
    /// appraisal.
    pub left_click_ident: bool,
}

/// The client and the loot profile for one call, reached one at a time (in
/// MossTank both answer from the same client).
pub trait IdentCall {
    /// The client.
    fn host(&mut self) -> &mut dyn MagHost;
    /// What the character owns, listed once a call.
    fn owned(&mut self) -> &[InventoryItem];
    /// The profile's verdict ([`LootVerdict::verdict`]).
    fn verdict(&mut self, item: &InventoryItem, properties: &ItemProperties) -> Option<Verdict>;
    /// The profile must appraise the item first
    /// ([`LootVerdict::needs_identify`]).
    fn needs_identify(&mut self, item: &InventoryItem, properties: &ItemProperties) -> bool;
}

/// A client and a loot profile held apart.
#[allow(missing_debug_implementations)]
pub struct Split<'a> {
    host: &'a mut dyn MagHost,
    verdict: &'a mut dyn LootVerdict,
    owned: Option<Vec<InventoryItem>>,
}

impl<'a> Split<'a> {
    /// The client and the profile.
    pub fn new(host: &'a mut dyn MagHost, verdict: &'a mut dyn LootVerdict) -> Self {
        Self {
            host,
            verdict,
            owned: None,
        }
    }
}

impl IdentCall for Split<'_> {
    fn host(&mut self) -> &mut dyn MagHost {
        self.host
    }

    fn owned(&mut self) -> &[InventoryItem] {
        let host = &mut *self.host;
        self.owned.get_or_insert_with(|| host.owned_items())
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

/// A container item waiting for the appraisal its profile needs.
#[derive(Debug, Clone)]
struct Waiting {
    /// The item as its container listed it.
    item: InventoryItem,
    /// Its container hides salvage rules.
    no_salvage: bool,
    /// When it started waiting.
    since_ms: f64,
}

/// The two detectors' state.
#[derive(Debug, Default)]
pub struct Printer {
    /// The objects selected, with when (`itemsSelected`).
    selected: Vec<(u32, f64)>,
    /// When the container scan started; `None` when it is not running.
    scan_started_ms: Option<f64>,
    /// When it last thought (`lastThought`).
    last_think_ms: Option<f64>,
    /// The container items taken this session (`itemsProcessed`).
    processed: HashSet<u32>,
    /// The items listed and not weighed yet, with whether their container
    /// hides salvage rules.
    backlog: VecDeque<(InventoryItem, bool)>,
    /// The container items waiting for their appraisal.
    waiting_for_id: HashMap<u32, Waiting>,
}

impl Printer {
    /// How many selections wait for an appraisal.
    #[must_use]
    pub fn selected_count(&self) -> usize {
        self.selected.len()
    }

    /// Neither the container scan, its backlog nor an awaited appraisal
    /// needs a turn.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.scan_started_ms.is_none() && self.backlog.is_empty() && self.waiting_for_id.is_empty()
    }

    /// Forgets everything (the character left the world).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The client's selection changed (`ItemSelected`), by `source` (one of
    /// [`selection_source`]): the selection waits for its appraisal, and
    /// one the mouse made, with the left-click identify on, asks for it
    /// ahead of everything waiting.
    pub fn on_selection(
        &mut self,
        queue: &mut IdentifyQueue,
        selected: Option<u32>,
        source: u8,
        now_ms: f64,
        settings: PrintSettings,
    ) {
        if !settings.enabled {
            return;
        }
        let Some(object_id) = selected.filter(|id| *id != 0) else {
            return;
        };
        match self.selected.iter_mut().find(|(id, _)| *id == object_id) {
            Some(entry) => entry.1 = now_ms,
            None => self.selected.push((object_id, now_ms)),
        }
        if settings.left_click_ident && source == selection_source::MOUSE {
            queue.push_first(object_id);
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
                // Whoever asked, the answer is here: a click's request still
                // waiting (refused as busy while the player's own examine
                // held the slot) is not sent again.
                queue.remove(id);
                if !SKIPPED_CLASSES.contains(&object.object_class) {
                    print_identified(call, object, settings);
                }
            }
        }
        // The profile's own wait (VTank's classify callback, which was
        // never cleared): printed wherever the item is now.
        if let Some(waiting) = self.waiting_for_id.remove(&id) {
            queue.remove(id);
            let item = fresh_record(call, object).unwrap_or(waiting.item);
            let properties = call.host().properties(id).unwrap_or_default();
            print_container_item(call, &item, &properties, waiting.no_salvage, settings);
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

    /// One frame: the container scan's think (at most every 100 ms), one
    /// listed item weighed, and the waits given up after 30 s.
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
        let think_due = self
            .last_think_ms
            .is_none_or(|at| now_ms - at >= THINK_INTERVAL_MS);
        if think_due && (self.scan_started_ms.is_some() || !self.waiting_for_id.is_empty()) {
            self.last_think_ms = Some(now_ms);
            let open = call.host().open_container();
            if open == 0 {
                // Closed: the scan stops, and the awaited items are no
                // longer asked for (their appraisals still print).
                self.scan_started_ms = None;
                for id in self.waiting_for_id.keys() {
                    queue.remove(*id);
                }
            } else if let Some(started) = self.scan_started_ms {
                self.think(call.host(), open, started, now_ms);
            }
        }
        for _ in 0..ITEMS_PER_FRAME {
            let Some((item, no_salvage)) = self.backlog.pop_front() else {
                break;
            };
            self.weigh(call, queue, &item, no_salvage, now_ms, settings);
        }
        if !self.waiting_for_id.is_empty() {
            self.waiting_for_id.retain(|id, waiting| {
                let keep = now_ms - waiting.since_ms < WAIT_FOR_ID_MS;
                if !keep {
                    queue.remove(*id);
                }
                keep
            });
        }
    }

    /// `Think`: lists the open container's new items into the backlog.
    fn think(&mut self, host: &mut dyn MagHost, open: u32, started_ms: f64, now_ms: f64) {
        let contents = host.loot_contents();
        // The contents can take a moment to arrive: no timeout meanwhile.
        if !contents.iter().any(|item| item.container_id == open) {
            return;
        }
        let no_salvage = host.object(open).is_some_and(|container| {
            NO_SALVAGE_CONTAINERS
                .iter()
                .any(|word| container.name.contains(word))
        });
        for item in contents {
            if item.container_id == open && self.processed.insert(item.object_id) {
                self.backlog.push_back((item, no_salvage));
            }
        }
        if now_ms - started_ms >= SCAN_MS {
            self.scan_started_ms = None;
        }
    }

    /// One listed item: printed now, or queued for the appraisal its
    /// profile needs.
    fn weigh(
        &mut self,
        call: &mut dyn IdentCall,
        queue: &mut IdentifyQueue,
        item: &InventoryItem,
        no_salvage: bool,
        now_ms: f64,
        settings: PrintSettings,
    ) {
        let host = call.host();
        let properties = host.properties(item.object_id).unwrap_or_default();
        let appraised = host
            .object(item.object_id)
            .is_some_and(|object| object.has_appraisal_data);
        if !appraised && call.needs_identify(item, &properties) {
            self.waiting_for_id.insert(
                item.object_id,
                Waiting {
                    item: item.clone(),
                    no_salvage,
                    since_ms: now_ms,
                },
            );
            // Asked for only while a container is open.
            if call.host().open_container() != 0 {
                queue.push_urgent(item.object_id);
            }
            return;
        }
        print_container_item(call, item, &properties, no_salvage, settings);
    }
}

/// The client's current record of an object: the owned item's or the open
/// container's.
fn fresh_record(call: &mut dyn IdentCall, object: &WorldObject) -> Option<InventoryItem> {
    let id = object.object_id;
    if object.is_owned
        && let Some(item) = call.owned().iter().find(|i| i.object_id == id)
    {
        return Some(item.clone());
    }
    let host = call.host();
    if host.open_container() != 0 {
        return host.loot_contents().into_iter().find(|i| i.object_id == id);
    }
    None
}

/// The item record of an identified object: the client's current one, or
/// what the world object itself says (a monster, an item on the ground).
fn item_of(call: &mut dyn IdentCall, object: &WorldObject) -> InventoryItem {
    fresh_record(call, object).unwrap_or_else(|| InventoryItem {
        object_id: object.object_id,
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
    })
}

/// The player's identify: the line with the verdict, or the info alone
/// without a profile.
fn print_identified(call: &mut dyn IdentCall, object: &WorldObject, settings: PrintSettings) {
    let item = item_of(call, object);
    let properties = call.host().properties(object.object_id).unwrap_or_default();
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
