#![cfg_attr(feature = "no_std_support", no_std)]

extern crate alloc;
use crate::usizemap::UsizeMap;

// Use alloc::vec::Vec when the no_std_support feature is enabled
#[cfg(feature="no_std_support")]
use alloc::vec::Vec;
// If std is available (default), Vec is used from the standard library prelude
#[cfg(not(feature="no_std_support"))]
use std::vec::Vec;

use core::fmt::{self, Debug};

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
pub struct Heap<T> {
    data: UsizeMap<Blob<T>>,
}

// Implement Debug manually to avoid requiring T: Debug for the struct definition,
// but require it for the Debug implementation.
impl<T: Debug> Debug for Heap<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Heap")
         .field("data", &self.data)
         .finish()
    }
}

impl<T> Heap<T> {
    /// Creates a new, empty `Heap`.
    #[inline]
    pub fn new() -> Heap<T> {
        Heap {
            data: UsizeMap::new(),
        }
    }

    /// Pushes a value onto the heap, returning a stable `usize` key.
    /// The initial reference count is 1.
    pub fn push(&mut self, data: T) -> usize {
        let blob = Blob {
            data,
            count: 1,
        };
        self.data.insert(blob)
    }

    /// Returns a mutable reference to the value associated with the given key.
    ///
    /// # Panics
    /// Panics if the `index` is not a valid key.
    pub fn get(&mut self, index: usize) -> &mut T {
        &mut self.data.get_mut(index).expect("Heap::get: Invalid index").data
    }

    /// Returns a mutable reference to the value associated with the key, if it exists.
    pub fn try_get(&mut self, index: usize) -> Option<&mut T> {
        self.data.get_mut(index).map(|blob| &mut blob.data)
    }

    /// Checks if the heap contains a value for the specified key.
    pub fn contains_key(&self, index: usize) -> bool {
        self.data.contains_key(index)
    }

    /// Returns the current reference count for the value associated with the key.
    /// Returns 0 if the key does not exist.
    pub fn count(&mut self, index: usize) -> usize {
        self.data.get_mut(index).map(|b| b.count).unwrap_or(0)
    }

    /// Increments the reference count for the value associated with the key.
    ///
    /// # Panics
    /// Panics if the `index` is not valid.
    pub fn incr(&mut self, index: usize) {
        if let Some(blob) = self.data.get_mut(index) {
            blob.count += 1;
        } else {
            panic!("Heap::incr: Invalid index {}", index);
        }
    }

    /// Decrements the reference count. If it reaches zero, the value is removed.
    ///
    /// # Panics
    /// Panics if the `index` is not valid.
    pub fn decr(&mut self, index: usize) {
        // Step 1: Check the count and decide if we need to remove.
        // We CANNOT call remove() inside this block because self.data is borrowed.
        let should_remove = if let Some(blob) = self.data.get_mut(index) {
            if blob.count > 1 {
                blob.count -= 1;
                false
            } else {
                // count is 1 (or 0, which shouldn't happen), so we remove it.
                true
            }
        } else {
            panic!("Heap::decr: Invalid index {}", index);
        };

        // Step 2: Perform removal if necessary (mutable borrow has ended)
        if should_remove {
            self.data.remove(index);
        }
    }

    /// Forcibly removes an item from the heap regardless of its reference count.
    /// Returns the data if it existed.
    ///
    /// Useful for error recovery or session cleanup.
    pub fn free(&mut self, index: usize) -> Option<T> {
        self.data.remove(index).map(|b| b.data)
    }

    /// Returns a vector containing all the keys currently present.
    pub fn keys(&self) -> Vec<usize> {
        self.data.keys()
    }

    /// Clears the heap, removing all values.
    pub fn clear(&mut self) {
        self.data.clear();
    }

    /// Shrinks the underlying memory storage to fit the current data.
    pub fn shrink_to_fit(&mut self) {
        self.data.shrink_to_fit();
    }
}

// Implement Default trait for Heap<T>
impl<T> Default for Heap<T> {
    fn default() -> Self {
        Self::new()
    }
}
