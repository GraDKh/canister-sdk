use std::borrow::Cow;
use std::marker::PhantomData;

use ic_exports::stable_structures::{btreemap, BoundedStorable, Memory, StableBTreeMap, Storable};

// Keys memory layout:
//
// |- k1 size in bytes -|- k1 bytes -|- k2 bytes |
//
// Size of k1 is stored because we need to make a difference between
// a k1 bytes and another shorter k1 bytes + k2 start bytes.
// For example, we have two key pairs with byte patterns:
// 1) k1 = [0x1, 0x2, 0x3] and k2 = [0x4, 0x5]
// 2) k1 = [0x1, 0x2] and k2 = [0x3, 0x4, 0x5]
//
// Concatination of both key pairs is same: [0x1, 0x2, 0x3, 0x4, 0x5],
// but with the `k1 size` prefix, it is different:
// 1) [0x3, 0x1, 0x2, 0x3, 0x4, 0x5]
// 2) [0x2, 0x1, 0x2, 0x3, 0x4, 0x5]
//
// Bytes count of `k1 size` is calculated from the `first_key_max_size` (see `size_bytes_len()`). Usually,
// keys are shorter then 256 bytes, so, size overhead will be just one byte per value.
// Inner [`StableBTreeMap`] limits max size by `u32::MAX`, so in worst case
// (for keys with max size greater then 65535), we will spend four bytes per value.

/// `StableMultimap` stores two keys against a single value, making it possible
/// to fetch all values by the root key, or a single value by specifying both keys.
pub struct StableMultimap<M, K1, K2, V>(StableBTreeMap<KeyPair<K1, K2>, Value<V>, M>)
where
    M: Memory + Clone,
    K1: BoundedStorable,
    K2: BoundedStorable,
    V: BoundedStorable;

impl<M, K1, K2, V> StableMultimap<M, K1, K2, V>
where
    M: Memory + Clone,
    K1: BoundedStorable,
    K2: BoundedStorable,
    V: BoundedStorable,
{
    /// Create a new instance of a `StableMultimap`.
    /// All keys and values byte representations should be less then related `..._max_size` arguments.
    pub fn new(memory: M) -> Self {
        Self(StableBTreeMap::init(memory))
    }

    /// Insert a new value into the map.
    /// Inserting a value with the same keys as an existing value
    /// will result in the old value being overwritten.
    ///
    /// # Preconditions:
    ///   - `first_key.to_bytes().len() <= K1::MAX_SIZE`
    ///   - `second_key.to_bytes().len() <= K2::MAX_SIZE`
    ///   - `value.to_bytes().len() <= V::MAX_SIZE`
    pub fn insert(&mut self, first_key: &K1, second_key: &K2, value: &V) -> Option<V> {
        let key = KeyPair::new(first_key, second_key);
        self.0.insert(key, value.into()).map(|v| v.into_inner())
    }

    /// Get a value for the given keys.
    /// If byte representation length of any key exceeds max size, `None` will be returned.
    ///
    /// # Preconditions:
    ///   - `first_key.to_bytes().len() <= K1::MAX_SIZE`
    ///   - `second_key.to_bytes().len() <= K2::MAX_SIZE`
    pub fn get(&self, first_key: &K1, second_key: &K2) -> Option<V> {
        let key = KeyPair::new(first_key, second_key);
        self.0.get(&key).map(|v| v.into_inner())
    }

    /// Remove a specific value and return it.
    ///
    /// # Preconditions:
    ///   - `first_key.to_bytes().len() <= K1::MAX_SIZE`
    ///   - `second_key.to_bytes().len() <= K2::MAX_SIZE`
    pub fn remove(&mut self, first_key: &K1, second_key: &K2) -> Option<V> {
        let key = KeyPair::new(first_key, second_key);

        self.0.remove(&key).map(Value::into_inner)
    }

    /// Remove all values for the partial key
    ///
    /// # Preconditions:
    ///   - `first_key.to_bytes().len() <= K1::MAX_SIZE`
    pub fn remove_partial(&mut self, first_key: &K1) {
        let min_key = KeyPair::<K1, K2>::min_key(first_key);
        let max_key = KeyPair::<K1, K2>::max_key(first_key);

        let keys: Vec<_> = self
            .0
            .range(min_key..=max_key)
            .map(|(keys, _)| keys)
            .collect();

        for k in keys {
            let _ = self.0.remove(&k);
        }
    }

    /// Get a range of key value pairs based on the root key.
    ///
    /// # Preconditions:
    ///   - `first_key.to_bytes().len() <= K1::MAX_SIZE`
    pub fn range(&self, first_key: &K1) -> RangeIter<M, K1, K2, V> {
        let min_key = KeyPair::<K1, K2>::min_key(first_key);
        let max_key = KeyPair::<K1, K2>::max_key(first_key);

        let inner = self.0.range(min_key..=max_key);
        RangeIter::new(inner)
    }

    /// Iterator over all items in the map.
    pub fn iter(&self) -> Iter<M, K1, K2, V> {
        Iter::new(self.0.iter())
    }

    /// Item count.
    pub fn len(&self) -> usize {
        self.0.len() as usize
    }

    /// Is the map empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&mut self) {
        let keys: Vec<_> = self.0.iter().map(|(k, _)| k).collect();
        for key in keys {
            self.0.remove(&key);
        }
    }
}

struct KeyPair<K1, K2> {
    encoded: Vec<u8>,
    first_key_len: usize,
    _p: PhantomData<(K1, K2)>,
}

impl<K1: BoundedStorable, K2: BoundedStorable> Clone for KeyPair<K1, K2> {
    fn clone(&self) -> Self {
        Self {
            encoded: self.encoded.clone(),
            first_key_len: self.first_key_len,
            _p: PhantomData,
        }
    }
}

impl<K1: BoundedStorable, K2: BoundedStorable> PartialEq for KeyPair<K1, K2> {
    fn eq(&self, other: &Self) -> bool {
        self.encoded == other.encoded && self.first_key_len == other.first_key_len
    }
}

impl<K1: BoundedStorable, K2: BoundedStorable> Eq for KeyPair<K1, K2> {}

impl<K1: BoundedStorable, K2: BoundedStorable> PartialOrd for KeyPair<K1, K2> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.encoded.partial_cmp(&other.encoded)
    }
}

impl<K1: BoundedStorable, K2: BoundedStorable> Ord for KeyPair<K1, K2> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.encoded.cmp(&other.encoded)
    }
}

impl<K1, K2> KeyPair<K1, K2>
where
    K1: BoundedStorable,
    K2: BoundedStorable,
{
    /// # Preconditions:
    ///   - `first_key.to_bytes().len() <= K1::MAX_SIZE`
    ///   - `second_key.to_bytes().len() <= K2::MAX_SIZE`
    pub fn new(first_key: &K1, second_key: &K2) -> Self {
        let first_key_bytes = first_key.to_bytes();
        let second_key_bytes = second_key.to_bytes();

        assert!(first_key_bytes.len() <= K1::MAX_SIZE as usize);
        assert!(second_key_bytes.len() <= K2::MAX_SIZE as usize);

        let full_len = Self::size_prefix_len() + first_key_bytes.len() + second_key_bytes.len();
        let mut buffer = Vec::with_capacity(full_len);
        Self::push_size_prefix(&mut buffer, first_key_bytes.len());
        buffer.extend_from_slice(&first_key_bytes);
        buffer.extend_from_slice(&second_key_bytes);

        Self {
            encoded: buffer,
            first_key_len: first_key_bytes.len(),
            _p: PhantomData,
        }
    }

    pub fn first_key(&self) -> K1 {
        let offset = Self::size_prefix_len();
        K1::from_bytes(self.encoded[offset..offset + self.first_key_len].into())
    }

    /// Minimum possible `KeyPair` for the specified `first_key`.
    pub fn min_key(first_key: &K1) -> Self {
        let first_key_bytes = first_key.to_bytes();

        assert!(first_key_bytes.len() <= K1::MAX_SIZE as usize);

        let full_len = Self::size_prefix_len() + first_key_bytes.len();
        let mut buffer = Vec::with_capacity(full_len);
        Self::push_size_prefix(&mut buffer, first_key_bytes.len());
        buffer.extend_from_slice(&first_key_bytes);

        Self {
            encoded: buffer,
            first_key_len: first_key_bytes.len(),
            _p: PhantomData,
        }
    }

    /// Maximum possible `KeyPair` for the specified `first_key`.
    pub fn max_key(first_key: &K1) -> Self {
        let first_key_bytes = first_key.to_bytes();

        assert!(first_key_bytes.len() <= K1::MAX_SIZE as usize);

        let full_len = Self::size_prefix_len() + first_key_bytes.len();
        let mut buffer = Vec::with_capacity(full_len);
        Self::push_size_prefix(&mut buffer, first_key_bytes.len());
        buffer.extend_from_slice(&first_key_bytes);
        buffer.resize(Self::MAX_SIZE as _, 0xFF);

        Self {
            encoded: buffer,
            first_key_len: first_key_bytes.len(),
            _p: PhantomData,
        }
    }

    pub fn second_key(&self) -> K2 {
        let offset = Self::size_prefix_len() + self.first_key_len;
        K2::from_bytes(self.encoded[offset..].into())
    }

    fn push_size_prefix(buf: &mut Vec<u8>, first_key_size: usize) {
        buf.extend_from_slice(&first_key_size.to_le_bytes()[..Self::size_prefix_len()]);
    }

    const fn size_prefix_len() -> usize {
        const U8_MAX: u32 = u8::MAX as u32;
        const U8_END: u32 = U8_MAX + 1;
        const U16_MAX: u32 = u16::MAX as u32;

        match K1::MAX_SIZE {
            0..=U8_MAX => 1,
            U8_END..=U16_MAX => 2,
            _ => 4,
        }
    }

    fn read_first_key_len(encoded: &[u8]) -> usize {
        let mut size_bytes = [0u8; 4];
        let size_prefix_len = Self::size_prefix_len();
        size_bytes[..size_prefix_len].copy_from_slice(&encoded[..size_prefix_len]);
        u32::from_le_bytes(size_bytes) as _
    }
}

impl<K1, K2> Storable for KeyPair<K1, K2>
where
    K1: BoundedStorable,
    K2: BoundedStorable,
{
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(&self.encoded)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let first_key_len = Self::read_first_key_len(&bytes);

        Self {
            encoded: bytes.to_vec(),
            first_key_len,
            _p: PhantomData,
        }
    }
}

impl<K1, K2> BoundedStorable for KeyPair<K1, K2>
where
    K1: BoundedStorable,
    K2: BoundedStorable,
{
    const MAX_SIZE: u32 = Self::size_prefix_len() as u32 + K1::MAX_SIZE + K2::MAX_SIZE;

    const IS_FIXED_SIZE: bool = false;
}

struct Value<V>(Vec<u8>, PhantomData<V>);

impl<V: Storable> Value<V> {
    pub fn into_inner(self) -> V {
        V::from_bytes(self.0.into())
    }
}

impl<V: Storable> From<&V> for Value<V> {
    fn from(value: &V) -> Self {
        Self(value.to_bytes().into(), PhantomData)
    }
}

impl<V> Storable for Value<V> {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(&self.0)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        Self(bytes.to_vec(), PhantomData)
    }
}

impl<V: BoundedStorable> BoundedStorable for Value<V> {
    const MAX_SIZE: u32 = V::MAX_SIZE;
    const IS_FIXED_SIZE: bool = V::IS_FIXED_SIZE;
}

/// Range iterator
pub struct RangeIter<'a, M, K1, K2, V>
where
    M: Memory + Clone,
    K1: BoundedStorable,
    K2: BoundedStorable,
    V: BoundedStorable,
{
    inner: btreemap::Iter<'a, KeyPair<K1, K2>, Value<V>, M>,
}

impl<'a, M, K1, K2, V> RangeIter<'a, M, K1, K2, V>
where
    M: Memory + Clone,
    K1: BoundedStorable,
    K2: BoundedStorable,
    V: BoundedStorable,
{
    fn new(inner: btreemap::Iter<'a, KeyPair<K1, K2>, Value<V>, M>) -> Self {
        Self { inner }
    }
}

// -----------------------------------------------------------------------------
//     - Range Iterator impl -
// -----------------------------------------------------------------------------
impl<'a, M, K1, K2, V> Iterator for RangeIter<'a, M, K1, K2, V>
where
    M: Memory + Clone,
    K1: BoundedStorable,
    K2: BoundedStorable,
    V: BoundedStorable,
{
    type Item = (K2, V);

    fn next(&mut self) -> Option<(K2, V)> {
        self.inner
            .next()
            .map(|(keys, v)| (keys.second_key(), v.into_inner()))
    }
}

pub struct Iter<'a, M, K1, K2, V>(btreemap::Iter<'a, KeyPair<K1, K2>, Value<V>, M>)
where
    M: Memory + Clone,
    K1: BoundedStorable,
    K2: BoundedStorable,
    V: BoundedStorable;

impl<'a, M, K1, K2, V> Iter<'a, M, K1, K2, V>
where
    M: Memory + Clone,
    K1: BoundedStorable,
    K2: BoundedStorable,
    V: BoundedStorable,
{
    fn new(inner: btreemap::Iter<'a, KeyPair<K1, K2>, Value<V>, M>) -> Self {
        Self(inner)
    }
}

impl<'a, M, K1, K2, V> Iterator for Iter<'a, M, K1, K2, V>
where
    M: Memory + Clone,
    K1: BoundedStorable,
    K2: BoundedStorable,
    V: BoundedStorable,
{
    type Item = (K1, K2, V);

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(keys, val)| {
            let k1 = keys.first_key();
            let k2 = keys.second_key();
            (k1, k2, val.into_inner())
        })
    }
}

impl<'a, M, K1, K2, V> IntoIterator for &'a StableMultimap<M, K1, K2, V>
where
    M: Memory + Clone,
    K1: BoundedStorable,
    K2: BoundedStorable,
    V: BoundedStorable,
{
    type Item = (K1, K2, V);

    type IntoIter = Iter<'a, M, K1, K2, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod test {
    use std::borrow::Cow;

    use ic_exports::stable_structures::DefaultMemoryImpl;

    use super::*;

    /// New type pattern used to implement `Storable` trait for all arrays.
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    struct Array<const N: usize>(pub [u8; N]);

    impl<const N: usize> Storable for Array<N> {
        fn to_bytes(&self) -> Cow<'_, [u8]> {
            Cow::Owned(self.0.to_vec())
        }

        fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
            let mut buf = [0u8; N];
            buf.copy_from_slice(&bytes);
            Array(buf)
        }
    }

    impl<const N: usize> BoundedStorable for Array<N> {
        const MAX_SIZE: u32 = N as _;
        const IS_FIXED_SIZE: bool = true;
    }

    fn make_map() -> StableMultimap<DefaultMemoryImpl, Array<2>, Array<3>, Array<6>> {
        let mut mm = StableMultimap::new(DefaultMemoryImpl::default());
        let k1 = Array([1u8, 2]);
        let k2 = Array([11u8, 12, 13]);
        let val = Array([200u8, 200, 200, 100, 100, 123]);
        mm.insert(&k1, &k2, &val);

        let k1 = Array([10u8, 20]);
        let k2 = Array([21u8, 22, 23]);
        let val = Array([123, 200u8, 200, 100, 100, 255]);
        mm.insert(&k1, &k2, &val);

        mm
    }

    #[test]
    fn inserts() {
        let mut mm = StableMultimap::new(DefaultMemoryImpl::default());
        for i in 0..10 {
            let k1 = Array([i; 1]);
            let k2 = Array([i * 10; 2]);
            let val = Array([i; 1]);
            mm.insert(&k1, &k2, &val);
        }

        assert_eq!(mm.len(), 10);
    }

    #[test]
    fn insert_should_replace_old_value() {
        let mut mm = make_map();

        let k1 = Array([1u8, 2]);
        let k2 = Array([11u8, 12, 13]);
        let val = Array([255u8, 255, 255, 255, 255, 255]);

        let prev_val = Array([200u8, 200, 200, 100, 100, 123]);
        let replaced_val = mm.insert(&k1, &k2, &val).unwrap();

        assert_eq!(prev_val, replaced_val);
        assert_eq!(mm.get(&k1, &k2), Some(val));
    }

    #[test]
    fn get() {
        let mm = make_map();
        let k1 = Array([1u8, 2]);
        let k2 = Array([11u8, 12, 13]);
        let val = mm.get(&k1, &k2).unwrap();

        let expected = Array([200u8, 200, 200, 100, 100, 123]);
        assert_eq!(val, expected);
    }

    #[test]
    fn remove() {
        let mut mm = make_map();
        let k1 = Array([1u8, 2]);
        let k2 = Array([11u8, 12, 13]);
        let val = mm.remove(&k1, &k2).unwrap();

        let expected = Array([200u8, 200, 200, 100, 100, 123]);
        assert_eq!(val, expected);
        assert_eq!(mm.len(), 1);

        let k1 = Array([10u8, 20]);
        let k2 = Array([21u8, 22, 23]);
        mm.remove(&k1, &k2).unwrap();
        assert!(mm.is_empty());
    }

    #[test]
    fn remove_partial() {
        let mut mm = StableMultimap::new(DefaultMemoryImpl::default());
        let k1 = Array([1u8, 2]);
        let k2 = Array([11u8, 12, 13]);
        let val = Array([200u8, 200, 200, 100, 100, 123]);
        mm.insert(&k1, &k2, &val);

        let k2 = Array([21u8, 22, 23]);
        let val = Array([123, 200u8, 200, 100, 100, 255]);
        mm.insert(&k1, &k2, &val);

        mm.remove_partial(&k1);
        assert!(mm.is_empty());
    }

    #[test]
    fn clear() {
        let mut mm = StableMultimap::new(DefaultMemoryImpl::default());
        let k1 = Array([1u8, 2]);
        let k2 = Array([11u8, 12, 13]);
        let val = Array([200u8, 200, 200, 100, 100, 123]);
        mm.insert(&k1, &k2, &val);

        let k2 = Array([21u8, 22, 23]);
        let val = Array([123, 200u8, 200, 100, 100, 255]);
        mm.insert(&k1, &k2, &val);
        let k1 = Array([21u8, 22]);
        mm.insert(&k1, &k2, &val);

        mm.clear();
        assert!(mm.is_empty());
    }

    #[test]
    fn iter() {
        let mm = make_map();
        let mut iter = mm.into_iter();
        assert!(iter.next().is_some());
        assert!(iter.next().is_some());
        assert!(iter.next().is_none());
    }

    #[test]
    fn range_iter() {
        let k1 = Array([1u8, 2]);
        let mm = make_map();
        let mut iter = mm.range(&k1);
        assert!(iter.next().is_some());
        assert!(iter.next().is_none());
    }
}
