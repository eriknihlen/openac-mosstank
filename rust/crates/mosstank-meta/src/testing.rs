//! A client kept in memory ([`FakeHost`]), for the engine's tests and the
//! plugin's: every record the client would answer is a field to arrange,
//! and every command it would carry out is recorded.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use ac_plugin_abi::character::{ActiveEnchantment, Attribute, Skill};
use ac_plugin_abi::combat::{CombatMode, CombatResult, CombatSnapshot, CombatStatus, CombatTarget};
use ac_plugin_abi::items::{
    EquipmentItem, EquipmentResult, EquipmentStatus, InventoryItem, ItemResult, ItemState,
    ItemStatus, LootContainer,
};
use ac_plugin_abi::magic::{CastResult, ComponentInfo, SpellInfo, SpellList};
use ac_plugin_abi::navigation::{MovementIntent, NavigationSnapshot, NavigationStatus};
use ac_plugin_abi::peers::{NetworkClient, PeerCast};
use ac_plugin_abi::social::{FellowMember, FellowshipState, LoginState, SelectionState};
use ac_plugin_abi::world::{ItemProperties, WorldObject, WorldTime};
use ac_plugin_abi::{CharacterInfo, ChatLine, LogLevel};
use mosstank_stores::storage::{MemoryStorage, PluginStorage, StorageError};

use crate::host::{MetaHost, PanelDescriptor, SpellExtras};

/// Storage in memory, shared by every handle on it, whose operations can be
/// told to fail the way a file held by another program does.
#[derive(Debug, Clone)]
pub struct SharedStorage {
    inner: Rc<RefCell<MemoryStorage>>,
    failure: Rc<RefCell<Option<(String, String)>>>,
    folder: bool,
}

impl Default for SharedStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedStorage {
    /// Empty storage whose keys match case exactly.
    #[must_use]
    pub fn new() -> Self {
        let mut inner = MemoryStorage::ordinal();
        inner.root = None;
        Self {
            inner: Rc::new(RefCell::new(inner)),
            failure: Rc::new(RefCell::new(None)),
            folder: false,
        }
    }

    /// Storage backed by a folder (so the global variables lock it).
    #[must_use]
    pub fn folder() -> Self {
        let mut storage = Self::new();
        storage.inner.borrow_mut().root = Some("C:\\storage".to_owned());
        storage.folder = true;
        storage
    }

    /// Storage that is not there.
    #[must_use]
    pub fn unavailable() -> Self {
        let storage = Self::new();
        storage.inner.borrow_mut().available = false;
        storage
    }

    /// Makes one kind of operation ("read", "write", "delete", "list") fail
    /// with `message`.
    pub fn fail(&self, operation: &str, message: &str) {
        *self.failure.borrow_mut() = Some((operation.to_owned(), message.to_owned()));
    }

    fn check(&self, operation: &str) -> Result<(), StorageError> {
        match &*self.failure.borrow() {
            Some((failing, message)) if failing == operation => Err(message.clone()),
            _ => Ok(()),
        }
    }

    /// The text under `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<String> {
        self.inner.borrow().get(key).map(str::to_owned)
    }

    /// Puts a file in place.
    pub fn insert(&self, key: &str, content: &str) {
        self.inner.borrow_mut().insert(key, content);
    }

    /// Every key.
    #[must_use]
    pub fn keys(&self) -> Vec<String> {
        self.inner.borrow().keys().map(str::to_owned).collect()
    }
}

impl PluginStorage for SharedStorage {
    fn is_available(&mut self) -> bool {
        self.inner.borrow_mut().is_available()
    }

    fn root_path(&mut self) -> Option<String> {
        self.inner.borrow_mut().root_path()
    }

    fn read_text(&mut self, key: &str) -> Result<Option<String>, StorageError> {
        self.check("read")?;
        self.inner.borrow_mut().read_text(key)
    }

    fn list(&mut self, prefix: &str) -> Result<Vec<String>, StorageError> {
        self.check("list")?;
        self.inner.borrow_mut().list(prefix)
    }

    fn write_text(&mut self, key: &str, content: &str) -> Result<(), StorageError> {
        self.check("write")?;
        self.inner.borrow_mut().write_text(key, content)
    }

    fn ensure_directory(&mut self, prefix: &str) -> Result<bool, StorageError> {
        self.inner.borrow_mut().ensure_directory(prefix)
    }

    fn delete(&mut self, key: &str) -> Result<bool, StorageError> {
        self.check("delete")?;
        self.inner.borrow_mut().delete(key)
    }

    fn lock(&mut self, key: &str) -> Result<u32, StorageError> {
        if self.folder {
            Ok(1 + u32::try_from(key.len()).unwrap_or(0))
        } else {
            self.inner.borrow_mut().lock(key)
        }
    }

    fn unlock(&mut self, handle: u32) -> bool {
        if self.folder {
            true
        } else {
            self.inner.borrow_mut().unlock(handle)
        }
    }
}

/// A window the fake host registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakePanel {
    /// Its handle.
    pub handle: u32,
    /// What it was registered with.
    pub descriptor: PanelDescriptor,
    /// Its markup.
    pub markup: String,
    /// It was taken down.
    pub disposed: bool,
}

/// The client's own windows, as the fake answers for them.
#[derive(Debug, Clone, Default)]
pub struct FakeUi {
    /// Views that exist (and are shown).
    pub views: Vec<String>,
    /// (view, control) pairs that exist.
    pub controls: Vec<(String, String)>,
    /// Whether a label is taken.
    pub label_accepted: bool,
    /// The last label set.
    pub label: String,
    /// The last visibility set.
    pub last_visible: Option<bool>,
    /// Every control exists and every view is shown.
    pub everything: bool,
}

/// A client in memory.
#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct FakeHost {
    /// Every log line.
    pub logs: Vec<(LogLevel, String)>,
    /// Every line shown in chat, with its text class.
    pub posted: Vec<(String, u32)>,
    /// Every line submitted.
    pub submitted: Vec<String>,
    /// What the chat entry holds.
    pub composed: String,
    /// The chat entry takes text.
    pub can_compose: bool,
    /// The chat transcript.
    pub chat: Vec<ChatLine>,
    /// The chat filters registered.
    pub filters: Vec<u32>,
    next_filter: u32,
    /// The character, `None` before it is in the world.
    pub character: Option<CharacterInfo>,
    /// Its skills.
    pub skills: Vec<Skill>,
    /// Its attributes.
    pub attributes: Vec<Attribute>,
    /// Its enchantments.
    pub enchantments: Vec<ActiveEnchantment>,
    /// Every object the client knows.
    pub objects: Vec<WorldObject>,
    /// How many times every object was listed.
    pub world_capture_count: usize,
    /// Appraisals, by object.
    pub properties: HashMap<u32, ItemProperties>,
    /// The open container.
    pub open_container: u32,
    /// The world clock.
    pub world_time: WorldTime,
    /// Where the character stands.
    pub navigation: NavigationSnapshot,
    /// Every movement intent set.
    pub intents: Vec<MovementIntent>,
    /// How many times the movement keys were let go.
    pub clear_movement_count: usize,
    /// The selection.
    pub selection: SelectionState,
    /// The item state.
    pub item_state: ItemState,
    /// What the character owns.
    pub owned_items: Vec<InventoryItem>,
    /// The last object used.
    pub used_object: u32,
    /// Every move: item, container, amount, slot, join.
    pub moves: Vec<(u32, u32, u32, i32, bool)>,
    /// The last salvage: tool and items.
    pub last_salvage: Option<(u32, Vec<u32>)>,
    /// The corpses nearby.
    pub corpses: Vec<LootContainer>,
    /// What can be worn.
    pub equipment: Vec<EquipmentItem>,
    /// The last item equipped.
    pub equipped_object: u32,
    /// An equip is in flight.
    pub equipment_busy: bool,
    /// The spell table.
    pub spells: HashMap<u32, SpellInfo>,
    /// The spell table's parts the client record lacks.
    pub spell_extras: HashMap<u32, SpellExtras>,
    /// Spells known besides those in the table.
    pub known_spell_ids: HashSet<u32>,
    /// Spells forgotten.
    pub unknown_spells: HashSet<u32>,
    /// The known self buffs.
    pub known_self_buffs: Vec<SpellInfo>,
    /// The known combat spells.
    pub known_combat_spells: Vec<SpellInfo>,
    /// The components.
    pub components: HashMap<u32, ComponentInfo>,
    /// The character has the components.
    pub has_components: bool,
    /// A cast is in flight.
    pub is_casting: bool,
    /// Casts are sent.
    pub cast_sent: bool,
    /// The last cast: spell and target.
    pub last_cast: (u32, u32),
    /// The combat mode.
    pub combat_mode: CombatMode,
    /// The last mode asked for.
    pub requested_mode: Option<CombatMode>,
    /// The hostile creatures nearby.
    pub hostile_targets: Vec<CombatTarget>,
    /// The fellowship.
    pub fellowship: FellowshipState,
    /// Its members.
    pub fellows: Vec<FellowMember>,
    /// The account's characters and the next login.
    pub login: LoginState,
    /// The other clients.
    pub peers: Vec<NetworkClient>,
    /// Their casts.
    pub peer_casts: Vec<PeerCast>,
    /// A window can be shown.
    pub has_ui: bool,
    /// The client's own windows.
    pub ui: FakeUi,
    /// The windows registered.
    pub panels: Vec<FakePanel>,
    /// The plugin's storage.
    pub storage: SharedStorage,
}

impl Default for FakeHost {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeHost {
    /// A character "Meta Tester" (id 1) on Coldeve, in the world, with full
    /// vitals of 100, nothing else.
    #[must_use]
    pub fn new() -> Self {
        Self {
            logs: Vec::new(),
            posted: Vec::new(),
            submitted: Vec::new(),
            composed: String::new(),
            can_compose: true,
            chat: Vec::new(),
            filters: Vec::new(),
            next_filter: 1,
            character: Some(CharacterInfo {
                id: 1,
                name: "Meta Tester".to_owned(),
                world: "Coldeve".to_owned(),
                account: "example-account".to_owned(),
                character_index: -1,
                health: (100, 100, 100),
                stamina: (100, 100, 100),
                mana: (100, 100, 100),
                vitae_multiplier: 1.0,
                ..CharacterInfo::default()
            }),
            skills: Vec::new(),
            attributes: Vec::new(),
            enchantments: Vec::new(),
            objects: Vec::new(),
            world_capture_count: 0,
            properties: HashMap::new(),
            open_container: 0,
            world_time: WorldTime::default(),
            navigation: NavigationSnapshot {
                is_available: true,
                local_object_id: 1,
                ..NavigationSnapshot::default()
            },
            intents: Vec::new(),
            clear_movement_count: 0,
            selection: SelectionState::default(),
            item_state: ItemState::default(),
            owned_items: Vec::new(),
            used_object: 0,
            moves: Vec::new(),
            last_salvage: None,
            corpses: Vec::new(),
            equipment: Vec::new(),
            equipped_object: 0,
            equipment_busy: false,
            spells: HashMap::new(),
            spell_extras: HashMap::new(),
            known_spell_ids: HashSet::new(),
            unknown_spells: HashSet::new(),
            known_self_buffs: Vec::new(),
            known_combat_spells: Vec::new(),
            components: HashMap::new(),
            has_components: true,
            is_casting: false,
            cast_sent: true,
            last_cast: (0, 0),
            combat_mode: CombatMode::Peace,
            requested_mode: None,
            hostile_targets: Vec::new(),
            fellowship: FellowshipState::default(),
            fellows: Vec::new(),
            login: LoginState::default(),
            peers: Vec::new(),
            peer_casts: Vec::new(),
            has_ui: false,
            ui: FakeUi::default(),
            panels: Vec::new(),
            storage: SharedStorage::unavailable(),
        }
    }

    /// The character, to change.
    ///
    /// # Panics
    /// The character is not in the world.
    pub fn character_mut(&mut self) -> &mut CharacterInfo {
        self.character
            .as_mut()
            .expect("the character is in the world")
    }

    /// The lines logged at `level`.
    #[must_use]
    pub fn logged(&self, level: LogLevel) -> Vec<String> {
        self.logs
            .iter()
            .filter(|(at, _)| *at == level)
            .map(|(_, line)| line.clone())
            .collect()
    }

    /// The text of every line shown in chat.
    #[must_use]
    pub fn posted_text(&self) -> Vec<String> {
        self.posted.iter().map(|(text, _)| text.clone()).collect()
    }

    /// Adds a chat line with the next sequence number.
    pub fn add_chat(&mut self, mut line: ChatLine) {
        line.sequence = self
            .chat
            .iter()
            .map(|line| line.sequence)
            .max()
            .unwrap_or(0)
            + 1;
        self.chat.push(line);
    }
}

fn find_skill(skills: &[Skill], id: u32) -> Option<Skill> {
    skills.iter().find(|skill| skill.skill_id == id).cloned()
}

impl MetaHost for FakeHost {
    fn log(&mut self, level: LogLevel, message: &str) {
        self.logs.push((level, message.to_owned()));
    }

    fn chat_post(&mut self, text: &str, log_text_type: u32) {
        self.posted.push((text.to_owned(), log_text_type));
    }

    fn chat_submit(&mut self, line: &str) -> bool {
        self.submitted.push(line.to_owned());
        true
    }

    fn chat_compose(&mut self, text: &str) -> bool {
        if !self.can_compose {
            return false;
        }
        text.clone_into(&mut self.composed);
        true
    }

    fn chat_captured(&mut self, after_sequence: u64) -> Vec<ChatLine> {
        self.chat
            .iter()
            .filter(|line| line.sequence > after_sequence)
            .cloned()
            .collect()
    }

    fn chat_add_filter(&mut self) -> u32 {
        let handle = self.next_filter;
        self.next_filter += 1;
        self.filters.push(handle);
        handle
    }

    fn chat_remove_filter(&mut self, handle: u32) -> bool {
        let before = self.filters.len();
        self.filters.retain(|filter| *filter != handle);
        before != self.filters.len()
    }

    fn character(&mut self) -> Option<CharacterInfo> {
        self.character.clone()
    }

    fn skill(&mut self, skill_id: u32) -> Option<Skill> {
        find_skill(&self.skills, skill_id)
    }

    fn attributes(&mut self) -> Vec<Attribute> {
        self.attributes.clone()
    }

    fn enchantments(&mut self) -> Vec<ActiveEnchantment> {
        self.enchantments.clone()
    }

    fn objects(&mut self) -> Vec<WorldObject> {
        self.world_capture_count += 1;
        self.objects.clone()
    }

    fn object(&mut self, object_id: u32) -> Option<WorldObject> {
        self.objects
            .iter()
            .find(|object| object.object_id == object_id)
            .cloned()
    }

    fn properties(&mut self, object_id: u32) -> Option<ItemProperties> {
        self.properties.get(&object_id).cloned()
    }

    fn int_property(&mut self, object_id: u32, property: u32) -> Option<i32> {
        self.properties
            .get(&object_id)
            .and_then(|properties| properties.int(property))
    }

    fn identify(&mut self, _object_id: u32) -> ItemResult {
        ItemResult::of(ItemStatus::Started)
    }

    fn open_container(&mut self) -> u32 {
        self.open_container
    }

    fn world_time(&mut self) -> WorldTime {
        self.world_time.clone()
    }

    fn navigation(&mut self) -> NavigationSnapshot {
        self.navigation.clone()
    }

    fn set_movement_intent(&mut self, intent: MovementIntent) -> NavigationStatus {
        self.intents.push(intent);
        NavigationStatus::Accepted
    }

    fn clear_movement_intent(&mut self) -> NavigationStatus {
        self.clear_movement_count += 1;
        NavigationStatus::Accepted
    }

    fn selection(&mut self) -> SelectionState {
        self.selection.clone()
    }

    fn select(&mut self, object_id: u32) -> bool {
        self.selection.previous = self.selection.selected;
        self.selection.selected = Some(object_id);
        true
    }

    fn item_state(&mut self) -> ItemState {
        self.item_state.clone()
    }

    fn owned_items(&mut self) -> Vec<InventoryItem> {
        self.owned_items.clone()
    }

    fn use_item(&mut self, object_id: u32) -> ItemResult {
        self.used_object = object_id;
        ItemResult::of(ItemStatus::Started)
    }

    fn apply_item(&mut self, _object_id: u32, _target_id: u32) -> ItemResult {
        ItemResult::of(ItemStatus::Started)
    }

    fn give_item(&mut self, _object_id: u32, _target_id: u32, _amount: u32) -> ItemResult {
        ItemResult::of(ItemStatus::Started)
    }

    fn drop_item(&mut self, _object_id: u32, _amount: u32) -> ItemResult {
        ItemResult::of(ItemStatus::Started)
    }

    fn move_item(
        &mut self,
        object_id: u32,
        container_id: u32,
        amount: u32,
        placement: i32,
        join_stack: bool,
    ) -> ItemResult {
        self.moves
            .push((object_id, container_id, amount, placement, join_stack));
        ItemResult::of(ItemStatus::Started)
    }

    fn salvage(&mut self, tool_id: u32, item_ids: Vec<u32>) -> ItemResult {
        self.last_salvage = Some((tool_id, item_ids));
        ItemResult::of(ItemStatus::Started)
    }

    fn corpses(&mut self, maximum_metres: f32) -> Vec<LootContainer> {
        self.corpses
            .iter()
            .filter(|corpse| corpse.distance <= maximum_metres)
            .cloned()
            .collect()
    }

    fn owned_equipment(&mut self) -> Vec<EquipmentItem> {
        self.equipment.clone()
    }

    fn equip(&mut self, object_id: u32, _location: u32) -> EquipmentResult {
        self.equipped_object = object_id;
        EquipmentResult {
            status: EquipmentStatus::Started,
            notice: None,
        }
    }

    fn equipment_busy(&mut self) -> bool {
        self.equipment_busy
    }

    fn spell(&mut self, spell_id: u32) -> Option<SpellInfo> {
        self.spells.get(&spell_id).cloned()
    }

    fn known_spells(&mut self, list: SpellList) -> Vec<SpellInfo> {
        match list {
            SpellList::SelfBuffs => self.known_self_buffs.clone(),
            SpellList::Combat => self.known_combat_spells.clone(),
            SpellList::Attacks => Vec::new(),
        }
    }

    fn is_spell_known(&mut self, spell_id: u32) -> bool {
        !self.unknown_spells.contains(&spell_id)
            && (self.known_spell_ids.contains(&spell_id) || self.spells.contains_key(&spell_id))
    }

    fn spell_component(&mut self, component_id: u32) -> Option<ComponentInfo> {
        self.components.get(&component_id).cloned()
    }

    fn cooldown_remaining(&mut self, _cooldown_id: u32) -> f64 {
        0.0
    }

    fn is_casting(&mut self) -> bool {
        self.is_casting
    }

    fn has_components(&mut self, _spell_id: u32) -> bool {
        self.has_components
    }

    fn cast(&mut self, spell_id: u32, target_id: u32) -> CastResult {
        self.last_cast = (spell_id, target_id);
        if self.cast_sent {
            CastResult::Sent
        } else {
            CastResult::Unavailable
        }
    }

    fn spell_extras(&mut self, spell_id: u32) -> SpellExtras {
        self.spell_extras
            .get(&spell_id)
            .copied()
            .unwrap_or_default()
    }

    fn combat(&mut self) -> CombatSnapshot {
        CombatSnapshot {
            mode: self.combat_mode,
            ..CombatSnapshot::default()
        }
    }

    fn hostile_targets(&mut self, maximum_metres: f32) -> Vec<CombatTarget> {
        self.hostile_targets
            .iter()
            .filter(|target| target.distance <= maximum_metres)
            .cloned()
            .collect()
    }

    fn enter_combat_mode(&mut self, mode: CombatMode) -> CombatResult {
        self.requested_mode = Some(mode);
        CombatResult {
            status: CombatStatus::ModeChangeSent,
            ..CombatResult::default()
        }
    }

    fn fellowship(&mut self) -> FellowshipState {
        self.fellowship.clone()
    }

    fn fellowship_roster(&mut self) -> Vec<FellowMember> {
        self.fellows.clone()
    }

    fn login(&mut self) -> LoginState {
        self.login.clone()
    }

    fn login_set_next(&mut self, character_id: u32) -> bool {
        if !self
            .login
            .roster
            .iter()
            .any(|character| character.object_id == character_id && !character.is_pending_delete)
        {
            return false;
        }
        self.login.next_login_id = character_id;
        true
    }

    fn login_clear_next(&mut self) -> bool {
        self.login.next_login_id = 0;
        true
    }

    fn peer_clients(&mut self) -> Vec<NetworkClient> {
        self.peers.clone()
    }

    fn peer_casts(&mut self, after_sequence: i64) -> Vec<PeerCast> {
        self.peer_casts
            .iter()
            .filter(|cast| cast.sequence > after_sequence)
            .cloned()
            .collect()
    }

    fn has_ui(&mut self) -> bool {
        self.has_ui
    }

    fn register_panel(&mut self, descriptor: &PanelDescriptor, markup: &str) -> Option<u32> {
        let handle = u32::try_from(self.panels.len()).unwrap_or(u32::MAX) + 1;
        self.panels.push(FakePanel {
            handle,
            descriptor: descriptor.clone(),
            markup: markup.to_owned(),
            disposed: false,
        });
        Some(handle)
    }

    fn unregister_panel(&mut self, handle: u32) {
        if let Some(panel) = self.panels.iter_mut().find(|panel| panel.handle == handle) {
            panel.disposed = true;
        }
    }

    fn ui_view_exists(&mut self, view: &str) -> bool {
        self.ui.everything || self.ui.views.iter().any(|known| known == view)
    }

    fn ui_view_visible(&mut self, view: &str) -> bool {
        self.ui_view_exists(view)
    }

    fn ui_control_exists(&mut self, view: &str, control: &str) -> bool {
        self.ui.everything
            || self
                .ui
                .controls
                .iter()
                .any(|(known_view, known)| known_view == view && known == control)
    }

    fn ui_set_control_label(&mut self, view: &str, control: &str, label: &str) -> bool {
        if !self.ui_control_exists(view, control) || !self.ui.label_accepted {
            return false;
        }
        label.clone_into(&mut self.ui.label);
        true
    }

    fn ui_set_control_visible(&mut self, view: &str, control: &str, visible: bool) -> bool {
        if !self.ui_control_exists(view, control) {
            return false;
        }
        self.ui.last_visible = Some(visible);
        true
    }

    fn storage(&mut self) -> &mut dyn PluginStorage {
        &mut self.storage
    }
}
