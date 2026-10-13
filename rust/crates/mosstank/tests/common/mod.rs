//! The plugin's tests' client: the SDK's fake host, with the two storages
//! the stores read kept in memory (the plugin's own data and the shared
//! VTank profile folder), so a profile written in one call is read in the
//! next.
#![allow(dead_code)]

pub mod automation;

use ac_plugin_abi::storage::{StorageKey, StoragePlace, StorageRoot, StorageWrite};
use ac_plugin_abi::{
    Call, CharacterInfo, CommandCall, Event, Import, LogLevel, StartInfo, Wire, calls,
};
use mosstank::MossTank;
use mosstank_stores::storage::{MemoryStorage, PluginStorage};
use openac_plugin::testing::FakeHost;
use openac_plugin::{Host, Plugin, Transport};

/// Where the fake client says the profile folder is.
pub const PROFILES_ROOT: &str = r"C:\Users\test\Documents\Decal Plugins\VirindiTank";

/// The client in memory.
#[derive(Debug)]
pub struct World {
    /// Everything but storage.
    pub fake: FakeHost,
    /// The plugin's own data.
    pub plugin_data: MemoryStorage,
    /// The shared profile folder.
    pub profiles: MemoryStorage,
    /// The chat lines the client has captured, when a test keeps them:
    /// each read answers the lines after the sequence it asks from.
    pub chat_log: Option<Vec<ac_plugin_abi::types::ChatLine>>,
    /// The client refuses to open any window (`ShowPanel` answers false).
    pub refuse_window_show: bool,
    /// The C# panel tests' `FakeAutomation`, when a test uses it: it
    /// answers the character, spell, cast, item and selection calls.
    pub automation: Option<automation::FakeAutomation>,
    /// The C# panel tests' `FrameLootSurface`, when a test uses it.
    pub loot: Option<FrameLootSurface>,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

/// A character in the world.
#[must_use]
pub fn character(name: &str) -> CharacterInfo {
    CharacterInfo {
        id: 0x5000_0001,
        name: name.to_owned(),
        world: "Coldeve".to_owned(),
        health: (100, 100, 100),
        stamina: (100, 100, 100),
        mana: (100, 100, 100),
        ..CharacterInfo::default()
    }
}

impl World {
    /// No character, both storages empty.
    #[must_use]
    pub fn new() -> Self {
        Self {
            fake: FakeHost::new("acdream.mosstank"),
            plugin_data: MemoryStorage::new(),
            profiles: MemoryStorage::new(),
            chat_log: None,
            refuse_window_show: false,
            automation: None,
            loot: None,
        }
    }

    /// `name` in the world.
    #[must_use]
    pub fn with_character(name: &str) -> Self {
        let mut world = Self::new();
        world.fake.character = Some(character(name));
        world
    }

    /// From now on, client function `C` answers `reply`.
    pub fn set_reply<C: Call>(&mut self, reply: &C::Reply) {
        self.fake.set_reply::<C>(reply);
    }

    /// Runs `f` with a host over this client.
    pub fn with_host<R>(&mut self, f: impl FnOnce(&mut Host<'_>) -> R) -> R {
        let mut host = Host::with_transport(self);
        f(&mut host)
    }

    /// Enables a plugin.
    ///
    /// # Panics
    /// The plugin refused.
    pub fn enable(&mut self) -> MossTank {
        let mut plugin = MossTank::default();
        let info = StartInfo {
            plugin_id: "acdream.mosstank".to_owned(),
            api_version: ac_plugin_abi::API_CURRENT.to_string(),
            client_version: "0.1.0".to_owned(),
        };
        self.with_host(|host| plugin.enable(host, &info)).unwrap();
        plugin
    }

    /// One tick.
    pub fn tick(&mut self, plugin: &mut MossTank) {
        self.with_host(|host| plugin.tick(host));
    }

    /// `count` ticks.
    pub fn ticks(&mut self, plugin: &mut MossTank, count: usize) {
        for _ in 0..count {
            self.tick(plugin);
        }
    }

    /// An event.
    pub fn send(&mut self, plugin: &mut MossTank, event: Event) {
        self.with_host(|host| plugin.on_event(host, event));
    }

    /// The player typed `/<verb> <arguments>`.
    pub fn command(&mut self, plugin: &mut MossTank, verb: &str, arguments: &str) {
        let call = CommandCall {
            verb: verb.to_owned(),
            arguments: arguments.to_owned(),
            raw: format!("/{verb} {arguments}"),
        };
        self.send(plugin, Event::Command(call));
    }

    /// `/vt <arguments>`.
    pub fn vt(&mut self, plugin: &mut MossTank, arguments: &str) {
        self.command(plugin, "vt", arguments);
    }

    /// Disables the plugin.
    pub fn disable(&mut self, plugin: &mut MossTank) {
        self.with_host(|host| plugin.disable(host));
    }

    /// Every chat line posted, in order.
    #[must_use]
    pub fn chat(&self) -> Vec<String> {
        self.fake
            .posted
            .iter()
            .map(|post| post.text.clone())
            .collect()
    }

    /// Forgets the chat lines posted so far.
    pub fn clear_chat(&mut self) {
        self.fake.posted.clear();
    }

    /// Every log line at a level.
    #[must_use]
    pub fn logs(&self, level: LogLevel) -> Vec<String> {
        self.fake
            .logs
            .iter()
            .filter(|(at, _)| *at == level)
            .map(|(_, line)| line.clone())
            .collect()
    }

    /// Every log line.
    #[must_use]
    pub fn all_logs(&self) -> Vec<String> {
        self.fake
            .logs
            .iter()
            .map(|(_, line)| line.clone())
            .collect()
    }

    /// The requests the plugin made to client function `C`.
    #[must_use]
    pub fn requests<C: Call>(&self) -> Vec<C::Request> {
        self.fake.requests::<C>()
    }

    fn storage(&mut self, place: &StoragePlace) -> Option<&mut MemoryStorage> {
        match place.root {
            StorageRoot::PluginData => Some(&mut self.plugin_data),
            StorageRoot::Profiles => Some(&mut self.profiles),
            _ => None,
        }
    }

    fn answer<C: Call>(
        &mut self,
        bytes: &[u8],
        f: impl FnOnce(&mut Self, C::Request) -> C::Reply,
    ) -> Vec<u8> {
        self.fake.calls.push(C::IMPORT);
        self.fake.requests.push((C::IMPORT, bytes.to_vec()));
        let request = C::Request::from_bytes(bytes).expect("a request the SDK encoded");
        f(self, request).to_bytes()
    }
}

fn unavailable<T>() -> Result<T, String> {
    Err("storage is unavailable".to_owned())
}

impl Transport for World {
    fn call(&mut self, import: Import, bytes: &[u8]) -> Vec<u8> {
        if let Some(reply) = self.automation_call(import, bytes) {
            return reply;
        }
        match import {
            Import::StorageReadText => {
                self.answer::<calls::StorageReadText>(bytes, |world, key: StorageKey| {
                    match world.storage(&key.place) {
                        Some(storage) => storage.read_text(&key.key),
                        None => Ok(None),
                    }
                })
            }
            Import::StorageRead => {
                self.answer::<calls::StorageRead>(bytes, |world, key: StorageKey| {
                    match world.storage(&key.place) {
                        Some(storage) => storage
                            .read_text(&key.key)
                            .map(|text| text.map(String::into_bytes)),
                        None => Ok(None),
                    }
                })
            }
            Import::StorageWrite => {
                self.answer::<calls::StorageWrite>(bytes, |world, write: StorageWrite| match world
                    .storage(&write.place)
                {
                    Some(storage) => {
                        storage.write_text(&write.key, &String::from_utf8_lossy(&write.bytes))
                    }
                    None => unavailable(),
                })
            }
            Import::StorageList => {
                self.answer::<calls::StorageList>(bytes, |world, key: StorageKey| {
                    match world.storage(&key.place) {
                        Some(storage) => storage.list(&key.key),
                        None => Ok(Vec::new()),
                    }
                })
            }
            Import::StorageDelete => {
                self.answer::<calls::StorageDelete>(bytes, |world, key: StorageKey| {
                    match world.storage(&key.place) {
                        Some(storage) => storage.delete(&key.key),
                        None => Ok(false),
                    }
                })
            }
            Import::StorageEnsureFolder => {
                self.answer::<calls::StorageEnsureFolder>(bytes, |world, key: StorageKey| {
                    match world.storage(&key.place) {
                        Some(storage) => storage.ensure_directory(&key.key),
                        None => Ok(false),
                    }
                })
            }
            Import::StorageDescribe => {
                self.answer::<calls::StorageDescribe>(bytes, |_, place: StoragePlace| {
                    match place.root {
                        StorageRoot::Profiles => Some(PROFILES_ROOT.to_owned()),
                        StorageRoot::PluginData => {
                            Some(r"C:\openac\plugins\acdream.mosstank\data".to_owned())
                        }
                        _ => None,
                    }
                })
            }
            Import::StorageLock => {
                self.answer::<calls::StorageLock>(bytes, |_, _key: StorageKey| Ok(1))
            }
            Import::StorageUnlock => {
                self.answer::<calls::StorageUnlock>(bytes, |_, _handle: u32| true)
            }
            Import::LootState if self.loot.is_some() => {
                self.answer::<calls::LootState>(bytes, |world, ()| {
                    let loot = world.loot.as_ref().unwrap();
                    ac_plugin_abi::items::LootState {
                        is_available: true,
                        is_busy: false,
                        requested_container_id: loot.opened,
                        current_container_id: loot.current,
                        current_contents_ready: loot.contents_ready,
                        ..ac_plugin_abi::items::LootState::default()
                    }
                })
            }
            Import::LootCorpses if self.loot.is_some() => self
                .answer::<calls::LootCorpses>(bytes, |world, _maximum: f32| {
                    world.loot.as_ref().unwrap().corpses.clone()
                }),
            Import::LootOpen if self.loot.is_some() => {
                self.answer::<calls::LootOpen>(bytes, |world, container: u32| {
                    world.loot.as_mut().unwrap().opened = container;
                    ac_plugin_abi::items::ItemResult::of(ac_plugin_abi::items::ItemStatus::Started)
                })
            }
            Import::UiPanelShow if self.refuse_window_show => {
                self.answer::<calls::UiPanelShow>(bytes, |_, _view| false)
            }
            Import::ChatCaptured if self.chat_log.is_some() => {
                self.answer::<calls::ChatCaptured>(bytes, |world, after: u64| {
                    world
                        .chat_log
                        .iter()
                        .flatten()
                        .filter(|line| line.sequence > after)
                        .cloned()
                        .collect()
                })
            }
            _ => self.fake.call(import, bytes),
        }
    }
}

/// One of the session proof's fixture files, read in place from the C#
/// test folder.
#[must_use]
pub fn proof_fixture(relative: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/AcDream.Plugins.MossTank.Tests/Fixtures/vt-proof")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Writes the session proof's settings, loot and route into the profile
/// folder (`WriteSessionProofFixture`).
pub fn write_session_proof_fixture(world: &mut World) {
    world.profiles.insert(
        "mosstank/profiles/vt-proof-settings.usd",
        &proof_fixture("vt-proof-settings.usd"),
    );
    world.profiles.insert(
        "mosstank/loot/vt-proof-loot.utl",
        &proof_fixture("vt-proof-loot.utl"),
    );
    world.profiles.insert(
        "mosstank/navs/vt-proof-route.af",
        &proof_fixture("navs/vt-proof-route.af"),
    );
}

impl World {
    /// Runs `f` with the plugin's macro shell and a client over this world.
    pub fn with_tank<R>(
        &mut self,
        plugin: &mut MossTank,
        f: impl FnOnce(
            &mut mosstank::tank::Tank,
            &mut mosstank::automation::HostAutomation<'_, '_>,
        ) -> R,
    ) -> R {
        self.with_host(|host| plugin.with_tank(host, f).expect("an enabled plugin"))
    }

    /// The session's settings.
    pub fn set_session(&mut self, settings: &[(&str, &str)]) {
        self.fake.settings = settings
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
    }

    /// The error lines logged.
    #[must_use]
    pub fn errors(&self) -> Vec<String> {
        self.logs(LogLevel::Error)
    }

    /// The information lines logged.
    #[must_use]
    pub fn infos(&self) -> Vec<String> {
        self.logs(LogLevel::Info)
    }
}

/// A fixture of the C# tests, read in place.
#[must_use]
pub fn csharp_fixture(relative: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/AcDream.Plugins.MossTank.Tests/Fixtures")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

impl World {
    /// A line arrives in the client's chat (`FakeAutomation.PostChat`): kept
    /// for the captured-chat reads, and sent as the event.
    pub fn post_chat(&mut self, plugin: &mut MossTank, text: &str) {
        let log = self.chat_log.get_or_insert_with(Vec::new);
        let sequence = log.last().map_or(1, |line| line.sequence + 1);
        let line = ac_plugin_abi::types::ChatLine {
            sequence,
            text: text.to_owned(),
            ..ac_plugin_abi::types::ChatLine::default()
        };
        log.push(line.clone());
        self.send(plugin, Event::ChatReceived(line));
    }
}

/// The windows as the tests drive them: a binding's value as the shell
/// works it out, and a handler fired with its argument.
impl World {
    /// The value of window binding `name`.
    ///
    /// # Panics
    /// No window binds `name`.
    pub fn ui(&mut self, plugin: &mut MossTank, name: &str) -> ac_plugin_abi::ui::UiValue {
        self.with_tank(plugin, |tank, client| tank.ui_value(client, name))
            .unwrap_or_else(|| panic!("no value for {name}"))
    }

    /// A text binding.
    pub fn ui_text(&mut self, plugin: &mut MossTank, name: &str) -> String {
        match self.ui(plugin, name) {
            ac_plugin_abi::ui::UiValue::Text(text) => text,
            other => panic!("{name} is {other:?}"),
        }
    }

    /// A switch binding.
    pub fn ui_bool(&mut self, plugin: &mut MossTank, name: &str) -> bool {
        match self.ui(plugin, name) {
            ac_plugin_abi::ui::UiValue::Bool(value) => value,
            other => panic!("{name} is {other:?}"),
        }
    }

    /// A number binding.
    pub fn ui_int(&mut self, plugin: &mut MossTank, name: &str) -> i32 {
        match self.ui(plugin, name) {
            ac_plugin_abi::ui::UiValue::Int(value) => value,
            other => panic!("{name} is {other:?}"),
        }
    }

    /// A list of lines.
    pub fn ui_list(&mut self, plugin: &mut MossTank, name: &str) -> Vec<String> {
        match self.ui(plugin, name) {
            ac_plugin_abi::ui::UiValue::TextList(lines) => lines,
            other => panic!("{name} is {other:?}"),
        }
    }

    /// A list of switches.
    pub fn ui_bools(&mut self, plugin: &mut MossTank, name: &str) -> Vec<bool> {
        match self.ui(plugin, name) {
            ac_plugin_abi::ui::UiValue::BoolList(values) => values,
            other => panic!("{name} is {other:?}"),
        }
    }

    /// Fires handler `name` with `value`.
    ///
    /// # Panics
    /// No window has the handler.
    pub fn act_with(
        &mut self,
        plugin: &mut MossTank,
        name: &str,
        value: ac_plugin_abi::ui::ActionValue,
    ) {
        let answered = self.with_tank(plugin, |tank, client| tank.ui_act(client, name, value));
        assert!(answered, "no handler {name}");
    }

    /// A click.
    pub fn act(&mut self, plugin: &mut MossTank, name: &str) {
        self.act_with(plugin, name, ac_plugin_abi::ui::ActionValue::None);
    }

    /// A handler given text.
    pub fn act_text(&mut self, plugin: &mut MossTank, name: &str, text: &str) {
        self.act_with(
            plugin,
            name,
            ac_plugin_abi::ui::ActionValue::Text(text.to_owned()),
        );
    }

    /// A handler given a row.
    pub fn act_int(&mut self, plugin: &mut MossTank, name: &str, row: i32) {
        self.act_with(plugin, name, ac_plugin_abi::ui::ActionValue::Int(row));
    }

    /// A handler given a slider value.
    pub fn act_float(&mut self, plugin: &mut MossTank, name: &str, value: f32) {
        self.act_with(plugin, name, ac_plugin_abi::ui::ActionValue::Float(value));
    }

    /// The row of `name` in list binding `list` (-1 when absent).
    pub fn ui_index(&mut self, plugin: &mut MossTank, list: &str, name: &str) -> i32 {
        self.ui_list(plugin, list)
            .iter()
            .position(|line| line == name)
            .map_or(-1, |index| i32::try_from(index).unwrap())
    }

    /// Meta option `name` as text (`GetMetaOption(...).ToDisplayString()`).
    pub fn option_text(&mut self, plugin: &mut MossTank, name: &str) -> String {
        self.with_tank(plugin, |tank, _| tank.option(name).to_display_string())
    }
}

/// An owned item as the panel tests make one (`MossTankPanelTests.Item`):
/// in the character's pack, its slot and class from its item type.
#[must_use]
pub fn item(object_id: u32, name: &str, item_type: u32) -> ac_plugin_abi::items::InventoryItem {
    use ac_plugin_abi::world::ObjectClass;
    let (valid_locations, object_class) = if item_type & 0x8000 != 0 {
        (0x0100_0000, ObjectClass::WandStaffOrb)
    } else if item_type & 0x100 != 0 {
        (0x0040_0000, ObjectClass::MissileWeapon)
    } else if item_type & 0x1 != 0 {
        (0x0010_0000, ObjectClass::MeleeWeapon)
    } else if item_type & 0x20 != 0 {
        (0, ObjectClass::Food)
    } else if item_type & 0x800 != 0 {
        (0, ObjectClass::Gem)
    } else if item_type & 0x1000 != 0 {
        (0, ObjectClass::SpellComponent)
    } else if item_type & 0x80 != 0 {
        (0, ObjectClass::Misc)
    } else {
        (0, ObjectClass::Unknown)
    };
    ac_plugin_abi::items::InventoryItem {
        object_id,
        name: name.to_owned(),
        item_type,
        container_id: 1,
        valid_locations,
        stack_size: 1,
        object_class,
        ..ac_plugin_abi::items::InventoryItem::default()
    }
}

impl World {
    /// The character owns `items`, each appraised (the object table and the
    /// properties answer for any of them).
    pub fn own_items(&mut self, items: Vec<ac_plugin_abi::items::InventoryItem>) {
        self.set_reply::<calls::ItemsOwned>(&items);
        self.set_reply::<calls::WorldObject>(&Some(ac_plugin_abi::world::WorldObject {
            is_owned: true,
            has_appraisal_data: true,
            last_id_time: 1,
            ..ac_plugin_abi::world::WorldObject::default()
        }));
        self.set_reply::<calls::ItemsProperties>(&Some(
            ac_plugin_abi::world::ItemProperties::default(),
        ));
    }

    /// The client has `object_id` selected (0: nothing).
    pub fn select_object(&mut self, object_id: u32) {
        if let Some(automation) = self.automation.as_mut() {
            automation.previous_selected = automation.selected;
            automation.selected = (object_id != 0).then_some(object_id);
            return;
        }
        self.set_reply::<calls::SelectionState>(&ac_plugin_abi::social::SelectionState {
            selected: (object_id != 0).then_some(object_id),
            previous: None,
        });
    }

    /// A list of numbers (icons, colours).
    pub fn ui_uints(&mut self, plugin: &mut MossTank, name: &str) -> Vec<u32> {
        match self.ui(plugin, name) {
            ac_plugin_abi::ui::UiValue::UintList(values) => values,
            other => panic!("{name} is {other:?}"),
        }
    }
}

impl World {
    /// `name` in the world as the C# panel tests' fake has it: no world
    /// name until a test gives one (`MossTankPanelTests.FakeAutomation`).
    #[must_use]
    pub fn panel_character(name: &str) -> Self {
        let mut world = Self::with_character(name);
        if let Some(character) = world.fake.character.as_mut() {
            character.world = String::new();
        }
        world
    }
}

impl World {
    /// A slider's value.
    pub fn ui_float(&mut self, plugin: &mut MossTank, name: &str) -> f32 {
        match self.ui(plugin, name) {
            ac_plugin_abi::ui::UiValue::Float(value) => value,
            other => panic!("{name} is {other:?}"),
        }
    }

    /// The character knows `spells` (every spell list answers them).
    pub fn know_spells(&mut self, spells: Vec<ac_plugin_abi::magic::SpellInfo>) {
        self.set_reply::<calls::SpellsKnown>(&spells);
    }
}

/// A self buff as the panel tests make one (`MossTankPanelTests.Spell`).
#[must_use]
pub fn spell(spell_id: u32, family: u32, description: &str) -> ac_plugin_abi::magic::SpellInfo {
    ac_plugin_abi::magic::SpellInfo {
        spell_id,
        name: format!("Spell {spell_id}"),
        family,
        tier: 1,
        difficulty: 10,
        mana_cost: 5,
        duration_seconds: 60.0,
        school: 1,
        description: description.to_owned(),
        is_self_targeted: true,
        is_beneficial: true,
        ..ac_plugin_abi::magic::SpellInfo::default()
    }
}

/// The C# panel tests' `FrameLootSurface`: a loot surface whose open
/// request, open container and contents a test moves by hand.
#[derive(Debug, Clone)]
pub struct FrameLootSurface {
    /// The container last asked open (`Opened`, `RequestedContainerId`).
    pub opened: u32,
    /// The container open now (`Current`).
    pub current: u32,
    /// Whether its contents have arrived (`ContentsReady`).
    pub contents_ready: bool,
    /// The corpses about (`Corpses`).
    pub corpses: Vec<ac_plugin_abi::items::LootContainer>,
}

impl FrameLootSurface {
    /// `FrameLootSurface.CorpseId`.
    pub const CORPSE_ID: u32 = 0x7000_0D01;
}

impl Default for FrameLootSurface {
    fn default() -> Self {
        Self {
            opened: 0,
            current: 0,
            contents_ready: true,
            corpses: Vec::new(),
        }
    }
}

impl World {
    /// The frame loot surface.
    ///
    /// # Panics
    /// The world has none.
    pub fn loot(&mut self) -> &mut FrameLootSurface {
        self.loot.as_mut().expect("a world with a FrameLootSurface")
    }
}
