use crate::heap::*;
use crate::sharedmutex::*;

/// Storage for runtime byte buffer values
static mut BH:SharedMutex<Heap<Vec<u8>>> = SharedMutex::mirror(0, 0);

/// Storage for runtime reference count reductions
static mut BD:SharedMutex<Vec<usize>> = SharedMutex::mirror(0, 0);

/// **DO NOT USE**
///
/// This function should only be used externally by DataArray and DataObject
pub fn bheap() -> &'static mut SharedMutex<Heap<Vec<u8>>> {
  unsafe { &mut BH }
}

fn bdrop() -> &'static mut SharedMutex<Vec<usize>> {
  unsafe { &mut BD }
}

/// Represents a buffer of bytes (```Vec<u8>```)
#[derive(Debug, Default)]
pub struct DataBytes {
  /// The pointer to the array in the byte buffer heap.
  pub data_ref: usize,
}

impl DataBytes {
  /// Initialize global storage of byte buffers. Call only once at startup.
  pub fn init() -> ((usize, usize), (usize, usize)){
    unsafe{
      BH = SharedMutex::new();
      BD = SharedMutex::new();
      let q = BH.share();
      let r = BD.share();
      (q, r)
    }
  }
  
  /// Mirror global storage of arrays from another process. Call only once at startup.
  pub fn mirror(q:(usize, usize), r:(usize, usize)){
    unsafe { 
      BH = SharedMutex::mirror(q.0, q.1);
      BD = SharedMutex::mirror(r.0, r.1);
    }
  }
  
  /// Create a new (empty) byte buffer.
  pub fn new() -> DataBytes {
    let data_ref = &mut bheap().lock().push(Vec::<u8>::new());
    return DataBytes {
      data_ref: *data_ref,
    };
  }
  
  /// Create a new byte buffer from a Vec<u8>.
  pub fn from_bytes(buf:&Vec<u8>) -> DataBytes {
    let data_ref = &mut bheap().lock().push(buf.to_vec());
    return DataBytes {
      data_ref: *data_ref,
    };
  }
  
  /// Returns the underlying vec of bytes in the array
  pub fn get_data(&self) -> Vec<u8> {
    let heap = &mut bheap().lock();
    let vec = heap.get(self.data_ref);
    vec.to_owned()
  }
  
  /// Sets the underlying vec of bytes in the array
  pub fn set_data(&self, buf:&Vec<u8>) {
    let heap = &mut bheap().lock();
    let vec = heap.get(self.data_ref);
    vec.clone_from_slice(buf);
  }
  
  /// Get the length of the underlying byte buffer
  pub fn len(&self) -> usize {
    let heap = &mut bheap().lock();
    let vec = heap.get(self.data_ref);
    vec.len()
  }
  
  /// Get a reference to the byte buffer from the heap
  pub fn get(data_ref: usize) -> DataBytes {
    let o = DataBytes{
      data_ref: data_ref,
    };
    let _x = &mut bheap().lock().incr(data_ref);
    o
  }
  
  /// Increase the reference count for this DataBytes.
  pub fn incr(&self) {
    let bheap = &mut bheap().lock();
    bheap.incr(self.data_ref); 
  }

  /// Decrease the reference count for this DataBytes.
  pub fn decr(&self) {
    let bheap = &mut bheap().lock();
    bheap.decr(self.data_ref); 
  }

  /// Returns a new ```DataBytes``` that points to the same underlying byte buffer.
  pub fn duplicate(&self) -> DataBytes {
    let o = DataBytes{
      data_ref: self.data_ref,
    };
    let _x = &mut bheap().lock().incr(self.data_ref);
    o
  }
  
  /// Returns a new ```DataBytes``` that points to a copy of the underlying byte buffer.
  pub fn deep_copy(&self) -> DataBytes {
    let heap = &mut bheap().lock();
    let bytes = heap.get(self.data_ref);
    let vec = bytes.to_owned();
    let data_ref = &mut bheap().lock().push(vec);
    return DataBytes {
      data_ref: *data_ref,
    };
  }
  
  /// Returns the byte buffer as a hexidecimal String.
  pub fn to_hex_string(&self) -> String {
    let heap = &mut bheap().lock();
    let bytes = heap.get(self.data_ref);
    let strs: Vec<String> = bytes.iter()
                                 .map(|b| format!("{:02X}", b))
                                 .collect();
    strs.join(" ")    
  }
  
  /// Prints the byte buffers currently stored in the heap
  pub fn print_heap() {
    println!("object {:?}", &mut bheap().lock());
  }
  
  /// Perform garbage collection. Byte buffers will not be removed from the heap until
  /// ```DataBytes::gc()``` is called.
  pub fn gc() {
    let bheap = &mut bheap().lock();
    let bdrop = &mut bdrop().lock();
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
    bdrop().lock().push(self.data_ref);
  }
}

