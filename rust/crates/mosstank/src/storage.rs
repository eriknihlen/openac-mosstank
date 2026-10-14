//! The stores' storage over the client: a [`PluginStorage`] for one place
//! (the plugin's own data, or the shared VTank profile folder), and the
//! [`StoreHost`] a profile store is handed for the length of one plugin
//! call.

use ac_plugin_abi::storage::{StoragePlace, StorageRoot};
use mosstank_stores::storage::{PluginStorage, Root, StorageError, StoreHost, StoreLog};
use openac_plugin::Host;
use openac_plugin::StorageLock;

use crate::automation::{HostAutomation, StorageView};

/// The client's storage at one place.
#[derive(Debug)]
pub struct HostStorage<'h, 'a> {
    host: &'h mut Host<'a>,
    place: StoragePlace,
}

impl<'h, 'a> HostStorage<'h, 'a> {
    /// The storage at `place`.
    pub fn new(host: &'h mut Host<'a>, place: StoragePlace) -> Self {
        Self { host, place }
    }

    /// The plugin's own data (the C#'s `IPluginHost.Storage`).
    pub fn plugin_data(host: &'h mut Host<'a>) -> Self {
        Self::new(host, StoragePlace::global(StorageRoot::PluginData))
    }

    /// The shared VTank profile folder (`IPluginHost.VtankProfiles`).
    pub fn profiles(host: &'h mut Host<'a>) -> Self {
        Self::new(host, StoragePlace::global(StorageRoot::Profiles))
    }
}

impl PluginStorage for HostStorage<'_, '_> {
    /// The client describes a place it has storage for; one it has none for
    /// has no description.
    fn is_available(&mut self) -> bool {
        self.host.storage_describe(&self.place).is_some()
    }

    fn root_path(&mut self) -> Option<String> {
        self.host.storage_describe(&self.place)
    }

    fn read_text(&mut self, key: &str) -> Result<Option<String>, StorageError> {
        self.host.storage_read_text(&self.place, key)
    }

    fn list(&mut self, prefix: &str) -> Result<Vec<String>, StorageError> {
        self.host.storage_list(&self.place, prefix)
    }

    fn write_text(&mut self, key: &str, content: &str) -> Result<(), StorageError> {
        self.host.storage_write_text(&self.place, key, content)
    }

    fn ensure_directory(&mut self, prefix: &str) -> Result<bool, StorageError> {
        self.host.storage_ensure_folder(&self.place, prefix)
    }

    fn delete(&mut self, key: &str) -> Result<bool, StorageError> {
        self.host.storage_delete(&self.place, key)
    }

    fn lock(&mut self, key: &str) -> Result<u32, StorageError> {
        self.host.storage_lock(&self.place, key).map(|lock| lock.0)
    }

    fn unlock(&mut self, handle: u32) -> bool {
        self.host.storage_unlock(StorageLock(handle))
    }
}

impl StoreLog for HostStorage<'_, '_> {
    fn info(&mut self, message: &str) {
        self.host.info(message);
    }

    fn warn(&mut self, message: &str) {
        self.host.warn(message);
    }

    fn error(&mut self, message: &str) {
        self.host.error(message);
    }
}

/// What a profile store reaches during one plugin call: both storages, the
/// log and the world's name, all through the one client of the call.
pub type HostStores<'h, 'a> = HostAutomation<'h, 'a>;

impl StoreHost for HostAutomation<'_, '_> {
    fn storage(&mut self, root: Root) -> &mut dyn PluginStorage {
        self.storage_view = StorageView::Store(StoragePlace::global(match root {
            Root::Plugin => StorageRoot::PluginData,
            Root::Profiles => StorageRoot::Profiles,
        }));
        self
    }

    fn log(&mut self) -> &mut dyn StoreLog {
        self
    }

    /// The character's world, empty before one is known.
    fn world_name(&mut self) -> String {
        self.character_info()
            .map(|character| character.world)
            .unwrap_or_default()
    }
}

impl StoreLog for HostAutomation<'_, '_> {
    fn info(&mut self, message: &str) {
        self.service().info(message);
    }

    fn warn(&mut self, message: &str) {
        self.service().warn(message);
    }

    fn error(&mut self, message: &str) {
        self.service().error(message);
    }
}

impl HostAutomation<'_, '_> {
    /// The place the storage view reads, and whether it is always there.
    fn storage_place(&self) -> (StoragePlace, bool) {
        match &self.storage_view {
            StorageView::Meta => (StoragePlace::global(StorageRoot::PluginData), true),
            StorageView::Store(place) => (place.clone(), false),
        }
    }

    /// [`Self::storage_place`], for a change to it: a change to the profile
    /// folder is noted (its pickers list it again).
    fn note_write(&mut self, key: &str) -> (StoragePlace, bool) {
        let (place, always) = self.storage_place();
        if place.root == StorageRoot::Profiles {
            self.profiles_written.push(key.to_owned());
        }
        (place, always)
    }
}

/// The storage the last accessor handed out: the meta's (the plugin's own
/// data, always there) or a profile store's place (there when the client
/// describes it).
impl PluginStorage for HostAutomation<'_, '_> {
    fn is_available(&mut self) -> bool {
        let (place, always) = self.storage_place();
        always || self.service().storage_describe(&place).is_some()
    }

    fn root_path(&mut self) -> Option<String> {
        let (place, _) = self.storage_place();
        self.service().storage_describe(&place)
    }

    fn read_text(&mut self, key: &str) -> Result<Option<String>, StorageError> {
        let (place, _) = self.storage_place();
        let text = self.service().storage_read_text(&place, key);
        if let Ok(Some(text)) = &text {
            self.storage_bytes_read += text.len();
        }
        text
    }

    fn list(&mut self, prefix: &str) -> Result<Vec<String>, StorageError> {
        let (place, _) = self.storage_place();
        self.service().storage_list(&place, prefix)
    }

    fn write_text(&mut self, key: &str, content: &str) -> Result<(), StorageError> {
        let (place, _) = self.note_write(key);
        self.service().storage_write_text(&place, key, content)
    }

    fn ensure_directory(&mut self, prefix: &str) -> Result<bool, StorageError> {
        let (place, _) = self.note_write(prefix);
        self.service().storage_ensure_folder(&place, prefix)
    }

    fn delete(&mut self, key: &str) -> Result<bool, StorageError> {
        let (place, _) = self.note_write(key);
        self.service().storage_delete(&place, key)
    }

    fn lock(&mut self, key: &str) -> Result<u32, StorageError> {
        let (place, _) = self.storage_place();
        self.service().storage_lock(&place, key).map(|lock| lock.0)
    }

    fn unlock(&mut self, handle: u32) -> bool {
        self.service().storage_unlock(StorageLock(handle))
    }
}

#[cfg(test)]
mod tests {
    use ac_plugin_abi::calls;
    use ac_plugin_abi::storage::StorageKey;
    use openac_plugin::testing::FakeHost;
    use openac_plugin::{Host, Plugin, StartInfo};

    use super::*;

    /// A plugin that reads one key from each root through the stores' view.
    #[derive(Debug, Default)]
    struct Reader {
        read: Vec<Option<String>>,
    }

    impl Plugin for Reader {
        fn enable(&mut self, host: &mut Host<'_>, _info: &StartInfo) -> Result<(), String> {
            let mut stores = HostStores::new(host);
            self.read.push(
                stores
                    .storage(Root::Profiles)
                    .read_text("mosstank/profiles/a.usd")?,
            );
            self.read.push(
                stores
                    .storage(Root::Plugin)
                    .read_text("profiles/macro/b.json")?,
            );
            stores.log().warn("read both");
            Ok(())
        }
    }

    #[test]
    fn each_root_reads_from_its_own_place() {
        let mut fake = FakeHost::new("acdream.mosstank");
        fake.set_reply::<calls::StorageReadText>(&Ok(Some("text".to_owned())));
        let mut plugin = Reader::default();
        fake.enable(&mut plugin).unwrap();

        let requests: Vec<StorageKey> = fake.requests::<calls::StorageReadText>();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].place.root, StorageRoot::Profiles);
        assert_eq!(requests[0].key, "mosstank/profiles/a.usd");
        assert_eq!(requests[1].place.root, StorageRoot::PluginData);
        assert_eq!(requests[1].key, "profiles/macro/b.json");
        assert_eq!(plugin.read, vec![Some("text".to_owned()); 2]);
        assert!(fake.logs.iter().any(|(_, line)| line == "read both"));
    }
}
