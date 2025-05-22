#![cfg_attr(feature = "no_std_support", no_std)] // Indicate no_std support at crate level if applicable

// Use alloc crate only when no_std_support feature is enabled
#[cfg(feature = "no_std_support")]
extern crate alloc;

// --- Conditional Imports for Vec and VecIntoIter ---

// Use alloc's Vec and IntoIter when 'no_std_support' is enabled
#[cfg(feature = "no_std_support")]
use alloc::vec::{Vec, IntoIter as VecIntoIter};

// Use std's Vec and IntoIter when 'no_std_support' is NOT enabled (implying std is available)
#[cfg(not(feature = "no_std_support"))]
use std::vec::{Vec, IntoIter as VecIntoIter};

// --- Other Core/Standard Imports ---
use core::fmt::{self, Debug};
//use core::mem;
use core::ops::{Index, IndexMut};
use core::iter::Enumerate; // Needed for the iterator struct


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
  ///
  /// # Examples
  ///
  /// ```
  /// // Use the actual crate name 'ndata' here
  /// use ndata::UsizeMap;
  /// let mut map: UsizeMap<String> = UsizeMap::new();
  /// assert_eq!(map.len(), 0);
  /// ```
  pub fn new() -> Self {
    UsizeMap {
      data: Vec::new(),
      empty: Vec::new(),
      count: 0,
    }
  }

  /// Creates a new, empty `UsizeMap` with a specified initial capacity.
  ///
  /// The map will be able to hold at least `capacity` elements without
  /// reallocating the underlying data vector.
  ///
  /// # Examples
  ///
  /// ```
  /// // Use the actual crate name 'ndata' here
  /// use ndata::UsizeMap;
  /// let mut map: UsizeMap<i32> = UsizeMap::with_capacity(10);
  /// assert_eq!(map.len(), 0);
  /// assert!(map.capacity() >= 10);
  /// ```
  pub fn with_capacity(capacity: usize) -> Self {
    UsizeMap {
      data: Vec::with_capacity(capacity),
      empty: Vec::new(), // Typically start empty list small
      count: 0,
    }
  }

  /// Returns the number of elements the map can hold without reallocating.
  /// This is the capacity of the underlying `Vec<Option<T>>`.
  ///
  /// # Examples
  ///
  /// ```
  /// // Use the actual crate name 'ndata' here
  /// use ndata::UsizeMap;
  /// let map: UsizeMap<i32> = UsizeMap::with_capacity(5);
  /// assert!(map.capacity() >= 5);
  /// ```
  pub fn capacity(&self) -> usize {
    self.data.capacity()
  }

  /// Inserts an element into the map, returning the `usize` key assigned to it.
  ///
  /// If there are previously removed slots, one will be reused. Otherwise,
  /// the element is appended to the underlying storage.
  ///
  /// # Examples
  ///
  /// ```
  /// // Use the actual crate name 'ndata' here
  /// use ndata::UsizeMap;
  /// let mut map = UsizeMap::new();
  /// let key1 = map.insert("a");
  /// let key2 = map.insert("b");
  /// assert_eq!(key1, 0);
  /// assert_eq!(key2, 1);
  /// assert_eq!(map.get(key1), Some(&"a"));
  /// ```
  pub fn insert(&mut self, element: T) -> usize {
    self.count += 1;
    if let Some(index) = self.empty.pop() {
      // Reuse an empty slot - O(1)
      // Ensure the slot is actually empty before reusing (sanity check)
      if index < self.data.len() {
        assert!(self.data[index].is_none(), "UsizeMap logic error: Empty slot index {} pointed to a non-empty slot!", index);
        self.data[index] = Some(element);
      } else {
        // This case should ideally not happen if logic is correct,
        // but handle defensively: extend data if index is out of bounds.
        // This might indicate a bug elsewhere if it occurs.
        self.data.resize_with(index + 1, || None);
        self.data[index] = Some(element);
      }
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
  /// Returns `None` if the key is invalid (out of bounds) or the slot was already empty.
  ///
  /// # Examples
  ///
  /// ```
  /// // Use the actual crate name 'ndata' here
  /// use ndata::UsizeMap;
  /// let mut map = UsizeMap::new();
  /// let key = map.insert(100);
  /// assert_eq!(map.remove(key), Some(100));
  /// assert_eq!(map.remove(key), None); // Already removed
  /// assert_eq!(map.len(), 0);
  /// assert_eq!(map.remove(999), None); // Out of bounds or invalid key
  /// ```
  pub fn remove(&mut self, key: usize) -> Option<T> {
    // Use `get_mut` to safely handle bounds checks and check for Some/None.
    // `take()` replaces `Some(T)` with `None` and returns `Some(T)`, or returns `None` if it was already `None`.
    match self.data.get_mut(key).and_then(|slot| slot.take()) {
      Some(value) => {
        // Only if a value was actually removed, decrement count and add key to empty list
        self.count -= 1;
        self.empty.push(key);
        Some(value)
      }
      None => {
        // Key was out of bounds or the slot was already None
        None
      }
    }
  }

  /// Returns an immutable reference to the element corresponding to the key.
  ///
  /// Returns `None` if the key is invalid or the slot is empty.
  ///
  /// # Examples
  ///
  /// ```
  /// // Use the actual crate name 'ndata' here
  /// use ndata::UsizeMap;
  /// let mut map = UsizeMap::new();
  /// let key = map.insert(true);
  /// assert_eq!(map.get(key), Some(&true));
  /// assert_eq!(map.get(key + 1), None);
  /// map.remove(key);
  /// assert_eq!(map.get(key), None);
  /// ```
  pub fn get(&self, key: usize) -> Option<&T> {
    // get returns Option<&Option<T>>, and_then unwraps the outer Option,
    // as_ref converts Option<T> to Option<&T>.
    self.data.get(key).and_then(|slot| slot.as_ref())
  }

  /// Returns a mutable reference to the element corresponding to the key.
  ///
  /// Returns `None` if the key is invalid or the slot is empty.
  ///
  /// # Examples
  ///
  /// ```
  /// // Use the actual crate name 'ndata' here
  /// use ndata::UsizeMap;
  /// let mut map = UsizeMap::new();
  /// let key = map.insert(String::from("hello"));
  /// if let Some(value) = map.get_mut(key) {
  ///     *value = String::from("world");
  /// }
  /// assert_eq!(map.get(key), Some(&String::from("world")));
  /// assert_eq!(map.get_mut(key + 1), None);
  /// map.remove(key);
  /// assert_eq!(map.get_mut(key), None);
  /// ```
  pub fn get_mut(&mut self, key: usize) -> Option<&mut T> {
    // get_mut returns Option<&mut Option<T>>, and_then unwraps the outer Option,
    // as_mut converts Option<T> to Option<&mut T>.
    self.data.get_mut(key).and_then(|slot| slot.as_mut())
  }

  /// Returns `true` if the map contains a value for the specified key.
  ///
  /// A key is considered valid only if it's within the bounds of the underlying
  /// data vector *and* the corresponding slot contains `Some(T)`.
  ///
  /// # Examples
  ///
  /// ```
  /// // Use the actual crate name 'ndata' here
  /// use ndata::UsizeMap;
  /// let mut map = UsizeMap::new();
  /// let key = map.insert(1);
  /// assert!(map.contains_key(key));
  /// assert!(!map.contains_key(key + 1)); // Out of bounds
  /// map.remove(key);
  /// assert!(!map.contains_key(key)); // Slot is now None
  /// ```
  pub fn contains_key(&self, key: usize) -> bool {
    // Check bounds AND ensure the slot is not None
    self.data.get(key).map_or(false, |slot| slot.is_some())
    // Equivalent to: self.get(key).is_some()
  }


  /// Returns the number of elements currently stored in the map.
  ///
  /// # Examples
  ///
  /// ```
  /// // Use the actual crate name 'ndata' here
  /// use ndata::UsizeMap;
  /// let mut map = UsizeMap::new();
  /// map.insert(1);
  /// map.insert(2);
  /// assert_eq!(map.len(), 2);
  /// map.remove(0);
  /// assert_eq!(map.len(), 1);
  /// ```
  #[inline]
  pub fn len(&self) -> usize {
    self.count // Directly track the count for O(1) length check
  }

  /// Returns `true` if the map contains no elements.
  ///
  /// # Examples
  ///
  /// ```
  /// // Use the actual crate name 'ndata' here
  /// use ndata::UsizeMap;
  /// let mut map: UsizeMap<i32> = UsizeMap::new();
  /// assert!(map.is_empty());
  /// map.insert(1);
  /// assert!(!map.is_empty());
  /// ```
  #[inline]
  pub fn is_empty(&self) -> bool {
    self.count == 0
  }

  /// Returns a vector containing all the keys currently associated with values in the map.
  /// The order of keys is not guaranteed.
  ///
  /// # Examples
  ///
  /// ```
  /// // Use the actual crate name 'ndata' here
  /// use ndata::UsizeMap;
  /// let mut map = UsizeMap::new();
  /// let k0 = map.insert("a");
  /// let k1 = map.insert("b");
  /// map.remove(k0);
  /// let k2 = map.insert("c"); // might reuse k0
  /// let mut keys = map.keys();
  /// keys.sort(); // Sort for predictable test output
  /// // keys will contain k1 and k2 (which might be 0). Order isn't guaranteed without sort.
  /// assert_eq!(keys.len(), 2);
  /// assert!(keys.contains(&k1));
  /// assert!(keys.contains(&k2));
  /// assert_eq!(keys, vec![k2, k1]); // k2 is likely 0, k1 is 1
  /// ```
  pub fn keys(&self) -> Vec<usize> {
    // Iterate through data, collecting indices of Some elements - O(N) where N = capacity
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
  /// The iterator element type is `(usize, &'a T)`.
  ///
  /// # Examples
  ///
  /// ```
  /// // Use the actual crate name 'ndata' here
  /// use ndata::UsizeMap;
  /// let mut map = UsizeMap::new();
  /// let k0 = map.insert("a");
  /// let k1 = map.insert("b");
  ///
  /// let mut count = 0;
  /// for (key, value) in map.iter() {
  ///     println!("Key: {}, Value: {}", key, value);
  ///     count += 1;
  /// }
  /// assert_eq!(count, 2); // Make sure we iterated
  ///
  /// // Example check: Collect into a sorted vector for predictable testing
  /// let mut items: Vec<_> = map.iter().collect();
  /// items.sort_by_key(|&(k, _)| k); // Sort by key
  /// assert_eq!(items, vec![(k0, &"a"), (k1, &"b")]);
  /// ```
  pub fn iter(&self) -> impl Iterator<Item = (usize, &T)> {
    self.data
    .iter()
    .enumerate()
    .filter_map(|(index, slot)| slot.as_ref().map(|value| (index, value)))
  }

  /// Returns an iterator visiting all key-value pairs in arbitrary order,
  /// with mutable references to the values.
  /// The iterator element type is `(usize, &'a mut T)`.
  ///
  /// # Examples
  ///
  /// ```
  /// // Use the actual crate name 'ndata' here
  /// use ndata::UsizeMap;
  /// let mut map = UsizeMap::new();
  /// let k0 = map.insert(10);
  /// let k1 = map.insert(20);
  ///
  /// for (_key, value) in map.iter_mut() {
  ///     *value *= 2;
  /// }
  ///
  /// assert_eq!(map.get(k0), Some(&20));
  /// assert_eq!(map.get(k1), Some(&40));
  /// ```
  pub fn iter_mut(&mut self) -> impl Iterator<Item = (usize, &mut T)> {
    self.data
    .iter_mut()
    .enumerate()
    .filter_map(|(index, slot)| slot.as_mut().map(|value| (index, value)))
  }

  /// Removes all elements from the map.
  ///
  /// The underlying allocated memory is dropped. If you want to keep the
  /// allocated memory, use [`retain`] or manually remove elements.
  /// After `clear`, `len()` will be 0. `capacity()` behavior depends on
  /// the allocator but is often 0.
  /// All previously valid keys become invalid.
  ///
  /// [`retain`]: #method.retain (If you implement retain later)
  ///
  /// # Examples
  ///
  /// ```
  /// // Use the actual crate name 'ndata' here
  /// use ndata::UsizeMap;
  /// let mut map = UsizeMap::new();
  /// map.insert(1);
  /// map.insert(2);
  /// map.clear();
  /// assert!(map.is_empty());
  /// assert_eq!(map.len(), 0);
  /// // Capacity is not guaranteed to be 0 after clear, so we don't test it.
  /// // assert_eq!(map.capacity(), 0); // <-- REMOVED
  /// ```
  pub fn clear(&mut self) {
    self.data.clear(); // Clears the vec and drops capacity
    self.empty.clear(); // Clear the list of reusable keys
    self.count = 0;     // Reset the count
  }

}

/// An iterator that consumes a `UsizeMap` and yields key-value pairs.
///
/// This struct is created by the `into_iter` method on [`UsizeMap`].
#[derive(Debug)]
pub struct UsizeMapIntoIter<T> {
  // Store the inner iterator state: enumerating the Vec's consuming iterator
  inner: Enumerate<VecIntoIter<Option<T>>>,
}

impl<T> Iterator for UsizeMapIntoIter<T> {
  type Item = (usize, T); // The type of elements yielded

  #[inline]
  fn next(&mut self) -> Option<Self::Item> {
    // Loop through the inner iterator (index, Option<T>)
    // Use loop and next() manually to ensure we skip None values correctly
    loop {
      match self.inner.next() {
        Some((index, Some(value))) => return Some((index, value)), // Found a value
        Some((_, None)) => continue, // Skip None slots
        None => return None, // Inner iterator is exhausted
      }
    }
  }

  // Optional: Implement size_hint if possible.
  fn size_hint(&self) -> (usize, Option<usize>) {
    // The exact number of remaining items is self.inner.len() if the inner iterator
    // provides an accurate size hint, but the number of *Some* items is unknown.
    let (_lower, upper) = self.inner.size_hint();
    // We know at least 0 Some items remain, and at most 'upper' items remain in total.
    (0, upper)
  }
}


/// Creates an iterator that takes ownership of the `UsizeMap` and yields
/// key-value pairs (`(usize, T)`). The order is arbitrary.
impl<T> IntoIterator for UsizeMap<T> {
  type Item = (usize, T);
  // Use the concrete struct type instead of `impl Trait`
  type IntoIter = UsizeMapIntoIter<T>;

  #[inline]
  fn into_iter(self) -> Self::IntoIter {
    // Create and return an instance of the iterator struct
    UsizeMapIntoIter {
      inner: self.data.into_iter().enumerate(),
    }
  }
}


/// Implements immutable indexing (`map[key]`).
///
/// # Panics
///
/// Panics if `key` is out of bounds or if the slot corresponding to `key` is empty (`None`).
/// For non-panicking access, use [`get`](UsizeMap::get).
impl<T> Index<usize> for UsizeMap<T> {
  type Output = T;

  #[inline]
  fn index(&self, key: usize) -> &Self::Output {
    // Provide clearer panic messages
    match self.data.get(key) {
      Some(Some(value)) => value,
      Some(None) => panic!("UsizeMap: Index {} points to an empty slot", key),
      None => panic!("UsizeMap: Index {} out of bounds (capacity is {})", key, self.data.len()),
    }
  }
}

/// Implements mutable indexing (`map[key] = value`).
///
/// # Panics
///
/// Panics if `key` is out of bounds or if the slot corresponding to `key` is empty (`None`).
/// For non-panicking mutable access, use [`get_mut`](UsizeMap::get_mut).
impl<T> IndexMut<usize> for UsizeMap<T> {
  #[inline]
  fn index_mut(&mut self, key: usize) -> &mut Self::Output {
    let capacity = self.data.len();
    // Provide clearer panic messages
    match self.data.get_mut(key) {
      Some(Some(value)) => value,
      Some(None) => panic!("UsizeMap: Index {} points to an empty slot for mutable access", key),
      None => panic!("UsizeMap: Index {} out of bounds for mutable access (capacity is {})", key, capacity),
    }
  }
}


/// Implements the `Debug` trait for `UsizeMap`.
/// Only requires `T` to implement `Debug`.
impl<T: Debug> Debug for UsizeMap<T> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Use debug_map for a format similar to HashMap
    f.debug_map().entries(self.iter()).finish()
  }
}

/// Implements the `Default` trait for `UsizeMap`.
impl<T> Default for UsizeMap<T> {
  /// Creates an empty `UsizeMap<T>`. Equivalent to `UsizeMap::new()`.
  fn default() -> Self {
    Self::new()
  }
}

// --- Tests ---
#[cfg(test)]
mod tests {
  // Use super to access UsizeMap if tests are in a submodule
  // If usizemap.rs is at the crate root (like src/usizemap.rs),
  // then using crate::UsizeMap might be needed inside tests if `super` doesn't work.
  // However, `use super::*;` is standard for tests in a submodule.
  use super::*;

  #[test]
  fn test_new_empty_len() {
    let map: UsizeMap<i32> = UsizeMap::new();
    assert!(map.is_empty());
    assert_eq!(map.len(), 0);
    assert_eq!(map.count, 0);
    assert_eq!(map.capacity(), 0); // Initial capacity is 0
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
    assert_eq!(map.get(2), None); // Out of bounds

    // Test Index trait
    assert_eq!(map[k1], "hello");
    assert_eq!(map[k2], "world");
  }

  #[test]
  #[should_panic(expected = "UsizeMap: Index 0 out of bounds (capacity is 0)")]
  fn test_index_panic_oob() {
    let map: UsizeMap<i32> = UsizeMap::new();
    let _ = map[0]; // Out of bounds
  }

  #[test]
  #[should_panic(expected = "UsizeMap: Index 1 out of bounds (capacity is 1)")]
  fn test_index_panic_oob_after_insert() {
    let mut map: UsizeMap<i32> = UsizeMap::new();
    map.insert(10); // Capacity becomes 1
    let _ = map[1]; // Out of bounds
  }


  #[test]
  fn test_remove() {
    let mut map = UsizeMap::new();
    let k1 = map.insert(10);
    let k2 = map.insert(20);
    let k3 = map.insert(30);

    assert_eq!(map.len(), 3);
    assert_eq!(map.remove(k2), Some(20)); // Remove middle
    assert_eq!(map.len(), 2);
    assert_eq!(map.count, 2);
    assert_eq!(map.get(k2), None); // k2 is now empty
    assert_eq!(map.remove(k2), None); // Remove already removed
    assert_eq!(map.len(), 2); // Length unchanged

    assert_eq!(map.remove(k1), Some(10));
    assert_eq!(map.len(), 1);
    assert_eq!(map.get(k1), None);

    assert_eq!(map.remove(99), None); // Remove out of bounds
    assert_eq!(map.len(), 1);

    assert_eq!(map.remove(k3), Some(30));
    assert_eq!(map.len(), 0);
    assert!(map.is_empty());
    assert_eq!(map.count, 0);
  }

  #[test]
  #[should_panic(expected = "UsizeMap: Index 0 points to an empty slot")]
  fn test_index_panic_empty_slot() {
    let mut map = UsizeMap::new();
    let k1 = map.insert(10);
    map.remove(k1); // Slot 0 is now empty
    let _ = map[k1]; // Access removed slot via index
  }

  #[test]
  fn test_reuse_keys() {
    let mut map = UsizeMap::new();
    let k0 = map.insert("a"); // 0
    let k1 = map.insert("b"); // 1
    let k2 = map.insert("c"); // 2

    assert_eq!(map.remove(k1), Some("b")); // Remove index 1
    assert_eq!(map.len(), 2);
    assert_eq!(map.empty.len(), 1); // key 1 is reusable
    assert!(map.empty.contains(&k1));


    let k3 = map.insert("d"); // Should reuse index 1
    assert_eq!(k3, k1); // Check if key 1 was reused
    assert_eq!(map.len(), 3);
    assert_eq!(map.get(k3), Some(&"d"));
    assert_eq!(map.get(k1), Some(&"d")); // k1 and k3 are the same index
    assert_eq!(map.empty.len(), 0); // No more empty slots available via list

    assert_eq!(map.remove(k0), Some("a")); // Remove index 0
    let k4 = map.insert("e"); // Should reuse index 0
    assert_eq!(k4, k0);
    assert_eq!(map.get(k0), Some(&"e"));
  }


  #[test]
  fn test_get_mut_and_index_mut() {
    let mut map = UsizeMap::new();
    let k1 = map.insert(100);
    let k2 = map.insert(200);

    // Via get_mut
    if let Some(val) = map.get_mut(k1) {
      *val += 1;
    }
    assert_eq!(map.get(k1), Some(&101));

    // Via IndexMut
    map[k2] += 5;
    assert_eq!(map.get(k2), Some(&205));

    assert!(map.get_mut(99).is_none());
  }

  #[test]
  #[should_panic(expected = "UsizeMap: Index 0 points to an empty slot for mutable access")]
  fn test_index_mut_panic_empty() {
    let mut map = UsizeMap::new();
    let k1 = map.insert(1);
    map.remove(k1);
    map[k1] = 2; // Panic!
  }

  #[test]
  fn test_keys() {
    let mut map = UsizeMap::new();
    let k0 = map.insert("a");
    let k1 = map.insert("b");
    let k2 = map.insert("c");
    map.remove(k1); // Remove middle one

    let mut keys = map.keys();
    keys.sort(); // Sort for predictable comparison

    assert_eq!(keys, vec![k0, k2]); // Should contain 0 and 2
  }

  #[test]
  fn test_iter() {
    let mut map = UsizeMap::new();
    let k0 = map.insert("a");
    let k1 = map.insert("b");
    map.remove(k0);
    let k2 = map.insert("c"); // reuses k0

    let mut items: Vec<_> = map.iter().collect();
    // Sort by key for predictable order in test
    items.sort_by_key(|&(key, _)| key);

    // Expect (0, "c") and (1, "b")
    assert_eq!(items.len(), 2);
    assert_eq!(items[0], (k2, &"c")); // k2 reused k0, so key is 0
    assert_eq!(items[1], (k1, &"b")); // k1 is 1
  }

  #[test]
  fn test_iter_mut() {
    let mut map = UsizeMap::new();
    let k0 = map.insert(1);
    let k1 = map.insert(10);

    for (key, val) in map.iter_mut() {
      if key == k0 {
        *val *= 5;
      } else {
        *val += 1;
      }
    }

    assert_eq!(map[k0], 5);
    assert_eq!(map[k1], 11);
  }

  #[test]
  fn test_into_iter() {
    let mut map = UsizeMap::new();
    let k0 = map.insert("a".to_string()); // owned value
    let k1 = map.insert("b".to_string());
    map.remove(k0);
    let k2 = map.insert("c".to_string()); // reuses k0

    let mut items: Vec<_> = map.into_iter().collect();
    items.sort_by_key(|&(key, _)| key);

    assert_eq!(items.len(), 2);
    // items now owns the Strings
    assert_eq!(items[0], (k2, "c".to_string())); // k2 reused k0 (index 0)
    assert_eq!(items[1], (k1, "b".to_string())); // k1 is index 1
  }

  #[test]
  fn test_into_iter_size_hint() {
    let mut map = UsizeMap::new();
    map.insert("a");
    map.insert("b");
    map.insert("c");
    map.remove(1); // remove "b"

    let iter = map.into_iter();
    // We removed one element, capacity is 3.
    // Lower bound is 0 (could remove all others before iterating).
    // Upper bound is 3 (max possible elements left in underlying vec iter).
    assert_eq!(iter.size_hint(), (0, Some(3)));
  }

  #[test]
  fn test_debug_format() {
    let mut map = UsizeMap::new();
    map.insert("hello");
    map.insert("world");

    let debug_str = format!("{:?}", map);
    // Default debug_map format is like {0: "hello", 1: "world"}
    // Order isn't strictly guaranteed by HashMap debug, but likely here.
    assert!(debug_str.contains("0: \"hello\""));
    assert!(debug_str.contains("1: \"world\""));
    assert!(debug_str.starts_with('{') && debug_str.ends_with('}'));

    map.remove(0);
    map.insert("reused"); // Should reuse key 0
    let debug_str_2 = format!("{:?}", map);
    assert!(debug_str_2.contains("0: \"reused\""));
    assert!(debug_str_2.contains("1: \"world\""));


    let empty_map: UsizeMap<i32> = UsizeMap::new();
    assert_eq!(format!("{:?}", empty_map), "{}");
  }

  #[test]
  fn test_contains_key() {
    let mut map = UsizeMap::new();
    let k0 = map.insert(0);
    assert!(map.contains_key(k0));
    assert!(!map.contains_key(k0 + 1)); // check out of bounds
    assert!(!map.contains_key(99));    // check way out of bounds

    map.remove(k0);
    assert!(!map.contains_key(k0)); // Key exists, but slot is None

    let k1 = map.insert(1); // Should reuse index 0
    assert_eq!(k1, k0);
    assert!(map.contains_key(k1));
    assert!(map.contains_key(k0));
  }

  #[test]
  fn test_clear() {
    let mut map = UsizeMap::new();
    map.insert(1);
    map.insert(2);
    let _cap_before = map.capacity(); // capacity might be >= 2
    let k0 = 0; // know a previously valid key

    map.clear();

    assert!(map.is_empty());
    assert_eq!(map.len(), 0);
    assert_eq!(map.count, 0);
    assert!(map.empty.is_empty());
    // Vec::clear doesn't guarantee capacity is zero, so we don't assert on it.
    assert!(!map.contains_key(k0));
    assert!(map.get(k0).is_none());

    // Test inserting after clear
    let k_after = map.insert(100);
    assert_eq!(k_after, 0); // Should start from 0 again
    assert_eq!(map.len(), 1);
    assert_eq!(map.get(k_after), Some(&100));
  }

  #[test]
  fn test_default() {
    let map: UsizeMap<u8> = Default::default();
    assert!(map.is_empty());
    assert_eq!(map.len(), 0);
  }
}
