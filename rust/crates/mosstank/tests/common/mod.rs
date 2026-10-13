//! The plugin's tests' client: the SDK's fake host, with the two storages
//! the stores read kept in memory (the plugin's own data and the shared
//! VTank profile folder), so a profile written in one call is read in the
//! next.
#![allow(dead_code)]

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
            _ => self.fake.call(import, bytes),
        }
    }
}
