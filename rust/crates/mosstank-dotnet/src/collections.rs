//! `Dictionary<K, V>` and `HashSet<T>` as .NET enumerates them.
//!
//! .NET keeps a dictionary's entries in an array in the order they were
//! added; a removed entry's slot goes on a free list (last freed, first
//! reused) and the next addition takes it. Enumeration walks the array and
//! skips the free slots. So a dictionary that never loses an entry
//! enumerates in insertion order, and one that does puts the next addition
//! where the last removal was. MossTank's C# writes files by enumerating
//! such sets (the assist items, the consumable names), so the order is part
//! of its output, and these types keep it.
//!
//! The key comparer is a type: [`Ordinal`] compares as the key's own `Eq`
//! does, [`OrdinalIgnoreCase`] compares strings after the invariant
//! upper-case mapping.

use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;

use openac_plugin_compat::text::casing::to_upper_invariant;

/// How a collection compares its keys (.NET's `IEqualityComparer<T>`): by a
/// normalised form of the borrowed key.
pub trait Comparer {
    /// The borrowed form lookups take (`str` for `String` keys).
    type Item: ?Sized;
    /// The normalised form keys are compared by.
    type Key: Hash + Eq;
    /// The normalised form of `item`.
    fn key(item: &Self::Item) -> Self::Key;
}

/// `StringComparer.Ordinal` for strings, the default comparer otherwise.
#[derive(Debug)]
pub struct Ordinal<T: ?Sized>(PhantomData<fn(&T)>);

impl Comparer for Ordinal<str> {
    type Item = str;
    type Key = String;
    fn key(item: &str) -> String {
        item.to_owned()
    }
}

impl<T: Hash + Eq + Clone> Comparer for Ordinal<T> {
    type Item = T;
    type Key = T;
    fn key(item: &T) -> T {
        item.clone()
    }
}

/// `StringComparer.OrdinalIgnoreCase`.
#[derive(Debug)]
pub struct OrdinalIgnoreCase;

impl Comparer for OrdinalIgnoreCase {
    type Item = str;
    type Key = String;
    fn key(item: &str) -> String {
        to_upper_invariant(item)
    }
}

/// `Dictionary<K, V>`: lookups through the comparer `C`, enumeration in
/// .NET's slot order (see the module documentation).
pub struct DotNetDictionary<K, V, C: Comparer> {
    slots: Vec<Option<(K, V)>>,
    free: Vec<usize>,
    index: HashMap<C::Key, usize>,
}

impl<K, V, C: Comparer> Default for DotNetDictionary<K, V, C> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            index: HashMap::new(),
        }
    }
}

impl<K: Clone, V: Clone, C: Comparer> Clone for DotNetDictionary<K, V, C>
where
    C::Key: Clone,
{
    fn clone(&self) -> Self {
        Self {
            slots: self.slots.clone(),
            free: self.free.clone(),
            index: self.index.clone(),
        }
    }
}

impl<K: fmt::Debug, V: fmt::Debug, C: Comparer> fmt::Debug for DotNetDictionary<K, V, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<K: PartialEq, V: PartialEq, C: Comparer> PartialEq for DotNetDictionary<K, V, C> {
    /// The same entries in the same enumeration order.
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl<K, V, C: Comparer> DotNetDictionary<K, V, C> {
    /// The entries in enumeration order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.slots
            .iter()
            .filter_map(|slot| slot.as_ref().map(|(key, value)| (key, value)))
    }

    /// The keys in enumeration order.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(key, _)| key)
    }

    /// The values in enumeration order.
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, value)| value)
    }
}

impl<K, V, C> DotNetDictionary<K, V, C>
where
    K: Borrow<C::Item>,
    C: Comparer,
{
    /// An empty dictionary.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many entries it holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Whether it holds none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// The value under `key`.
    #[must_use]
    pub fn get(&self, key: &C::Item) -> Option<&V> {
        let slot = *self.index.get(&C::key(key))?;
        self.slots[slot].as_ref().map(|(_, value)| value)
    }

    /// The value under `key`, to change.
    pub fn get_mut(&mut self, key: &C::Item) -> Option<&mut V> {
        let slot = *self.index.get(&C::key(key))?;
        self.slots[slot].as_mut().map(|(_, value)| value)
    }

    /// `ContainsKey`.
    #[must_use]
    pub fn contains_key(&self, key: &C::Item) -> bool {
        self.index.contains_key(&C::key(key))
    }

    /// The indexer's set: replaces the value of an existing key where it
    /// stands (keeping the key as first added), or adds the entry. Returns
    /// the value replaced.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let normalised = C::key(key.borrow());
        if let Some(&slot) = self.index.get(&normalised) {
            let entry = self.slots[slot].as_mut().expect("an indexed slot is full");
            return Some(std::mem::replace(&mut entry.1, value));
        }
        let slot = match self.free.pop() {
            Some(slot) => {
                self.slots[slot] = Some((key, value));
                slot
            }
            None => {
                self.slots.push(Some((key, value)));
                self.slots.len() - 1
            }
        };
        self.index.insert(normalised, slot);
        None
    }

    /// `TryAdd`: adds the entry unless the key is there already.
    pub fn try_add(&mut self, key: K, value: V) -> bool {
        if self.contains_key(key.borrow()) {
            return false;
        }
        self.insert(key, value);
        true
    }

    /// `Remove`: the entry's slot is the next one an addition takes.
    pub fn remove(&mut self, key: &C::Item) -> Option<V> {
        let slot = self.index.remove(&C::key(key))?;
        self.free.push(slot);
        self.slots[slot].take().map(|(_, value)| value)
    }

    /// `Clear`.
    pub fn clear(&mut self) {
        self.slots.clear();
        self.free.clear();
        self.index.clear();
    }

    /// `RemoveAll`-style filtering in enumeration order, each removal
    /// freeing its slot as `Remove` does.
    pub fn retain(&mut self, mut keep: impl FnMut(&K, &V) -> bool) {
        for slot in 0..self.slots.len() {
            let Some((key, value)) = &self.slots[slot] else {
                continue;
            };
            if keep(key, value) {
                continue;
            }
            self.index.remove(&C::key(key.borrow()));
            self.slots[slot] = None;
            self.free.push(slot);
        }
    }
}

/// `HashSet<T>`: a [`DotNetDictionary`] with no values.
pub struct DotNetSet<T, C: Comparer> {
    map: DotNetDictionary<T, (), C>,
}

impl<T, C: Comparer> Default for DotNetSet<T, C> {
    fn default() -> Self {
        Self {
            map: DotNetDictionary::default(),
        }
    }
}

impl<T: Clone, C: Comparer> Clone for DotNetSet<T, C>
where
    C::Key: Clone,
{
    fn clone(&self) -> Self {
        Self {
            map: self.map.clone(),
        }
    }
}

impl<T: fmt::Debug, C: Comparer> fmt::Debug for DotNetSet<T, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.map.keys()).finish()
    }
}

impl<T: PartialEq, C: Comparer> PartialEq for DotNetSet<T, C> {
    fn eq(&self, other: &Self) -> bool {
        self.map.slots.iter().flatten().map(|(k, ())| k).eq(other
            .map
            .slots
            .iter()
            .flatten()
            .map(|(k, ())| k))
    }
}

impl<T, C> DotNetSet<T, C>
where
    T: Borrow<C::Item>,
    C: Comparer,
{
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many items it holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether it holds none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// `Contains`.
    #[must_use]
    pub fn contains(&self, item: &C::Item) -> bool {
        self.map.contains_key(item)
    }

    /// `Add`: false when the item is there already.
    pub fn insert(&mut self, item: T) -> bool {
        self.map.try_add(item, ())
    }

    /// `Remove`.
    pub fn remove(&mut self, item: &C::Item) -> bool {
        self.map.remove(item).is_some()
    }

    /// `Clear`.
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// `RemoveWhere`.
    pub fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        self.map.retain(|item, ()| keep(item));
    }

    /// The items in enumeration order.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.map.keys()
    }
}

impl<T, C> FromIterator<T> for DotNetSet<T, C>
where
    T: Borrow<C::Item>,
    C: Comparer,
{
    fn from_iter<I: IntoIterator<Item = T>>(items: I) -> Self {
        let mut set = Self::new();
        for item in items {
            set.insert(item);
        }
        set
    }
}

/// A `HashSet<string>` with `StringComparer.Ordinal`.
pub type OrdinalStringSet = DotNetSet<String, Ordinal<str>>;

/// A `Dictionary<string, V>` with `StringComparer.Ordinal`.
pub type OrdinalStringMap<V> = DotNetDictionary<String, V, Ordinal<str>>;

/// A `Dictionary<string, V>` with `StringComparer.OrdinalIgnoreCase`.
pub type IgnoreCaseStringMap<V> = DotNetDictionary<String, V, OrdinalIgnoreCase>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_removed_slot_is_the_next_one_filled() {
        let mut set: OrdinalStringSet = ["a", "b", "c", "d"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert!(set.remove("b"));
        assert!(set.remove("d"));
        set.insert("e".to_owned());
        set.insert("f".to_owned());
        set.insert("g".to_owned());
        let order: Vec<&str> = set.iter().map(String::as_str).collect();
        // d's slot was freed last, so e takes it; then b's; then a new one.
        assert_eq!(order, ["a", "f", "c", "e", "g"]);
    }

    #[test]
    fn ignore_case_keys_keep_their_first_spelling() {
        let mut map: IgnoreCaseStringMap<i32> = DotNetDictionary::new();
        map.insert("Setting".to_owned(), 1);
        assert_eq!(map.insert("SETTING".to_owned(), 2), Some(1));
        assert_eq!(map.get("setting"), Some(&2));
        assert_eq!(map.keys().next().map(String::as_str), Some("Setting"));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn numbers_compare_as_themselves() {
        let mut set: DotNetSet<u32, Ordinal<u32>> = DotNetSet::new();
        assert!(set.insert(7));
        assert!(!set.insert(7));
        assert!(set.contains(&7));
        set.retain(|value| *value != 7);
        assert!(set.is_empty());
    }
}
