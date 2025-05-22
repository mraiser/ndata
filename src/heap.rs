extern crate alloc;
use crate::usizemap::*; // Assuming UsizeMap is defined elsewhere in the crate

// Only use alloc::vec::Vec when the no_std_support feature is enabled
#[cfg(feature="no_std_support")]
use alloc::vec::Vec;
// If std is available (default), Vec is used from the standard library prelude

// Internal struct to hold the data and its reference count.
// Not public, as it's an implementation detail of Heap.
#[derive(Debug)]
struct Blob<T> {
  data: T,
  count: usize,
}

/// A reference counting container for objects of a given type with basic
/// garbage collection based on reference counts reaching zero.
///
/// Keys are `usize` indices returned by `push`.
#[derive(Debug)]
pub struct Heap<T> {
  data: UsizeMap<Blob<T>>,
}

// Implementation requires T to be Debug because Heap itself derives Debug.
// If Heap didn't need to be Debug, this bound could potentially be relaxed
// or moved only to methods/functions that specifically require it.
impl<T: core::fmt::Debug> Heap<T> {
  /// Creates a new, empty `Heap`.
  ///
  /// # Examples
  ///
  /// ```
  /// // Assuming Heap is accessible
  /// // let mut heap: Heap<i32> = Heap::new();
  /// ```
  #[inline]
  pub fn new() -> Heap<T> {
    Heap {
      data: UsizeMap::<Blob<T>>::new(),
    }
  }

  /// Pushes a value onto the heap, returning a stable `usize` key.
  ///
  /// The initial reference count for the pushed value is set to 1.
  ///
  /// # Arguments
  ///
  /// * `data`: The value of type `T` to store on the heap.
  ///
  /// # Returns
  ///
  /// A `usize` key that can be used to access, increment, or decrement
  /// the reference count of the stored value.
  ///
  /// # Examples
  ///
  /// ```
  /// // Assuming Heap is accessible
  /// // let mut heap = Heap::new();
  /// // let key = heap.push("hello");
  /// // assert_eq!(heap.count(key), 1);
  /// ```
  pub fn push(&mut self, data: T) -> usize {
    let blob = Blob {
      data: data,
      count: 1, // Start with a reference count of 1
    };
    self.data.insert(blob)
  }

  /// Returns a mutable reference to the value associated with the given key.
  ///
  /// # Arguments
  ///
  /// * `index`: The `usize` key obtained from `push`.
  ///
  /// # Returns
  ///
  /// A mutable reference `&mut T` to the stored value.
  ///
  /// # Panics
  ///
  /// Panics if the `index` is not a valid key currently present in the heap.
  /// For a non-panicking version, see [`try_get`](#method.try_get).
  ///
  /// ```
  /// // Assuming Heap is accessible
  /// // let mut heap = Heap::new();
  /// // let key = heap.push(10);
  /// // *heap.get(key) += 5;
  /// // assert_eq!(*heap.get(key), 15);
  /// ```
  pub fn get(&mut self, index: usize) -> &mut T {
    // Use expect for a slightly more informative panic message than unwrap
    &mut self.data.get_mut(index).expect("Heap::get: Invalid index").data
  }

  /// Returns a mutable reference to the value associated with the key, if it exists.
  ///
  /// # Arguments
  ///
  /// * `index`: The `usize` key obtained from `push`.
  ///
  /// # Returns
  ///
  /// * `Some(&mut T)` if the key is valid and present.
  /// * `None` if the key is not valid or the data is no longer present.
  ///
  /// ```
  /// // Assuming Heap is accessible
  /// // let mut heap = Heap::new();
  /// // let key = heap.push(10);
  /// // if let Some(value) = heap.try_get(key) {
  /// //     *value += 5;
  /// // }
  /// // assert!(heap.try_get(999).is_none()); // Assuming 999 is not a valid key
  /// ```
  pub fn try_get(&mut self, index: usize) -> Option<&mut T> {
    // Use map for a more idiomatic way to transform Option<Blob<T>> to Option<T>
    self.data.get_mut(index).map(|blob| &mut blob.data)
  }

  /// Returns the current reference count for the value associated with the key.
  ///
  /// Note: This method requires `&mut self` for consistency with the original crate's
  /// signature and potential internal implementation details of `UsizeMap`.
  ///
  /// # Arguments
  ///
  /// * `index`: The `usize` key obtained from `push`.
  ///
  /// # Returns
  ///
  /// The current reference count (`usize`).
  ///
  /// # Panics
  ///
  /// Panics if the `index` is not a valid key currently present in the heap.
  ///
  /// ```
  /// // Assuming Heap is accessible
  /// // let mut heap = Heap::new();
  /// // let key = heap.push("data");
  /// // assert_eq!(heap.count(key), 1);
  /// // heap.incr(key);
  /// // assert_eq!(heap.count(key), 2);
  /// ```
  pub fn count(&mut self, index: usize) -> usize {
    // Use get_mut().expect() for consistency with get() and decr() panicking behavior.
    // Retain &mut self signature for backward compatibility.
    self.data.get_mut(index).expect("Heap::count: Invalid index").count
  }

  /// Increments the reference count for the value associated with the key.
  ///
  /// # Arguments
  ///
  /// * `index`: The `usize` key obtained from `push`.
  ///
  /// # Panics
  ///
  /// Panics if the `index` is not a valid key currently present in the heap.
  ///
  /// ```
  /// // Assuming Heap is accessible
  /// // let mut heap = Heap::new();
  /// // let key = heap.push(10);
  /// // assert_eq!(heap.count(key), 1);
  /// // heap.incr(key);
  /// // assert_eq!(heap.count(key), 2);
  /// ```
  pub fn incr(&mut self, index: usize) {
    self.data.get_mut(index).expect("Heap::incr: Invalid index").count += 1;
  }

  /// Decrements the reference count for the value associated with the key.
  ///
  /// If the reference count reaches zero after decrementing, the value is
  /// removed from the heap (garbage collected).
  ///
  /// # Arguments
  ///
  /// * `index`: The `usize` key obtained from `push`.
  ///
  /// # Panics
  ///
  /// Panics if the `index` is not a valid key currently present in the heap.
  /// It's the caller's responsibility to ensure `decr` is not called more times
  /// than `incr` plus the initial `push`.
  ///
  /// ```
  /// // Assuming Heap is accessible
  /// // let mut heap = Heap::new();
  /// // let key = heap.push(10); // count = 1
  /// // heap.incr(key);          // count = 2
  /// // heap.decr(key);          // count = 1
  /// // assert!(heap.try_get(key).is_some());
  /// // heap.decr(key);          // count = 0, value removed
  /// // assert!(heap.try_get(key).is_none());
  /// ```
  pub fn decr(&mut self, index: usize) {
    // Get mutable reference, panic if index is invalid
    let blob = self.data.get_mut(index).expect("Heap::decr: Invalid index");

    // Check the count *before* decrementing
    if blob.count == 1 {
      // If count is 1, decrementing makes it 0, so remove the data.
      self.data.remove(index);
    } else {
      // Otherwise, just decrement the count.
      // This check prevents underflow, assuming counts are managed correctly externally.
      blob.count -= 1;
    }
  }

  /// Returns a vector containing all the keys currently present in the heap.
  ///
  /// The order of keys is not guaranteed.
  ///
  /// # Returns
  ///
  /// A `Vec<usize>` of the keys.
  ///
  /// ```
  /// // Assuming Heap is accessible
  /// // let mut heap = Heap::new();
  /// // let key1 = heap.push(1);
  /// // let key2 = heap.push(2);
  /// // let keys = heap.keys();
  /// // assert_eq!(keys.len(), 2);
  /// // assert!(keys.contains(&key1));
  /// // assert!(keys.contains(&key2));
  /// ```
  pub fn keys(&self) -> Vec<usize> {
    // Assuming UsizeMap provides a keys() method returning Vec<usize>
    self.data.keys()
  }
}

// Implement Default trait for Heap<T>
impl<T: core::fmt::Debug> Default for Heap<T> {
  /// Creates a new, empty `Heap` using the default trait.
  /// Equivalent to `Heap::new()`.
  fn default() -> Self {
    Self::new()
  }
}
