extern crate alloc;
use core::cmp;
use crate::heap::*;
use crate::sharedmutex::*;

/// Storage for runtime byte buffer values
static mut BH:SharedMutex<Heap<DataStream>> = SharedMutex::new();

/// Storage for runtime reference count reductions
static mut BD:SharedMutex<Vec<usize>> = SharedMutex::new();

/// Implements a stream of bytes
#[derive(Debug, Default)]
pub struct DataStream {
  /// Raw data currently held in stream
  data: Vec<u8>,
  /// Length of data to be sent in this stream. Value should be zero (unset) or fixed (unchanging) value.
  len: usize,
  /// Indicates whether the current stream is open to reading
  read_open: bool,
  /// Indicates whether the current stream is open to writing
  write_open: bool,
  /// Optional MIME type of this stream
  mime_type: Option<String>,
}

impl DataStream {
  /// Create a new (empty) byte stream.
  pub fn new() -> Self {
    DataStream {
      data: Vec::new(),
      len: 0,
      read_open: true,
      write_open: true,
      mime_type: None,
    }
  }

  /// Create a new byte stream from a Vec<u8>.
  pub fn from_bytes(buf:Vec<u8>) -> DataStream {
    let len = buf.len();
    DataStream {
      data: buf,
      len: len,
      read_open: true,
      write_open: false,
      mime_type: None,
    }
  }

  /// Return a deep copy of the data stream
  pub fn deep_copy(&self) -> DataStream {
    DataStream {
      data: self.data.to_owned(),
      len: self.len,
      read_open: self.read_open,
      write_open: self.write_open,
      mime_type: self.mime_type.to_owned(),
    }
  }
}

/// **DO NOT USE**
///
/// This function should only be used externally by DataArray and DataObject
pub fn bheap() -> &'static mut SharedMutex<Heap<DataStream>> {
  #[allow(static_mut_refs)]
  unsafe { &mut BH }
}

fn bdrop() -> &'static mut SharedMutex<Vec<usize>> {
  #[allow(static_mut_refs)]
  unsafe { &mut BD }
}

/// Represents a buffer of bytes (```Vec<u8>```)
#[derive(Debug, Default)]
pub struct DataBytes {
  /// The pointer to the array in the byte buffer heap.
  pub data_ref: usize,
}

impl Clone for DataBytes{
  /// Returns another DataBytes pointing to the same value.
  fn clone(&self) -> Self {
    let o = DataBytes{
      data_ref: self.data_ref,
    };
    let _x = &mut bheap().lock().incr(self.data_ref);
    o
  }
}

impl DataBytes {
  /// Initialize global storage of byte buffers. Call only once at startup.
  #[allow(static_mut_refs)]
  pub fn init() -> ((u64, u64),(u64, u64)){
    unsafe {
      if !BH.is_initialized() {
        BH.set(Heap::new());
        BD.set(Vec::new());
      }
    }
    DataBytes::share()
  }

  #[allow(static_mut_refs)]
  pub fn share() -> ((u64, u64), (u64, u64)){
    unsafe{
      let q = BH.share();
      let r = BD.share();
      (q, r)
    }
  }

  /// Mirror global storage of arrays from another process. Call only once at startup.
  #[allow(static_mut_refs)]
  pub fn mirror(q:(u64, u64), r:(u64, u64)){
    unsafe {
      BH.mirror(q.0, q.1);
      BD.mirror(r.0, r.1);
    }
  }

  /// Create a new (empty) byte buffer.
  pub fn new() -> DataBytes {
    let data_ref = &mut bheap().lock().push(DataStream::new());
    return DataBytes {
      data_ref: *data_ref,
    };
  }

  /// Create a new byte buffer from a Vec<u8>.
  pub fn from_bytes(buf:&Vec<u8>) -> DataBytes {
    let data_ref = &mut bheap().lock().push(DataStream::from_bytes(buf.to_vec()));
    return DataBytes {
      data_ref: *data_ref,
    };
  }

  /// Returns a copy of the underlying vec of bytes in the array
  pub fn get_data(&self) -> Vec<u8> {
    let heap = &mut bheap().lock();
    let vec = heap.get(self.data_ref);
    vec.data.to_owned()
  }

  /// Appends the given slice to the end of the bytes in the array
  pub fn write(&self, buf:&[u8]) -> bool {
    let heap = &mut bheap().lock();
    let vec = heap.get(self.data_ref);
    if !vec.write_open || !vec.read_open { return false }
    vec.data.extend_from_slice(buf);
    true
  }

  /// Removes and returns up to the requested number of bytes from the array
  pub fn read(&self, n:usize) -> Vec<u8> {
    let heap = &mut bheap().lock();
    let vec = heap.get(self.data_ref);
    if !vec.read_open { panic!("Attempt to read from closed data stream"); }
    let n = cmp::min(n, vec.data.len());
    let d = vec.data[0..n].to_vec();
    vec.data.drain(0..n);
    if !vec.write_open && vec.data.len() == 0 {
      vec.read_open = false;
    }
    d
  }

  /// Sets the underlying vec of bytes in the array
  pub fn set_data(&self, buf:&Vec<u8>) {
    let heap = &mut bheap().lock();
    let vec = heap.get(self.data_ref);
    let len = buf.len();
    vec.data.resize(len, 0); // FIXME - Is this necessary?
    vec.data.clone_from_slice(buf);
    vec.len = len;
    vec.write_open = false;
  }

  /// Get the number of bytes currently in the underlying byte buffer
  pub fn current_len(&self) -> usize {
    let heap = &mut bheap().lock();
    let vec = heap.get(self.data_ref);
    vec.data.len()
  }

  /// Get the declared total number of bytes in the stream
  pub fn stream_len(&self) -> usize {
    let heap = &mut bheap().lock();
    let vec = heap.get(self.data_ref);
    vec.len
  }

  /// Set the declared total number of bytes in the stream
  pub fn set_stream_len(&self, len: usize) {
    let heap = &mut bheap().lock();
    let vec = heap.get(self.data_ref);
    vec.len = len;
  }

  /// Return true if the underlying data stream is open for writing
  pub fn is_write_open(&self) -> bool {
    let heap = &mut bheap().lock();
    let vec = heap.get(self.data_ref);
    vec.write_open
  }

  /// Return true if the underlying data stream is open for reading
  pub fn is_read_open(&self) -> bool {
    let heap = &mut bheap().lock();
    let vec = heap.get(self.data_ref);
    vec.read_open
  }

  /// Close the underlying data stream to further writing
  pub fn close_write(&self) {
    let heap = &mut bheap().lock();
    let vec = heap.get(self.data_ref);
    vec.write_open = false;
  }

  /// Close the underlying data stream to further reading
  pub fn close_read(&self) {
    let heap = &mut bheap().lock();
    let vec = heap.get(self.data_ref);
    vec.read_open = false;
  }

  /// Set the optional MIME type for this stream
  pub fn set_mime_type(&self, mime:Option<String>) {
    let heap = &mut bheap().lock();
    let vec = heap.get(self.data_ref);
    vec.mime_type = mime;
  }

  /// Get the optional MIME type for this stream
  pub fn get_mime_type(&self) -> Option<String> {
    let heap = &mut bheap().lock();
    let vec = heap.get(self.data_ref);
    vec.mime_type.to_owned()
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
  #[deprecated(since="0.3.0", note="please use `clone` instead")]
  pub fn duplicate(&self) -> DataBytes {
    self.clone()
  }

  /// Returns a new ```DataBytes``` that points to a copy of the underlying byte buffer.
  pub fn deep_copy(&self) -> DataBytes {
    let heap = &mut bheap().lock();
    let bytes = heap.get(self.data_ref);
    let vec = bytes.deep_copy();
    let data_ref = &mut bheap().lock().push(vec);
    return DataBytes {
      data_ref: *data_ref,
    };
  }

  /// Returns the byte buffer as a hexidecimal String.
  pub fn to_hex_string(&self) -> String {
    let heap = &mut bheap().lock();
    let bytes = heap.get(self.data_ref);
    let strs: Vec<String> = bytes.data.iter()
    .map(|b| format!("{:02X}", b))
    .collect();
    strs.join(" ")
  }

  /// Prints the byte buffers currently stored in the heap
  #[cfg(not(feature="no_std_support"))]
  pub fn print_heap() {
    println!("bytes {:?}", &mut bheap().lock().keys());
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
