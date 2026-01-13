#![cfg_attr(feature = "no_std_support", no_std)]

// Use alloc crate only when no_std_support feature is enabled
#[cfg(feature = "no_std_support")]
extern crate alloc;

// --- Conditional Imports for Vec and VecIntoIter ---

// Use alloc's Vec and IntoIter when 'no_std_support' is enabled
#[cfg(feature = "no_std_support")]
use alloc::vec::{Vec, IntoIter as VecIntoIter};

// Use std's Vec and IntoIter when 'no_std_support' is NOT enabled
#[cfg(not(feature = "no_std_support"))]
use std::vec::{Vec, IntoIter as VecIntoIter};

// --- Other Core/Standard Imports ---
use core::fmt::{self, Debug};
use core::ops::{Index, IndexMut};
use core::iter::Enumerate;

/// A map of type `<usize, T>` where the keys (`usize`) are generated and reused by the map.
///
/// This structure provides dense storage using a `Vec<Option<T>>`. When items
/// are removed, their indices are stored and reused for future insertions,
/// minimizing memory fragmentation over time compared to simply pushing new elements.
/// Access time is O(1), insertion is amortized O(1), and removal is O(1).
pub struct UsizeMap<T> {
  /// The contiguous storage for elements. Slots can be `Some(T)` or `None`.
  data: Vec<Option<T>>,
  /// A list of indices corresponding to `None` slots in `data` that can be reused.
  empty: Vec<usize>,
  /// The number of `Some(T)` elements currently stored.
  count: usize,
}

impl<T> UsizeMap<T> {
  /// Creates a new, empty `UsizeMap`.
  pub fn new() -> Self {
    UsizeMap {
      data: Vec::new(),
      empty: Vec::new(),
      count: 0,
    }
  }

  /// Creates a new, empty `UsizeMap` with a specified initial capacity.
  pub fn with_capacity(capacity: usize) -> Self {
    UsizeMap {
      data: Vec::with_capacity(capacity),
      empty: Vec::new(),
      count: 0,
    }
  }

  /// Returns the number of elements the map can hold without reallocating.
  pub fn capacity(&self) -> usize {
    self.data.capacity()
  }

  /// Reserves capacity for at least `additional` more elements to be inserted.
  ///
  /// The collection may reserve more space to avoid frequent reallocations.
  /// After calling `reserve`, capacity will be greater than or equal to `self.len() + additional`.
  /// Does nothing if capacity is already sufficient.
  pub fn reserve(&mut self, additional: usize) {
      // We can subtract the number of reusable slots from the request
      // because insertions reuse slots before growing `data`.
      let reusable = self.empty.len();
      if additional > reusable {
          self.data.reserve(additional - reusable);
      }
  }

  /// Shrinks the capacity of the underlying vectors as much as possible.
  ///
  /// It will drop down as close as possible to the length but the allocator may still leave
  /// some space.
  pub fn shrink_to_fit(&mut self) {
      self.data.shrink_to_fit();
      self.empty.shrink_to_fit();
  }

  /// Inserts an element into the map, returning the `usize` key assigned to it.
  ///
  /// If there are previously removed slots, one will be reused. Otherwise,
  /// the element is appended to the underlying storage.
  pub fn insert(&mut self, element: T) -> usize {
    self.count += 1;
    if let Some(index) = self.empty.pop() {
      // Reuse an empty slot - O(1)

      // SAFETY CHECK:
      // The old logic resized here if index >= len. This was dangerous (vector bombing risk).
      // We now strictly enforce the invariant that `empty` only contains valid indices.
      if index >= self.data.len() {
         panic!("UsizeMap Integrity Error: recovered key {} is out of bounds (len {})", index, self.data.len());
      }

      debug_assert!(self.data[index].is_none(), "UsizeMap logic error: Empty slot index {} pointed to a non-empty slot!", index);

      self.data[index] = Some(element);
      index
    } else {
      // Append to the end - Amortized O(1)
      let index = self.data.len();
      self.data.push(Some(element));
      index
    }
  }

  /// Removes the element associated with the given key, returning it if it existed.
  ///
  /// The key is added to a list of reusable keys for future insertions.
  pub fn remove(&mut self, key: usize) -> Option<T> {
    match self.data.get_mut(key).and_then(|slot| slot.take()) {
      Some(value) => {
        self.count -= 1;
        self.empty.push(key);
        Some(value)
      }
      None => None,
    }
  }

  /// Retains only the elements specified by the predicate.
  ///
  /// In other words, remove all elements `e` such that `f(index, &mut e)` returns `false`.
  /// This is significantly faster than iterating and calling remove individually because
  /// it avoids key lookups and handles the free-list updating in a single pass.
  pub fn retain<F>(&mut self, mut f: F)
  where
      F: FnMut(usize, &mut T) -> bool,
  {
      for (index, slot) in self.data.iter_mut().enumerate() {
          if let Some(value) = slot {
              if !f(index, value) {
                  *slot = None;
                  self.count -= 1;
                  self.empty.push(index);
              }
          }
      }
  }

  /// Returns an immutable reference to the element corresponding to the key.
  pub fn get(&self, key: usize) -> Option<&T> {
    self.data.get(key).and_then(|slot| slot.as_ref())
  }

  /// Returns a mutable reference to the element corresponding to the key.
  pub fn get_mut(&mut self, key: usize) -> Option<&mut T> {
    self.data.get_mut(key).and_then(|slot| slot.as_mut())
  }

  /// Returns `true` if the map contains a value for the specified key.
  pub fn contains_key(&self, key: usize) -> bool {
    self.data.get(key).map_or(false, |slot| slot.is_some())
  }

  /// Returns the number of elements currently stored in the map.
  #[inline]
  pub fn len(&self) -> usize {
    self.count
  }

  /// Returns `true` if the map contains no elements.
  #[inline]
  pub fn is_empty(&self) -> bool {
    self.count == 0
  }

  /// Returns a vector containing all the keys currently associated with values in the map.
  pub fn keys(&self) -> Vec<usize> {
    self.data
    .iter()
    .enumerate()
    .filter_map(|(index, slot)| {
      if slot.is_some() {
        Some(index)
      } else {
        None
      }
    })
    .collect()
  }

  /// Returns an iterator visiting all key-value pairs in arbitrary order.
  pub fn iter(&self) -> impl Iterator<Item = (usize, &T)> {
    self.data
    .iter()
    .enumerate()
    .filter_map(|(index, slot)| slot.as_ref().map(|value| (index, value)))
  }

  /// Returns an iterator visiting all key-value pairs in arbitrary order,
  /// with mutable references to the values.
  pub fn iter_mut(&mut self) -> impl Iterator<Item = (usize, &mut T)> {
    self.data
    .iter_mut()
    .enumerate()
    .filter_map(|(index, slot)| slot.as_mut().map(|value| (index, value)))
  }

  /// Removes all elements from the map.
  ///
  /// The underlying allocated memory is dropped. If you want to keep the
  /// allocated memory, use [`retain`](Self::retain) with a predicate that always returns false.
  pub fn clear(&mut self) {
    self.data.clear();
    self.empty.clear();
    self.count = 0;
  }
}

/// An iterator that consumes a `UsizeMap` and yields key-value pairs.
#[derive(Debug)]
pub struct UsizeMapIntoIter<T> {
  inner: Enumerate<VecIntoIter<Option<T>>>,
}

impl<T> Iterator for UsizeMapIntoIter<T> {
  type Item = (usize, T);

  #[inline]
  fn next(&mut self) -> Option<Self::Item> {
    loop {
      match self.inner.next() {
        Some((index, Some(value))) => return Some((index, value)),
        Some((_, None)) => continue,
        None => return None,
      }
    }
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    let (_lower, upper) = self.inner.size_hint();
    (0, upper)
  }
}

impl<T> IntoIterator for UsizeMap<T> {
  type Item = (usize, T);
  type IntoIter = UsizeMapIntoIter<T>;

  #[inline]
  fn into_iter(self) -> Self::IntoIter {
    UsizeMapIntoIter {
      inner: self.data.into_iter().enumerate(),
    }
  }
}

impl<T> Index<usize> for UsizeMap<T> {
  type Output = T;

  #[inline]
  fn index(&self, key: usize) -> &Self::Output {
    match self.data.get(key) {
      Some(Some(value)) => value,
      Some(None) => panic!("UsizeMap: Index {} points to an empty slot", key),
      None => panic!("UsizeMap: Index {} out of bounds (capacity is {})", key, self.data.len()),
    }
  }
}

impl<T> IndexMut<usize> for UsizeMap<T> {
  #[inline]
  fn index_mut(&mut self, key: usize) -> &mut Self::Output {
    let capacity = self.data.len();
    match self.data.get_mut(key) {
      Some(Some(value)) => value,
      Some(None) => panic!("UsizeMap: Index {} points to an empty slot for mutable access", key),
      None => panic!("UsizeMap: Index {} out of bounds for mutable access (capacity is {})", key, capacity),
    }
  }
}

impl<T: Debug> Debug for UsizeMap<T> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_map().entries(self.iter()).finish()
  }
}

impl<T> Default for UsizeMap<T> {
  fn default() -> Self {
    Self::new()
  }
}

// --- Tests ---
#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_new_empty_len() {
    let map: UsizeMap<i32> = UsizeMap::new();
    assert!(map.is_empty());
    assert_eq!(map.len(), 0);
    assert_eq!(map.count, 0);
    assert_eq!(map.capacity(), 0);
  }

  #[test]
  fn test_with_capacity() {
    let map: UsizeMap<f64> = UsizeMap::with_capacity(10);
    assert!(map.is_empty());
    assert_eq!(map.len(), 0);
    assert!(map.capacity() >= 10);
  }

  #[test]
  fn test_insert_and_get() {
    let mut map = UsizeMap::new();
    let k1 = map.insert("hello");
    let k2 = map.insert("world");

    assert_eq!(map.len(), 2);
    assert_eq!(map.count, 2);
    assert!(!map.is_empty());

    assert_eq!(k1, 0);
    assert_eq!(k2, 1);

    assert_eq!(map.get(k1), Some(&"hello"));
    assert_eq!(map.get(k2), Some(&"world"));
    assert_eq!(map.get(2), None);

    assert_eq!(map[k1], "hello");
    assert_eq!(map[k2], "world");
  }

  #[test]
  #[should_panic(expected = "UsizeMap: Index 0 out of bounds (capacity is 0)")]
  fn test_index_panic_oob() {
    let map: UsizeMap<i32> = UsizeMap::new();
    let _ = map[0];
  }

  #[test]
  fn test_remove() {
    let mut map = UsizeMap::new();
    let k1 = map.insert(10);
    let k2 = map.insert(20);
    let k3 = map.insert(30);

    assert_eq!(map.len(), 3);
    assert_eq!(map.remove(k2), Some(20));
    assert_eq!(map.len(), 2);
    assert_eq!(map.count, 2);
    assert_eq!(map.get(k2), None);
    assert_eq!(map.remove(k2), None);
    assert_eq!(map.len(), 2);

    assert_eq!(map.remove(k1), Some(10));
    assert_eq!(map.len(), 1);

    assert_eq!(map.remove(99), None);
    assert_eq!(map.len(), 1);

    assert_eq!(map.remove(k3), Some(30));
    assert_eq!(map.len(), 0);
    assert!(map.is_empty());
  }

  #[test]
  #[should_panic(expected = "UsizeMap: Index 0 points to an empty slot")]
  fn test_index_panic_empty_slot() {
    let mut map = UsizeMap::new();
    let k1 = map.insert(10);
    map.remove(k1);
    let _ = map[k1];
  }

  #[test]
  fn test_reuse_keys() {
    let mut map = UsizeMap::new();
    let k0 = map.insert("a");
    let k1 = map.insert("b");
    let _ = map.insert("c");

    assert_eq!(map.remove(k1), Some("b"));
    assert_eq!(map.len(), 2);
    assert_eq!(map.empty.len(), 1);
    assert!(map.empty.contains(&k1));

    let k3 = map.insert("d");
    assert_eq!(k3, k1);
    assert_eq!(map.len(), 3);
    assert_eq!(map.get(k3), Some(&"d"));
    assert_eq!(map.empty.len(), 0);

    assert_eq!(map.remove(k0), Some("a"));
    let k4 = map.insert("e");
    assert_eq!(k4, k0);
  }

  #[test]
  fn test_reserve_logic() {
      let mut map: UsizeMap<i32> = UsizeMap::new();

      // Case 1: Standard reserve
      map.reserve(10);
      assert!(map.capacity() >= 10);

      // Fill it up
      for i in 0..10 { map.insert(i); }

      // Case 2: Reserve with available recycled slots
      // Remove 5 items. They go into `empty`.
      for i in 0..5 { map.remove(i); }

      let old_cap = map.capacity();

      // We have 5 slots in `empty`.
      // Asking for 3 more slots should NOT grow the vector,
      // because we can satisfy 3 inserts from the 5 empty slots.
      map.reserve(3);
      assert_eq!(map.capacity(), old_cap);

      // Asking for 10 more slots should grow it,
      // but only by roughly (10 - 5) = 5.
      map.reserve(10);
      assert!(map.capacity() >= old_cap + 5);
  }

  #[test]
  fn test_retain() {
      let mut map = UsizeMap::new();
      let k1 = map.insert(10);
      let k2 = map.insert(20);
      let k3 = map.insert(30);

      // Keep only values > 15
      map.retain(|_k, v| *v > 15);

      assert_eq!(map.len(), 2);
      assert_eq!(map.get(k1), None);
      assert_eq!(map.get(k2), Some(&20));
      assert_eq!(map.get(k3), Some(&30));

      // Ensure reusable slot was recycled
      let k4 = map.insert(40);
      assert_eq!(k4, k1);
  }

  #[test]
  fn test_shrink_to_fit() {
      let mut map = UsizeMap::new();
      // Push capacity up
      for i in 0..100 { map.insert(i); }
      let big_cap = map.capacity();
      assert!(big_cap >= 100);

      map.clear();

      // Vector doesn't auto-shrink on clear
      // (Implementation dependent, but usually true for standard Vec)

      map.shrink_to_fit();
      // Should now be much smaller
      assert!(map.capacity() < big_cap);
  }
}
