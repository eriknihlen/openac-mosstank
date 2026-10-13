//! A string-keyed map that enumerates in the order .NET's
//! `Dictionary<string, T>` does: entries in their slots, a removed slot
//! reused by the next insertion (the most recently freed first). Variable
//! listings, dictionary keys and saved files all show this order.

use std::collections::HashMap;

/// A map with .NET `Dictionary` enumeration order.
#[derive(Debug, Clone)]
pub struct DotNetDictionary<V> {
    slots: Vec<Option<(String, V)>>,
    free: Vec<usize>,
    index: HashMap<String, usize>,
}

impl<V> Default for DotNetDictionary<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> DotNetDictionary<V> {
    /// An empty map.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// How many entries it holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Whether it is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// The value under `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&V> {
        let slot = *self.index.get(key)?;
        self.slots[slot].as_ref().map(|(_, value)| value)
    }

    /// Whether `key` is present.
    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.index.contains_key(key)
    }

    /// Sets `key` (the indexer): replaces in place, or adds in the next
    /// free slot.
    pub fn insert(&mut self, key: &str, value: V) {
        if let Some(slot) = self.index.get(key) {
            if let Some(entry) = self.slots[*slot].as_mut() {
                entry.1 = value;
            }
            return;
        }
        let slot = if let Some(slot) = self.free.pop() {
            self.slots[slot] = Some((key.to_owned(), value));
            slot
        } else {
            self.slots.push(Some((key.to_owned(), value)));
            self.slots.len() - 1
        };
        self.index.insert(key.to_owned(), slot);
    }

    /// `TryAdd`: false (and nothing changes) when the key is present.
    pub fn try_add(&mut self, key: &str, value: V) -> bool {
        if self.contains_key(key) {
            return false;
        }
        self.insert(key, value);
        true
    }

    /// Removes `key`; whether it was there.
    pub fn remove(&mut self, key: &str) -> bool {
        let Some(slot) = self.index.remove(key) else {
            return false;
        };
        self.slots[slot] = None;
        self.free.push(slot);
        true
    }

    /// Removes everything.
    pub fn clear(&mut self) {
        self.slots.clear();
        self.free.clear();
        self.index.clear();
    }

    /// The entries in enumeration order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.slots
            .iter()
            .filter_map(|slot| slot.as_ref().map(|(key, value)| (key.as_str(), value)))
    }

    /// The keys in enumeration order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.iter().map(|(key, _)| key)
    }

    /// The values in enumeration order.
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, value)| value)
    }
}

impl<V: Clone> DotNetDictionary<V> {
    /// A copy as `new Dictionary(other)` makes it: entries in the source's
    /// enumeration order, packed.
    #[must_use]
    pub fn copied(&self) -> Self {
        let mut copy = Self::new();
        for (key, value) in self.iter() {
            copy.insert(key, value.clone());
        }
        copy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_freed_slot_is_reused_by_the_next_insertion() {
        let mut map = DotNetDictionary::new();
        for key in ["a", "b", "c"] {
            map.insert(key, 0);
        }
        map.remove("a");
        map.insert("d", 1);
        assert_eq!(map.keys().collect::<Vec<_>>(), ["d", "b", "c"]);
        map.remove("b");
        map.remove("c");
        map.insert("e", 2);
        map.insert("f", 3);
        assert_eq!(map.keys().collect::<Vec<_>>(), ["d", "f", "e"]);
    }
}
