//! The rules' view of the client ([`Automation`]) over the SDK's host, for
//! the length of one plugin call.
//!
//! # What a call reads once
//!
//! The C# rules read the character, its skills, spellbook and pack, and
//! the objects about from objects in the plugin's own memory, so they read
//! them again and again within one pass. Through the client every read is
//! a call that copies and decodes the whole answer, so this view keeps what
//! it read for the rest of its call ([`CallReads`]): the answer the client
//! would give again, for as long as nothing could have changed it.
//!
//! - The client's state changes between plugin calls (the server's
//!   messages are handled on the session's thread between them) and within
//!   a call only through the plugin's own calls: a cast, a use, a move, a
//!   selection and the like act on the client's state at once.
//! - So any call this view does not know to be a pure read, and any use of
//!   the host by another view built on this one ([`HostHandle`]), forgets
//!   everything kept; the next read asks the client again.
//! - The reads it keeps, and the reads it makes between them without
//!   forgetting, are answered by the client's `&self` handlers
//!   (`character_info`, `character_skills`, `items_owned`, `world_object`,
//!   ...): they read the session's state and change nothing. So are the
//!   plugin host's own services (storage, logs, windows, the clock:
//!   [`HostAutomation::service`]), the client's enchantment tracker, its
//!   peer board and its spellbook's cooldown table, which change only
//!   themselves, and a selection (a cast gate may make one too): no read
//!   kept here comes from any of them.
//! - The views of one call share what they read ([`KeptReads`]); the
//!   plugin forgets it when each call begins and ends, and when it uses
//!   the host itself between two views.
//!
//! The plugin's actions, their order and what each one is told are
//! unchanged; only a repeat of an identical read is answered from the copy.
//!
//! The spell table is the client's static data: it is read once, when the
//! client first hands it out, and kept by the plugin ([`KeptReads`]).
//!
//! The known-spell lists are kept across calls by the client's revision of
//! them (`spells_known_revision`): the same revision is the same lists, so
//! a list is read again only when the revision moved. The revision is asked
//! once a call, and again after anything that forgets the call's reads. The
//! lists are handed out shared (`Rc`), not copied.

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::character::{ActiveEnchantment, Attribute, Skill};
use ac_plugin_abi::combat::{AttackHeight, CombatMode, CombatResult, CombatSnapshot, CombatTarget};
use ac_plugin_abi::items::{
    EquipmentItem, EquipmentObservation, EquipmentPlacement, EquipmentResult, InventoryCompletion,
    InventoryItem, ItemResult, ItemUseCompletion,
};
use ac_plugin_abi::magic::{
    CastCompletion, CastGate, CastResult, ComponentInfo, SpellInfo, SpellList, TrackedEnchantment,
};
use ac_plugin_abi::navigation::{
    MovementIntent, NavigationObject, NavigationSnapshot, NavigationStatus,
};
use ac_plugin_abi::peers::{NetworkClient, PeerCast};
use ac_plugin_abi::projectile::{ProjectilePathQuery, ProjectilePathResult};
use ac_plugin_abi::social::{BusyState, FellowMember, FellowshipStatus, SelectionAction};
use ac_plugin_abi::types::ChatLine;
use ac_plugin_abi::ui::ProjectileSample;
use ac_plugin_abi::world::{ItemProperties, Position, WorldObject};
use mosstank_automation::{Automation, SpellCatalog, SpellTable};
use openac_plugin::Host;

/// The SDK's host as the views of one call share it: any use of it through
/// [`DerefMut`] (every client call takes the host mutably) is noted, so the
/// view forgets its kept reads before its next read.
#[derive(Debug)]
pub struct HostHandle<'h, 'a> {
    host: &'h mut Host<'a>,
    used: bool,
}

impl<'a> Deref for HostHandle<'_, 'a> {
    type Target = Host<'a>;

    fn deref(&self) -> &Host<'a> {
        self.host
    }
}

impl<'a> DerefMut for HostHandle<'_, 'a> {
    fn deref_mut(&mut self) -> &mut Host<'a> {
        self.used = true;
        self.host
    }
}

/// What one view has read in its call (see the module documentation).
#[derive(Debug, Default)]
struct CallReads {
    character: Option<Option<CharacterInfo>>,
    skills: Option<Vec<Skill>>,
    skill: HashMap<u32, Option<Skill>>,
    /// The known-spell lists' revision, as the client said it in this call.
    known_revision: Option<u64>,
    owned_items: Option<Vec<InventoryItem>>,
    objects: HashMap<u32, Option<WorldObject>>,
    components: HashMap<u32, Option<ComponentInfo>>,
    has_components: HashMap<u32, bool>,
}

/// The known-spell lists as the client last handed them out, kept for as
/// long as the client's revision of them is the same: the same revision
/// means the same lists, so a list kept at it is the answer a fresh read
/// would give.
#[derive(Debug, Default)]
struct KnownLists {
    /// The revision the kept lists belong to.
    revision: Option<u64>,
    /// Self buffs, attacks, combat spells, once read at `revision`.
    spells: [Option<Rc<[SpellInfo]>>; 3],
    /// Their ids, in the same order.
    ids: [Option<Rc<[u32]>>; 3],
}

impl KnownLists {
    fn slot(list: SpellList) -> usize {
        match list {
            SpellList::SelfBuffs => 0,
            SpellList::Attacks => 1,
            SpellList::Combat => 2,
        }
    }
}

/// Where the known-spell lists are kept: by the plugin between its calls,
/// or by this view alone.
#[derive(Debug)]
enum KnownListsSlot<'h> {
    Kept(&'h mut KnownLists),
    Own(KnownLists),
}

impl KnownListsSlot<'_> {
    fn get(&mut self) -> &mut KnownLists {
        match self {
            Self::Kept(lists) => lists,
            Self::Own(lists) => lists,
        }
    }
}

/// Where the spell table is kept: by the plugin between its calls, or by
/// this view alone.
#[derive(Debug)]
enum SpellTableSlot<'h> {
    Kept(&'h mut Option<Rc<SpellTable>>),
    Own(Option<Rc<SpellTable>>),
}

impl SpellTableSlot<'_> {
    fn get(&mut self) -> &mut Option<Rc<SpellTable>> {
        match self {
            Self::Kept(slot) => slot,
            Self::Own(slot) => slot,
        }
    }
}

/// What the plugin keeps for the client's views: the spell table and the
/// known-spell lists (by the client's revision of them) between its calls,
/// and what the views of one call have read, shared by them until the call
/// ends ([`Self::end_call`]).
#[derive(Debug, Default)]
pub struct KeptReads {
    /// The spell table, once the client has handed it out.
    spell_table: Option<Rc<SpellTable>>,
    /// The known-spell lists, by the client's revision of them.
    known: KnownLists,
    /// What the views of the call under way have read.
    call: CallReads,
}

impl KeptReads {
    /// A plugin call begins or ends, or the plugin used the host itself
    /// between two views: what the views read is forgotten (the client
    /// may change between calls, and an action may change it).
    pub fn end_call(&mut self) {
        self.call = CallReads::default();
    }
}

/// Where the reads of a call are kept: shared by the plugin's views of
/// one call, or by this view alone.
#[derive(Debug)]
enum ReadsSlot<'h> {
    Kept(&'h mut CallReads),
    Own(Box<CallReads>),
}

impl ReadsSlot<'_> {
    fn get(&mut self) -> &mut CallReads {
        match self {
            Self::Kept(reads) => reads,
            Self::Own(reads) => reads,
        }
    }
}

/// The client for one call, as every part of the macro sees it: the rules
/// ([`Automation`]), the looter ([`mosstank_loot::host::LootHost`]), the
/// route ([`mosstank_nav::NavigationAutomation`]), the meta
/// ([`mosstank_meta::host::MetaHost`]) and the profile stores
/// ([`mosstank_stores::storage::StoreHost`]) all reach the one SDK host
/// through it.
#[derive(Debug)]
pub struct HostAutomation<'h, 'a> {
    pub(crate) host: HostHandle<'h, 'a>,
    /// What the views of this call have read.
    reads: ReadsSlot<'h>,
    /// The spell table, once the client has handed it out.
    spell_table: SpellTableSlot<'h>,
    /// The known-spell lists, by the client's revision of them.
    known: KnownListsSlot<'h>,
    /// The placements the client announced since the last read
    /// (`Event::EquipmentPlacement`), kept by the plugin between calls.
    observations: Option<&'h mut Vec<EquipmentObservation>>,
    /// Which storage the last storage accessor handed out: the meta's own
    /// (always there) or a profile store's place.
    pub(crate) storage_view: StorageView,
    /// How each kind of UB line is shown, once the UB settings said so.
    pub(crate) ub_displays: Option<[mosstank_meta::host::UbDisplay; 4]>,
    /// The windows the meta draws, kept by the plugin between calls; with
    /// none, the meta's windows are not drawn.
    pub(crate) meta_windows: Option<&'h mut crate::meta_windows::MetaWindows>,
}

/// Which storage a [`HostAutomation`] answers as.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StorageView {
    /// The meta's storage: the plugin's own data, always available.
    Meta,
    /// A profile store's storage at a place, available when the client
    /// describes it.
    Store(ac_plugin_abi::storage::StoragePlace),
}

impl<'h, 'a> HostAutomation<'h, 'a> {
    /// The rules' view of `host`.
    pub fn new(host: &'h mut Host<'a>) -> Self {
        Self {
            host: HostHandle { host, used: false },
            reads: ReadsSlot::Own(Box::default()),
            spell_table: SpellTableSlot::Own(None),
            known: KnownListsSlot::Own(KnownLists::default()),
            observations: None,
            storage_view: StorageView::Meta,
            ub_displays: None,
            meta_windows: None,
        }
    }

    /// The same view, reading the equipment placements the plugin kept
    /// from its events.
    pub fn with_equipment_observations(
        host: &'h mut Host<'a>,
        observations: &'h mut Vec<EquipmentObservation>,
    ) -> Self {
        let mut client = Self::new(host);
        client.observations = Some(observations);
        client
    }

    /// The plugin's own call: the equipment placements, the meta's windows
    /// and the reads it keeps between calls.
    pub fn for_call(
        host: &'h mut Host<'a>,
        observations: &'h mut Vec<EquipmentObservation>,
        meta_windows: &'h mut crate::meta_windows::MetaWindows,
        kept: &'h mut KeptReads,
    ) -> Self {
        let mut client = Self::with_equipment_observations(host, observations);
        client.meta_windows = Some(meta_windows);
        let KeptReads {
            spell_table,
            known,
            call,
        } = kept;
        client.spell_table = SpellTableSlot::Kept(spell_table);
        client.known = KnownListsSlot::Kept(known);
        client.reads = ReadsSlot::Kept(call);
        client
    }

    /// The host, for the other views of the client built on this one.
    pub(crate) fn host(&mut self) -> &mut Host<'a> {
        &mut self.host
    }

    /// The host for the plugin host's own services: storage, logs, the
    /// clock and settings, the plugin's windows, canvases and images, its
    /// status line and its loot classifiers. The plugin host answers them
    /// itself and none reads or changes the game, so using them keeps the
    /// call's reads.
    pub(crate) fn service(&mut self) -> &mut Host<'a> {
        self.host.host
    }

    /// The reads kept, forgotten first if the host was used for anything
    /// else since they were made.
    fn kept(&mut self) -> &mut CallReads {
        if std::mem::take(&mut self.host.used) {
            *self.reads.get() = CallReads::default();
        }
        self.reads.get()
    }

    /// The host for a read the client answers without changing anything:
    /// using it does not forget the kept reads.
    pub(crate) fn reader(&mut self) -> &mut Host<'a> {
        if std::mem::take(&mut self.host.used) {
            *self.reads.get() = CallReads::default();
        }
        self.host.host
    }

    /// The known-spell lists' revision, asked once a call (and again after
    /// anything that forgets the call's reads); the kept lists are dropped
    /// when it moved.
    fn known_revision(&mut self) -> u64 {
        if let Some(revision) = self.kept().known_revision {
            return revision;
        }
        let revision = self.reader().known_spells_revision();
        self.reads.get().known_revision = Some(revision);
        let lists = self.known.get();
        if lists.revision != Some(revision) {
            *lists = KnownLists {
                revision: Some(revision),
                ..KnownLists::default()
            };
        }
        revision
    }

    /// A known-spell list, shared: read once a revision of the lists.
    pub(crate) fn known_shared(&mut self, list: SpellList) -> Rc<[SpellInfo]> {
        self.known_revision();
        let slot = KnownLists::slot(list);
        if let Some(spells) = &self.known.get().spells[slot] {
            return Rc::clone(spells);
        }
        let spells: Rc<[SpellInfo]> = self.reader().known_spells(list).into();
        self.known.get().spells[slot] = Some(Rc::clone(&spells));
        spells
    }

    /// A known-spell list's ids, in its order: from the kept list when
    /// there is one, else read as ids alone; once a revision of the lists.
    pub(crate) fn known_ids(&mut self, list: SpellList) -> Rc<[u32]> {
        self.known_revision();
        let slot = KnownLists::slot(list);
        let lists = self.known.get();
        if let Some(ids) = &lists.ids[slot] {
            return Rc::clone(ids);
        }
        let ids: Rc<[u32]> = match &lists.spells[slot] {
            Some(spells) => spells.iter().map(|spell| spell.spell_id).collect(),
            None => self.reader().known_spell_ids(list).into(),
        };
        self.known.get().ids[slot] = Some(Rc::clone(&ids));
        ids
    }

    /// A known-spell list, copied.
    pub(crate) fn known_list(&mut self, list: SpellList) -> Vec<SpellInfo> {
        self.known_shared(list).to_vec()
    }

    /// The character, kept for the rest of the call.
    pub(crate) fn character_info(&mut self) -> Option<CharacterInfo> {
        if let Some(character) = &self.kept().character {
            return character.clone();
        }
        let character = self.reader().character();
        self.reads.get().character = Some(character.clone());
        character
    }
}

/// A view that used the host for anything but a read the call keeps leaves
/// the call's reads forgotten for the views after it.
impl Drop for HostAutomation<'_, '_> {
    fn drop(&mut self) {
        if self.host.used {
            *self.reads.get() = CallReads::default();
        }
    }
}

impl SpellCatalog for HostAutomation<'_, '_> {
    fn known_self_buffs(&mut self) -> Vec<SpellInfo> {
        self.known_list(SpellList::SelfBuffs)
    }

    fn try_get(&mut self, spell_id: u32) -> Option<SpellInfo> {
        self.reader().spell(spell_id)
    }

    /// From the kept list or its ids, without copying its spells.
    fn known_self_buff_ids(&mut self) -> Vec<u32> {
        self.known_ids(SpellList::SelfBuffs).to_vec()
    }

    fn known_attack_spells(&mut self) -> Vec<SpellInfo> {
        self.known_list(SpellList::Attacks)
    }

    fn known_combat_spells(&mut self) -> Vec<SpellInfo> {
        self.known_list(SpellList::Combat)
    }

    fn known_self_buffs_shared(&mut self) -> Rc<[SpellInfo]> {
        self.known_shared(SpellList::SelfBuffs)
    }

    fn known_attack_spells_shared(&mut self) -> Rc<[SpellInfo]> {
        self.known_shared(SpellList::Attacks)
    }

    fn known_combat_spells_shared(&mut self) -> Rc<[SpellInfo]> {
        self.known_shared(SpellList::Combat)
    }

    fn is_known(&mut self, spell_id: u32) -> bool {
        self.reader().is_spell_known(spell_id)
    }

    fn cooldown_remaining(&mut self, cooldown_id: u32) -> f64 {
        // Reading it drops an expired cooldown from the spellbook's
        // cooldown table, which no read kept here holds.
        self.reader().cooldown_remaining(cooldown_id)
    }

    fn all(&mut self) -> Vec<SpellInfo> {
        self.table().spells().to_vec()
    }

    /// The table as the client first handed it out: the client's static
    /// data (read from its game files), the same on every read once the
    /// character is in the world. An empty answer (no character yet) is not
    /// kept, so the next lookup asks again, as every lookup did.
    fn table(&mut self) -> Rc<SpellTable> {
        if let Some(table) = self.spell_table.get() {
            return Rc::clone(table);
        }
        let table = Rc::new(SpellTable::new(self.reader().all_spells()));
        if !table.is_empty() {
            *self.spell_table.get() = Some(Rc::clone(&table));
        }
        table
    }

    fn try_find_by_name(&mut self, name: &str, partial_match: bool) -> Option<SpellInfo> {
        self.table().find_by_name(name, partial_match).cloned()
    }

    fn try_get_component(&mut self, component_id: u32) -> Option<ComponentInfo> {
        if let Some(component) = self.kept().components.get(&component_id) {
            return component.clone();
        }
        let component = self.reader().spell_component(component_id);
        self.reads
            .get()
            .components
            .insert(component_id, component.clone());
        component
    }
}

impl Automation for HostAutomation<'_, '_> {
    /// In the world: the client has a character to describe.
    fn is_available(&mut self) -> bool {
        self.character_info().is_some()
    }

    fn character(&mut self) -> CharacterInfo {
        self.character_info().unwrap_or_default()
    }

    fn skills(&mut self) -> Vec<Skill> {
        if let Some(skills) = &self.kept().skills {
            return skills.clone();
        }
        let skills = self.reader().skills();
        self.reads.get().skills = Some(skills.clone());
        skills
    }

    fn try_get_skill(&mut self, skill_id: u32) -> Option<Skill> {
        if let Some(skill) = self.kept().skill.get(&skill_id) {
            return skill.clone();
        }
        let skill = self.reader().skill(skill_id);
        self.reads.get().skill.insert(skill_id, skill.clone());
        skill
    }

    fn spells(&mut self) -> &mut dyn SpellCatalog {
        self
    }

    /// The SDK's target 0 is the selection. Asking may select the target,
    /// which none of the kept reads holds, so it keeps them.
    fn evaluate_gate(&mut self, spell_id: u32, target: Option<u32>) -> CastGate {
        self.reader().cast_gate(spell_id, target.unwrap_or(0))
    }

    fn cast(&mut self, spell_id: u32, target: Option<u32>) -> bool {
        self.host.cast(spell_id, target.unwrap_or(0)) == CastResult::Sent
    }

    fn has_components(&mut self, spell_id: u32) -> bool {
        if let Some(has) = self.kept().has_components.get(&spell_id) {
            return *has;
        }
        let has = self.reader().has_components(spell_id);
        self.reads.get().has_components.insert(spell_id, has);
        has
    }

    fn last_cast_completion(&mut self) -> CastCompletion {
        // Reading it also files the cast with the client's enchantment
        // tracker, which no kept read holds.
        self.reader().last_cast()
    }

    fn combat_mode(&mut self) -> CombatMode {
        self.reader().combat().mode
    }

    fn enter_combat_mode(&mut self, mode: CombatMode) -> CombatResult {
        self.host.enter_combat_mode(mode)
    }

    fn items_available(&mut self) -> bool {
        self.reader().item_state().is_available
    }

    fn items_busy(&mut self) -> bool {
        self.reader().item_state().is_busy
    }

    fn owned_items(&mut self) -> Vec<InventoryItem> {
        if let Some(items) = &self.kept().owned_items {
            return items.clone();
        }
        let items = self.reader().owned_items();
        self.reads.get().owned_items = Some(items.clone());
        items
    }

    fn last_item_completion(&mut self) -> ItemUseCompletion {
        self.reader().item_state().last_completion
    }

    fn use_item(&mut self, object_id: u32) -> ItemResult {
        self.host.use_item(object_id)
    }

    fn apply_item(&mut self, object_id: u32, target_id: u32) -> ItemResult {
        self.host.apply_item(object_id, target_id)
    }

    fn item_properties(&mut self, object_id: u32) -> Option<ItemProperties> {
        self.reader().item_properties(object_id)
    }

    /// The object table is there whenever the character is.
    fn objects_available(&mut self) -> bool {
        self.is_available()
    }

    fn object(&mut self, object_id: u32) -> Option<WorldObject> {
        if let Some(object) = self.kept().objects.get(&object_id) {
            return object.clone();
        }
        let object = self.reader().object(object_id);
        self.reads.get().objects.insert(object_id, object.clone());
        object
    }

    fn identify(&mut self, object_id: u32) -> ItemResult {
        self.host.identify(object_id)
    }

    fn in_fellowship(&mut self) -> bool {
        self.reader().fellowship().is_in_fellowship
    }

    fn fellowship_members(&mut self) -> Vec<FellowMember> {
        self.host.fellowship_members()
    }

    fn request_fellow_vitals(&mut self, requested: bool) -> FellowshipStatus {
        self.host.fellowship_request_vitals(requested)
    }

    fn network_available(&mut self) -> bool {
        self.host.peers_connected()
    }

    fn network_clients(&mut self) -> Vec<NetworkClient> {
        self.host.peer_clients()
    }

    fn position(&mut self) -> Position {
        self.reader().navigation().position
    }

    fn chat_messages(&mut self, after_sequence: u64) -> Vec<ChatLine> {
        self.reader().chat_captured(after_sequence)
    }

    fn post_system_message(&mut self, text: &str) {
        self.reader().chat_post(text);
    }

    fn log_info(&mut self, text: &str) {
        self.reader().info(text);
    }

    fn log_warn(&mut self, text: &str) {
        self.reader().warn(text);
    }

    fn chat_input_active(&mut self) -> bool {
        self.reader().chat_input_active()
    }

    fn is_casting(&mut self) -> bool {
        self.reader().is_casting()
    }

    /// The SDK's target 0 is the selection.
    fn request_cast(&mut self, spell_id: u32, target: Option<u32>) -> CastResult {
        self.host.cast(spell_id, target.unwrap_or(0))
    }

    fn combat_snapshot(&mut self) -> CombatSnapshot {
        self.reader().combat()
    }

    fn move_item(&mut self, object_id: u32, container_id: u32) -> ItemResult {
        self.host.move_item(object_id, container_id, 0, 0)
    }

    fn split_stack(&mut self, object_id: u32, container_id: u32, amount: u32) -> ItemResult {
        self.host.move_item(object_id, container_id, amount, 0)
    }

    fn inventory_completion(&mut self) -> InventoryCompletion {
        self.reader().item_state().last_inventory_completion
    }

    fn capture_objects(&mut self) -> Vec<WorldObject> {
        self.reader().objects()
    }

    fn navigation_snapshot(&mut self) -> NavigationSnapshot {
        self.reader().navigation()
    }

    fn navigation_object(&mut self, object_id: u32) -> Option<NavigationObject> {
        self.reader().navigation_object(object_id)
    }

    fn set_movement_intent(&mut self, intent: MovementIntent) -> NavigationStatus {
        self.host.set_movement_intent(intent)
    }

    fn clear_movement_intent(&mut self) -> NavigationStatus {
        self.host.clear_movement_intent()
    }

    fn face_heading(&mut self, heading_degrees: f32) -> NavigationStatus {
        self.host.face_heading(heading_degrees)
    }

    fn selected_object(&mut self) -> Option<u32> {
        self.reader().selection().selected
    }

    fn attributes(&mut self) -> Vec<Attribute> {
        self.reader().attributes()
    }

    fn active_enchantments(&mut self) -> Vec<ActiveEnchantment> {
        self.reader().enchantments()
    }

    fn timed_enchantments(&mut self) -> Vec<ActiveEnchantment> {
        self.reader().timed_enchantments()
    }

    fn hostile_targets(&mut self, maximum_metres: f32) -> Vec<CombatTarget> {
        self.reader().hostile_targets(maximum_metres)
    }

    fn check_room_ahead(&mut self, distance_meters: f32) -> mosstank_automation::RoomAhead {
        use ac_plugin_abi::navigation::RoomAheadStatus;
        match self.host.check_room_ahead(distance_meters).status {
            RoomAheadStatus::Clear => mosstank_automation::RoomAhead::Clear,
            RoomAheadStatus::Blocked => mosstank_automation::RoomAhead::Blocked,
            _ => mosstank_automation::RoomAhead::Unknown,
        }
    }

    fn begin_attack(&mut self, target_id: u32, height: AttackHeight, power: f32) -> CombatResult {
        self.host.begin_attack(target_id, height, power)
    }

    fn release_attack(&mut self) -> CombatResult {
        self.host.release_attack()
    }

    fn abort_attack(&mut self) -> CombatResult {
        self.host.abort_attack()
    }

    fn dismiss_ghost(&mut self, target_id: u32) -> CombatResult {
        self.host.dismiss_ghost(target_id)
    }

    fn combat_control(&mut self, acquire: bool) -> bool {
        self.host.combat_control(acquire)
    }

    /// Equipping is wired whenever the character is in the world.
    fn equipment_available(&mut self) -> bool {
        self.is_available()
    }

    /// An equip the client started is still in flight (OpenAC's
    /// `IsAutoWieldBusy`).
    fn equipment_busy(&mut self) -> bool {
        self.reader().equipment_busy()
    }

    fn owned_equipment(&mut self) -> Vec<EquipmentItem> {
        self.reader().owned_equipment()
    }

    fn equipment_placements(&mut self) -> Vec<EquipmentPlacement> {
        self.reader().equipment_placements()
    }

    fn take_equipment_observations(&mut self) -> Vec<EquipmentObservation> {
        self.observations
            .as_mut()
            .map(|kept| std::mem::take(&mut **kept))
            .unwrap_or_default()
    }

    /// Slot 0: the client chooses where it goes.
    fn equip(&mut self, object_id: u32) -> EquipmentResult {
        self.host.equip(object_id, 0)
    }

    fn equip_secondary(&mut self, object_id: u32) -> EquipmentResult {
        self.host.equip_secondary(object_id)
    }

    fn busy_state(&mut self) -> BusyState {
        self.reader().busy_state()
    }

    fn projectiles_available(&mut self) -> bool {
        self.reader().projectiles_available()
    }

    fn evaluate_projectile_path(&mut self, query: &ProjectilePathQuery) -> ProjectilePathResult {
        self.host.evaluate_projectile_path(query)
    }

    fn show_projectile_samples(&mut self, samples: &[ProjectileSample]) {
        self.host.show_projectile_samples(samples);
    }

    /// The selection is not among the kept reads, so selecting keeps them.
    fn select(&mut self, object_id: u32) -> bool {
        self.reader().select(object_id)
    }

    fn clear_selection(&mut self) -> bool {
        self.reader().clear_selection()
    }

    fn selection_step(&mut self, action: SelectionAction) -> bool {
        self.reader().selection_step(action)
    }

    fn tracked_enchantments(&mut self, target_id: u32) -> Vec<TrackedEnchantment> {
        self.reader().tracked_enchantments(target_id)
    }

    fn report_cast(&mut self, target_id: u32, spell_id: u32, duration_seconds: f64) -> bool {
        self.reader()
            .report_cast(target_id, spell_id, duration_seconds)
    }

    fn forget_enchantments(&mut self, target_id: u32) {
        self.reader().forget_enchantments(target_id);
    }

    fn announce_cast_attempt(&mut self, target_id: u32, spell_id: u32, skill: i32) -> bool {
        self.reader()
            .announce_cast_attempt(target_id, spell_id, skill)
    }

    fn announce_cast_success(
        &mut self,
        target_id: u32,
        spell_id: u32,
        skill: i32,
        duration_seconds: f64,
    ) -> bool {
        self.reader()
            .announce_cast_success(target_id, spell_id, skill, duration_seconds)
    }

    fn peer_casts(&mut self, after_sequence: i64) -> Vec<PeerCast> {
        self.reader().peer_casts(after_sequence)
    }
}

#[cfg(test)]
mod tests {
    use ac_plugin_abi::calls;
    use ac_plugin_abi::magic::CastRequest;
    use openac_plugin::testing::FakeHost;
    use openac_plugin::{Plugin, StartInfo};

    use super::*;

    /// A plugin that casts once at the selection and once at an object.
    #[derive(Debug, Default)]
    struct Caster {
        sent: Vec<bool>,
    }

    impl Plugin for Caster {
        fn enable(&mut self, host: &mut Host<'_>, _info: &StartInfo) -> Result<(), String> {
            let mut automation = HostAutomation::new(host);
            self.sent.push(automation.cast(7, None));
            self.sent.push(automation.cast(7, Some(42)));
            Ok(())
        }
    }

    #[test]
    fn a_cast_at_the_selection_is_target_zero_and_sent_means_true() {
        let mut fake = FakeHost::new("acdream.mosstank");
        fake.set_reply::<calls::MagicCast>(&CastResult::Sent);
        let mut plugin = Caster::default();
        fake.enable(&mut plugin).unwrap();

        assert_eq!(plugin.sent, [true, true]);
        let requests: Vec<CastRequest> = fake.requests::<calls::MagicCast>();
        let targets: Vec<u32> = requests.iter().map(|r| r.target_id).collect();
        assert_eq!(targets, [0, 42]);
    }

    /// A plugin that equips once and reads the placements it kept twice.
    #[derive(Debug, Default)]
    struct Wielder {
        kept: Vec<EquipmentObservation>,
        read: Vec<usize>,
    }

    impl Plugin for Wielder {
        fn enable(&mut self, host: &mut Host<'_>, _info: &StartInfo) -> Result<(), String> {
            let mut automation = HostAutomation::with_equipment_observations(host, &mut self.kept);
            automation.equip(9);
            let first = automation.take_equipment_observations().len();
            let second = automation.take_equipment_observations().len();
            self.read = vec![first, second];
            Ok(())
        }
    }

    #[test]
    fn an_equip_lets_the_client_choose_the_slot_and_kept_placements_are_read_once() {
        let mut fake = FakeHost::new("acdream.mosstank");
        let mut plugin = Wielder {
            kept: vec![EquipmentObservation {
                object_id: 9,
                equipped_location: 0x0010_0000,
                ..EquipmentObservation::default()
            }],
            read: Vec::new(),
        };
        fake.enable(&mut plugin).unwrap();

        let requests = fake.requests::<calls::EquipmentEquip>();
        assert_eq!(requests.len(), 1);
        assert_eq!((requests[0].object_id, requests[0].location), (9, 0));
        assert_eq!(plugin.read, [1, 0]);
        assert!(plugin.kept.is_empty());
    }
}
