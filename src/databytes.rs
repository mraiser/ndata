use std::sync::Mutex;
use std::alloc::{alloc, Layout};
use crate::heap::*;

/// Storage for runtime byte buffer values
pub static mut BH:*mut Mutex<Heap<Vec<u8>>> = 0 as *mut Mutex<Heap<Vec<u8>>>;

/// Storage for runtime reference count reductions
pub static mut BD:*mut Mutex<Vec<usize>> = 0 as *mut Mutex<Vec<usize>>;

/// **DO NOT USE**
///
/// This function should only be used externally by DataArray and DataObject
pub fn bheap() -> &'static mut Mutex<Heap<Vec<u8>>> {
  unsafe { &mut *BH }
}

fn bdrop() -> &'static mut Mutex<Vec<usize>> {
  unsafe { &mut *BD }
}

/// Represents a buffer of bytes (```Vec<u8>```)
#[derive(Debug, Default)]
pub struct DataBytes {
  /// The pointer to the array in the byte buffer heap.
  pub data_ref: usize,
}

impl DataBytes {
  /// Initialize global storage of byte buffers. Call only once at startup.
  pub fn init() -> (usize, usize){
    let ptr1;
    let ptr2;
    unsafe {
      let layout = Layout::new::<Mutex<Heap<Vec<u8>>>>();
      ptr1 = alloc(layout);
      *(ptr1 as *mut Mutex<Heap<Vec<u8>>>) = Mutex::new(Heap::new());
      let layout = Layout::new::<Mutex<Vec<usize>>>();
      ptr2 = alloc(layout);
      *(ptr2 as *mut Mutex<Vec<usize>>) = Mutex::new(Vec::new());
    }
    let q = ptr1 as usize;
    let r = ptr2 as usize;
    DataBytes::mirror(q, r);
    (q, r)
  }
  
  /// Mirror global storage of arrays from another process. Call only once at startup.
  pub fn mirror(q:usize, r:usize){
    unsafe { 
      BH = q as *mut Mutex<Heap<Vec<u8>>>; 
      BD = r as *mut Mutex<Vec<usize>>;
    }
  }
  
  /// Create a new (empty) byte buffer.
  pub fn new() -> DataBytes {
    let data_ref = &mut bheap().lock().unwrap().push(Vec::<u8>::new());
    return DataBytes {
      data_ref: *data_ref,
    };
  }
  
  /// Get a reference to the byte buffer from the heap
  pub fn get(data_ref: usize) -> DataBytes {
    let o = DataBytes{
      data_ref: data_ref,
    };
    let _x = &mut bheap().lock().unwrap().incr(data_ref);
    o
  }
  
  /// Increase the reference count for this DataBytes.
  pub fn incr(&self) {
    let bheap = &mut bheap().lock().unwrap();
    bheap.incr(self.data_ref); 
  }

  /// Decrease the reference count for this DataBytes.
  pub fn decr(&self) {
    let bheap = &mut bheap().lock().unwrap();
    bheap.decr(self.data_ref); 
  }

  /// Returns a new ```DataBytes``` that points to the same underlying byte buffer.
  pub fn duplicate(&self) -> DataBytes {
    let o = DataBytes{
      data_ref: self.data_ref,
    };
    let _x = &mut bheap().lock().unwrap().incr(self.data_ref);
    o
  }
  
  /// Returns a new ```DataBytes``` that points to a copy of the underlying byte buffer.
  pub fn deep_copy(&self) -> DataBytes {
    let heap = &mut bheap().lock().unwrap();
    let bytes = heap.get(self.data_ref);
    let vec = bytes.to_owned();
    let data_ref = &mut bheap().lock().unwrap().push(vec);
    return DataBytes {
      data_ref: *data_ref,
    };
  }
  
  /// Returns the byte buffer as a hexidecimal String.
  pub fn to_hex_string(&self) -> String {
    let heap = &mut bheap().lock().unwrap();
    let bytes = heap.get(self.data_ref);
    let strs: Vec<String> = bytes.iter()
                                 .map(|b| format!("{:02X}", b))
                                 .collect();
    strs.join(" ")    
  }
  
  /// Prints the byte buffers currently stored in the heap
  pub fn print_heap() {
    println!("object {:?}", &mut bheap().lock().unwrap());
  }
  
  /// Perform garbage collection. Byte buffers will not be removed from the heap until
  /// ```DataBytes::gc()``` is called.
  pub fn gc() {
    let bheap = &mut bheap().lock().unwrap();
    let bdrop = &mut bdrop().lock().unwrap();
    let mut i = bdrop.len();
    while i>0 {
      i = i - 1;
      let x = bdrop.remove(0);
      bheap.decr(x);
    }
  }
}

/// Adds this ```DataBytes```'s data_ref to BDROP. Reference counts are adjusted when
/// ```DataBytes::gc()``` is called.
impl Drop for DataBytes {
  fn drop(&mut self) {
    bdrop().lock().unwrap().push(self.data_ref);
  }
}

