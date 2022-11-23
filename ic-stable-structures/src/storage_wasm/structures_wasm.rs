use std::marker::PhantomData;

use ic_exports::stable_structures::{btreemap, cell, memory_manager::MemoryId, Storable};
use ic_exports::stable_structures::{log, BoundedStorable};

use crate::unbounded::{self, SlicedStorable};
use crate::{get_memory_by_id, multimap, Error, Iter, RangeIter};
use crate::{Memory, Result};

/// Stores value in stable memory, providing `get()/set()` API.
pub struct StableCell<T: Storable>(cell::Cell<T, Memory>);

impl<T: Storable> StableCell<T> {
    /// Create new storage for values with `T` type.
    pub fn new(memory_id: MemoryId, value: T) -> Result<Self> {
        let memory = super::get_memory_by_id(memory_id);
        let cell = cell::Cell::init(memory, value)?;
        Ok(Self(cell))
    }

    /// Returns reference to value stored in stable memory.
    pub fn get(&self) -> &T {
        self.0.get()
    }

    /// Updates value in stable memory.
    pub fn set(&mut self, value: T) -> Result<()> {
        self.0.set(value)?;
        Ok(())
    }
}
/// Stores key-value data in stable memory.
pub struct StableBTreeMap<K, V>(btreemap::BTreeMap<Memory, K, V>)
where
    K: BoundedStorable,
    V: BoundedStorable;

impl<K, V> StableBTreeMap<K, V>
where
    K: BoundedStorable,
    V: BoundedStorable,
{
    /// Create new instance of key-value storage.
    pub fn new(memory_id: MemoryId) -> Self {
        let memory = get_memory_by_id(memory_id);
        Self(btreemap::BTreeMap::init(memory))
    }

    /// Return value associated with `key` from stable memory.
    pub fn get(&self, key: &K) -> Option<V> {
        self.0.get(key)
    }

    /// Add or replace value associated with `key` in stable memory.
    pub fn insert(&mut self, key: K, value: V) -> Result<()> {
        self.0.insert(key, value)?;
        Ok(())
    }

    /// Remove value associated with `key` from stable memory.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.0.remove(key)
    }

    /// Iterate over all currently stored key-value pairs.
    pub fn iter(&self) -> btreemap::Iter<'_, Memory, K, V> {
        self.0.iter()
    }
}

/// `StableMultimap` stores two keys against a single value, making it possible
/// to fetch all values by the root key, or a single value by specifying both keys.
pub struct StableMultimap<K1, K2, V>(multimap::StableMultimap<Memory, K1, K2, V>)
where
    K1: BoundedStorable,
    K2: BoundedStorable,
    V: BoundedStorable;

impl<K1, K2, V> StableMultimap<K1, K2, V>
where
    K1: BoundedStorable,
    K2: BoundedStorable,
    V: BoundedStorable,
{
    /// Create a new instance of a `StableMultimap`.
    /// All keys and values byte representations should be less then related `..._max_size` arguments.
    pub fn new(memory_id: MemoryId) -> Self {
        let memory = crate::get_memory_by_id(memory_id);
        Self(multimap::StableMultimap::new(memory))
    }

    /// Get a value for the given keys.
    /// If byte representation length of any key exceeds max size, `None` will be returned.
    pub fn get(&self, first_key: &K1, second_key: &K2) -> Option<V> {
        self.0.get(first_key, second_key)
    }

    /// Insert a new value into the map.
    /// Inserting a value with the same keys as an existing value
    /// will result in the old value being overwritten.
    ///
    /// # Errors
    ///
    /// If byte representation length of any key or value exceeds max size, the `Error::ValueTooLarge`
    /// will be returned.
    ///
    /// If stable memory unable to grow, the `Error::OutOfStableMemory` will be returned.
    pub fn insert(&mut self, first_key: &K1, second_key: &K2, value: &V) -> Result<()> {
        self.0.insert(first_key, second_key, value)
    }

    /// Remove a specific value and return it.
    ///
    /// # Errors
    ///
    /// If byte representation length of any key exceeds max size, the `Error::ValueTooLarge`
    /// will be returned.
    pub fn remove(&mut self, first_key: &K1, second_key: &K2) -> Result<Option<V>> {
        self.0.remove(first_key, second_key)
    }

    /// Remove all values for the partial key
    ///
    /// # Errors
    ///
    /// If byte representation length of `first_key` exceeds max size, the `Error::ValueTooLarge`
    /// will be returned.
    pub fn remove_partial(&mut self, first_key: &K1) -> Result<()> {
        self.0.remove_partial(first_key)
    }

    /// Get a range of key value pairs based on the root key.
    ///
    /// # Errors
    ///
    /// If byte representation length of `first_key` exceeds max size, the `Error::ValueTooLarge`
    /// will be returned.
    pub fn range(&self, first_key: &K1) -> Result<RangeIter<Memory, K1, K2, V>> {
        self.0.range(first_key)
    }

    /// Iterator over all items in map.
    pub fn iter(&self) -> Iter<Memory, K1, K2, V> {
        self.0.iter()
    }

    /// Items count.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Is map empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Stores list of immutable values in stable memory.
/// Provides only `append()` and `get()` operations.
pub struct StableLog<T: Storable>(log::Log<Memory, Memory>, PhantomData<T>);

impl<T: Storable> StableLog<T> {
    /// Create new storage for values with `T` type.
    pub fn new(index_memory_id: MemoryId, data_memory_id: MemoryId) -> Result<Self> {
        // Method returns Result to be compatible with wasm implementation.

        // Index and data should be stored in different memories.
        assert_ne!(index_memory_id, data_memory_id);

        let index_memory = crate::get_memory_by_id(index_memory_id);
        let data_memory = crate::get_memory_by_id(data_memory_id);

        Ok(Self(
            log::Log::new(index_memory, data_memory),
            PhantomData::default(),
        ))
    }

    /// Returns reference to value stored in stable memory.
    pub fn get(&self, index: usize) -> Option<T> {
        self.0.get(index).map(T::from_bytes)
    }

    /// Updates value in stable memory.
    pub fn append(&mut self, value: T) -> Result<usize> {
        self.0
            .append(&value.to_bytes())
            .map_err(|_| Error::OutOfStableMemory)
    }

    /// Count of values in the log.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    // Return true, if the Log doesn't contain any value.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Stores key-value data in stable memory.
pub struct StableUnboundedMap<K, V>(unbounded::StableUnboundedMap<Memory, K, V>)
where
    K: BoundedStorable,
    V: SlicedStorable;

impl<K, V> StableUnboundedMap<K, V>
where
    K: BoundedStorable,
    V: SlicedStorable,
{
    /// Create new instance of key-value storage.
    ///
    /// If a memory with the `memory_id` contains data of the map, the map reads it, and the instance
    /// will contain the data from the memory.
    pub fn new(memory_id: MemoryId) -> Self {
        let memory = crate::get_memory_by_id(memory_id);
        Self(unbounded::StableUnboundedMap::new(memory))
    }

    /// Return value associated with `key` from stable memory.
    pub fn get(&self, key: &K) -> Option<V> {
        self.0.get(key)
    }

    /// Add or replace value associated with `key` in stable memory.
    pub fn insert(&mut self, key: &K, value: &V) -> Result<()> {
        self.0.insert(&key, &value)
    }

    /// Remove value associated with `key` from stable memory.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.0.remove(key)
    }

    /// List all currently stored key-value pairs.
    pub fn iter(&self) -> unbounded::Iter<'_, Memory, K, V> {
        self.0.iter()
    }

    /// Count of items in the map.
    pub fn len(&self) -> u64 {
        self.0.len()
    }

    /// Is the map empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
