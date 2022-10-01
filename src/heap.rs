use crate::usizemap::*;

#[derive(Debug)]
struct Blob<T> {
  data: T,
  count: usize,
}

/// A reference counting container for objects of a given type with automatic garbage collection
#[derive(Debug)]
pub struct Heap<T> {
  data: UsizeMap<Blob<T>>,
}

impl<T: std::fmt::Debug> Heap<T> {
  /// Create a new ```Heap``` of type ```T```.
  pub fn new() -> Heap<T> {
    Heap {
      data: UsizeMap::<Blob<T>>::new(),
    }
  }

  /// Push an instance of type ```T``` to the heap and return a (```usize```) reference to it.
  pub fn push(&mut self, data: T) -> usize {
    let blob = Blob{
      data: data,
      count: 1,
    };
    
    self.data.insert(blob)
  }
  
  /// Return the value for the given data reference.
  pub fn get(&mut self, index:usize) -> &mut T {
    &mut self.data.get_mut(index).unwrap().data
  }

  /// Return the given instance's reference count.
  pub fn count(&mut self, index:usize) -> usize {
    self.data[index].count
  }

  /// Increase the given instance's reference count by one.
  pub fn incr(&mut self, index:usize) {
    self.data.get_mut(index).unwrap().count += 1;
  }
 
  /// Decrease the given instance's reference count by one.
  pub fn decr(&mut self, index: usize) {
    let b = self.data.get_mut(index).unwrap();
    let c = b.count;
    if c == 1 {
      self.data.remove(index);
    }
    else {
      b.count = c-1;
    }
  }
  
  /// List the keys to the data on the heap
  pub fn keys(&self) -> Vec<usize> {
    self.data.keys()
  }
}


