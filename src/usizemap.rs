use core::ops::Index;
use core::mem;
use core::fmt;
use core::fmt::Debug;

#[cfg(feature="no_std_support")]
use alloc::vec::Vec;

/// A map of type ```<usize, T>``` where the keys are generated and reused by the map.
pub struct UsizeMap<T> {
  /// The list of objects contained in this map
  data: Vec<Option<T>>,
  /// The list of empty (reusable) keys
  empty: Vec<usize>,
}

impl<T: Debug> UsizeMap<T> {
  /// Return a new (empty) ```UsizeMap```.
  pub fn new() -> UsizeMap<T> {
    UsizeMap {
      data: Vec::new(),
      empty: Vec::new(),
    }
  }
  
  /// Add an object to this map and return a key (```usize```) for it.
  pub fn insert(&mut self, t:T) -> usize {
    if self.empty.len() > 0 {
      let i = self.empty.remove(0);
      self.data[i] = Some(t);
      return i;
    }
    let i = self.data.len();
    self.data.push(Some(t));
    i
  }
  
  /// Return a mutable reference to the stored value with the given key.
  pub fn get_mut(&mut self, i:usize) -> Option<&mut T> {
    let x = self.data.get_mut(i);
    if x.is_some(){
      let x = x.unwrap();
      if x.is_some(){
        return x.as_mut();
      }
    }
    None
  } 
  
  /// Remove the stored value with the given key.
  pub fn remove(&mut self, i:usize) -> Option<T> {
    self.empty.push(i);
    mem::replace(&mut self.data[i], None)
  }
  
  /// Return the number of key/value pairs contained in this map.
  pub fn len(&self) -> usize {
    self.data.len() - self.empty.len()
  }

  /// List the keys to the data in this map
  pub fn keys(&self) -> Vec<usize> {
    let mut v = Vec::new();
    let n = self.data.len();
    let mut i = 0;
    while i<n {
      if !self.empty.contains(&(i as usize)) {
        v.push(i as usize);
      }
      i += 1;
    }
    v
  }
}

impl<T> Index<usize> for UsizeMap<T> {
  type Output = T;
  
  fn index(&self, i: usize) -> &Self::Output {
    self.data[i].as_ref().unwrap()
  }
}

impl<T: core::fmt::Debug> fmt::Debug for UsizeMap<T> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "count {}, empty {}", self.len(), self.empty.len()).unwrap();
    Ok(())
  }
}

