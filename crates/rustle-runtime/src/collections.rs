//! Observable collections: a list and a map whose changes recompose.

use std::collections::HashMap;
use std::hash::Hash;

use crate::composition::{group, remember};
use crate::state::State;

/// A list that recomposes when it changes. Cloning shares the list.
pub struct StateList<T> {
    items: State<Vec<T>>,
}

impl<T> Clone for StateList<T> {
    fn clone(&self) -> Self {
        Self {
            items: self.items.clone(),
        }
    }
}

impl<T: Clone + 'static> StateList<T> {
    pub fn len(&self) -> usize {
        self.items.with(Vec::len)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, index: usize) -> Option<T> {
        self.items.with(|items| items.get(index).cloned())
    }

    /// A copy of the items as they are now.
    pub fn to_vec(&self) -> Vec<T> {
        self.items.get()
    }

    pub fn push(&self, item: T) {
        self.items.update(|items| items.push(item));
    }

    /// Inserts `item` before `index`, or at the end if `index` is past it.
    pub fn insert(&self, index: usize, item: T) {
        self.items.update(|items| {
            let index = index.min(items.len());
            items.insert(index, item);
        });
    }

    /// Removes and returns the item at `index`, if there is one.
    pub fn remove(&self, index: usize) -> Option<T> {
        let mut removed = None;
        self.items.update(|items| {
            if index < items.len() {
                removed = Some(items.remove(index));
            }
        });
        removed
    }

    /// Replaces the item at `index`, if there is one.
    pub fn set(&self, index: usize, item: T) {
        self.items.update(|items| {
            if let Some(slot) = items.get_mut(index) {
                *slot = item;
            }
        });
    }

    pub fn clear(&self) {
        self.items.update(Vec::clear);
    }

    /// Changes the whole list in place.
    pub fn update(&self, change: impl FnOnce(&mut Vec<T>)) {
        self.items.update(change);
    }
}

/// Remembers a [`StateList`] at this call site, starting with what `init` returns.
#[track_caller]
pub fn remember_state_list<T: Clone + 'static>(init: impl FnOnce() -> Vec<T>) -> StateList<T> {
    group(|| StateList {
        items: remember(init),
    })
}

/// A map that recomposes when it changes. Cloning shares the map.
pub struct StateMap<K, V> {
    entries: State<HashMap<K, V>>,
}

impl<K, V> Clone for StateMap<K, V> {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
        }
    }
}

impl<K: Hash + Eq + Clone + 'static, V: Clone + 'static> StateMap<K, V> {
    pub fn len(&self) -> usize {
        self.entries.with(HashMap::len)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, key: &K) -> Option<V> {
        self.entries.with(|entries| entries.get(key).cloned())
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.entries.with(|entries| entries.contains_key(key))
    }

    /// Sets the value of `key`, returning the one it replaced.
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        let mut replaced = None;
        self.entries
            .update(|entries| replaced = entries.insert(key, value));
        replaced
    }

    pub fn remove(&self, key: &K) -> Option<V> {
        let mut removed = None;
        self.entries.update(|entries| removed = entries.remove(key));
        removed
    }

    pub fn clear(&self) {
        self.entries.update(HashMap::clear);
    }

    /// A copy of the entries as they are now.
    pub fn to_map(&self) -> HashMap<K, V> {
        self.entries.get()
    }
}

/// Remembers a [`StateMap`] at this call site, starting with what `init` returns.
#[track_caller]
pub fn remember_state_map<K: Hash + Eq + Clone + 'static, V: Clone + 'static>(
    init: impl FnOnce() -> HashMap<K, V>,
) -> StateMap<K, V> {
    group(|| StateMap {
        entries: remember(init),
    })
}
