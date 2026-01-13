extern crate alloc;
use core::cmp;
use core::mem; 
use crate::heap::*;
use crate::sharedmutex::*;
use alloc::collections::VecDeque; 

#[cfg(feature="no_std_support")]
use alloc::string::{String, ToString};
#[cfg(feature="no_std_support")]
use alloc::vec::Vec;
#[cfg(not(feature="no_std_support"))]
use std::println;


// --- NDataError Definition ---
#[derive(Debug)]
pub enum NDataError {
    InvalidBytesRef,
    StreamNotReadable,
    StreamNotWritable,
}

impl core::fmt::Display for NDataError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            NDataError::InvalidBytesRef => write!(f, "DataBytes reference is invalid or points to deallocated memory"),
            NDataError::StreamNotReadable => write!(f, "Stream is not open for reading"),
            NDataError::StreamNotWritable => write!(f, "Stream is not open for writing"),
        }
    }
}

#[cfg(not(feature = "no_std_support"))]
impl std::error::Error for NDataError {}


/// Storage for runtime byte buffer values
static mut BH:SharedMutex<Heap<DataStream>> = SharedMutex::new();

/// Storage for runtime reference count reductions
static mut BD:SharedMutex<Vec<usize>> = SharedMutex::new();

/// Implements a stream of bytes.
///
/// Internally uses a VecDeque to allow O(1) removal from the front of the stream,
/// solving performance issues with repeated small reads from large buffers.
#[derive(Debug, Default)]
pub struct DataStream {
    /// Raw data currently held in stream
    data: VecDeque<u8>,
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
    pub fn new() -> Self {
        DataStream {
            data: VecDeque::new(),
            len: 0,
            read_open: true,
            write_open: true,
            mime_type: None,
        }
    }

    pub fn from_bytes(buf: Vec<u8>) -> DataStream {
        let len = buf.len();
        DataStream {
            data: VecDeque::from(buf),
            len: len,
            read_open: true,
            write_open: false,
            mime_type: None,
        }
    }

    pub fn deep_copy(&self) -> DataStream {
        DataStream {
            data: self.data.clone(),
            len: self.len,
            read_open: self.read_open,
            write_open: self.write_open,
            mime_type: self.mime_type.as_ref().map(|s| s.to_string()),
        }
    }
}

pub fn bheap() -> &'static mut SharedMutex<Heap<DataStream>> {
    #[allow(static_mut_refs)]
    unsafe { &mut BH }
}

fn bdrop() -> &'static mut SharedMutex<Vec<usize>> {
    #[allow(static_mut_refs)]
    unsafe { &mut BD }
}

#[derive(Debug, Default)]
pub struct DataBytes {
    pub data_ref: usize,
}

impl Clone for DataBytes{
    fn clone(&self) -> Self {
        let _ = bheap().lock().incr(self.data_ref);
        DataBytes{
            data_ref: self.data_ref,
        }
    }
}

impl DataBytes {
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

    #[allow(static_mut_refs)]
    pub fn mirror(q:(u64, u64), r:(u64, u64)){
        unsafe {
            BH.mirror(q.0, q.1);
            BD.mirror(r.0, r.1);
        }
    }

    pub fn new() -> DataBytes {
        let data_ref = bheap().lock().push(DataStream::new());
        DataBytes { data_ref }
    }

    pub fn from_bytes(buf:&Vec<u8>) -> DataBytes {
        // Clone the input vec into the deque
        let data_ref = bheap().lock().push(DataStream::from_bytes(buf.clone()));
        DataBytes { data_ref }
    }

    // --- Original Public Methods (panicking on error) ---

    pub fn get_data(&self) -> Vec<u8> {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            panic!("DataBytes::get_data called on invalid data_ref: {}", self.data_ref);
        }
        let stream = heap_guard.get(self.data_ref);
        // Convert VecDeque to Vec
        Vec::from(stream.data.clone())
    }

    pub fn write(&self, buf:&[u8]) -> bool {
        // WARNING - No guard on writing too much too fast and running out of RAM
        //if self.current_len() > 100 * 1024 * 1024 { return false; }

        let mut heap_guard = bheap().lock();
         if !heap_guard.contains_key(self.data_ref) {
             if cfg!(debug_assertions) {
                #[cfg(not(feature="no_std_support"))]
                println!("Warning: DataBytes::write called on invalid data_ref: {}", self.data_ref);
             }
            return false;
        }
        let stream = heap_guard.get(self.data_ref);
        if !stream.write_open || !stream.read_open { return false; }

        // Efficiently extend the VecDeque
        stream.data.extend(buf.iter().copied());
        true
    }

    pub fn read(&self, n:usize) -> Vec<u8> {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            panic!("DataBytes::read called on invalid data_ref: {}", self.data_ref);
        }
        let stream = heap_guard.get(self.data_ref);
        if !stream.read_open {
            panic!("Attempt to read from closed data stream: ref {}", self.data_ref);
        }
        let num_to_read = cmp::min(n, stream.data.len());

        // Optimized drain on VecDeque (O(1) pointer move, followed by collection)
        let d: Vec<u8> = stream.data.drain(0..num_to_read).collect();

        if !stream.write_open && stream.data.is_empty() {
            stream.read_open = false;
        }
        
        // MEMORY RECOVERY:
        // If the stream was large but is now empty, release the backing memory.
        if stream.data.is_empty() && stream.data.capacity() > 1024 * 64 {
            stream.data.shrink_to_fit();
        }

        d
    }

    pub fn set_data(&self, buf:&Vec<u8>) {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            panic!("DataBytes::set_data called on invalid data_ref: {}", self.data_ref);
        }
        let stream = heap_guard.get(self.data_ref);
        let len = buf.len();
        stream.data.clear();
        stream.data.extend(buf.iter().copied());

        stream.len = len;
        stream.write_open = false;
    }

    pub fn current_len(&self) -> usize {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            panic!("DataBytes::current_len called on invalid data_ref: {}", self.data_ref);
        }
        let stream = heap_guard.get(self.data_ref);
        stream.data.len()
    }

    pub fn stream_len(&self) -> usize {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            panic!("DataBytes::stream_len called on invalid data_ref: {}", self.data_ref);
        }
        let stream = heap_guard.get(self.data_ref);
        stream.len
    }

    pub fn set_stream_len(&self, len: usize) {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            panic!("DataBytes::set_stream_len called on invalid data_ref: {}", self.data_ref);
        }
        let stream = heap_guard.get(self.data_ref);
        stream.len = len;
    }

    pub fn is_write_open(&self) -> bool {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            panic!("DataBytes::is_write_open called on invalid data_ref: {}", self.data_ref);
        }
        let stream = heap_guard.get(self.data_ref);
        stream.write_open
    }

    pub fn is_read_open(&self) -> bool {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            panic!("DataBytes::is_read_open called on invalid data_ref: {}", self.data_ref);
        }
        let stream = heap_guard.get(self.data_ref);
        stream.read_open
    }

    pub fn close_write(&self) {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            panic!("DataBytes::close_write called on invalid data_ref: {}", self.data_ref);
        }
        let stream = heap_guard.get(self.data_ref);
        stream.write_open = false;
    }

    pub fn close_read(&self) {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            panic!("DataBytes::close_read called on invalid data_ref: {}", self.data_ref);
        }
        let stream = heap_guard.get(self.data_ref);
        stream.read_open = false;
    }

    pub fn set_mime_type(&self, mime:Option<String>) {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            panic!("DataBytes::set_mime_type called on invalid data_ref: {}", self.data_ref);
        }
        let stream = heap_guard.get(self.data_ref);
        stream.mime_type = mime;
    }

    pub fn get_mime_type(&self) -> Option<String> {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            panic!("DataBytes::get_mime_type called on invalid data_ref: {}", self.data_ref);
        }
        let stream = heap_guard.get(self.data_ref);
        stream.mime_type.as_ref().map(|s| s.to_string())
    }

    pub fn to_hex_string(&self) -> String {
        let mut heap_guard = bheap().lock();
         if !heap_guard.contains_key(self.data_ref) {
            panic!("DataBytes::to_hex_string called on invalid data_ref: {}", self.data_ref);
        }
        let stream = heap_guard.get(self.data_ref);
        let strs: Vec<String> = stream.data.iter()
            .map(|b| format!("{:02X}", b))
            .collect();
        strs.join(" ")
    }

    pub fn deep_copy(&self) -> DataBytes {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            panic!("DataBytes::deep_copy called on invalid data_ref: {}", self.data_ref);
        }
        let stream_to_copy = heap_guard.get(self.data_ref);
        let new_stream = stream_to_copy.deep_copy();

        let new_data_ref = heap_guard.push(new_stream);
        DataBytes {
            data_ref: new_data_ref,
        }
    }

    // --- New High-Performance Methods ---

    /// Reads up to `buf.len()` bytes into the provided buffer.
    /// Returns the number of bytes read.
    pub fn read_into(&self, buf: &mut [u8]) -> usize {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            panic!("DataBytes::read_into called on invalid data_ref: {}", self.data_ref);
        }
        let stream = heap_guard.get(self.data_ref);
        if !stream.read_open {
             panic!("Attempt to read from closed data stream: ref {}", self.data_ref);
        }

        let num_to_read = cmp::min(buf.len(), stream.data.len());

        // Optimized copy from VecDeque
        for (i, byte) in stream.data.drain(0..num_to_read).enumerate() {
            buf[i] = byte;
        }

        if !stream.write_open && stream.data.is_empty() {
            stream.read_open = false;
        }

        // MEMORY RECOVERY
        if stream.data.is_empty() && stream.data.capacity() > 1024 * 64 {
            stream.data.shrink_to_fit();
        }

        num_to_read
    }

    /// Peeks at the first `n` bytes of the stream without consuming them.
    pub fn peek(&self, n: usize) -> Vec<u8> {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            panic!("DataBytes::peek called on invalid data_ref: {}", self.data_ref);
        }
        let stream = heap_guard.get(self.data_ref);
        let num_to_peek = cmp::min(n, stream.data.len());
        stream.data.iter().take(num_to_peek).copied().collect()
    }

    /// Moves all available data from this stream to the destination stream.
    /// Returns the number of bytes moved.
    pub fn pipe(&self, dest: &DataBytes) -> usize {
        // Reads all data and writes to dest.
        // NOTE: This allocates a temporary Vec. 
        // For zero-allocation piping, we would need a new method in Heap/SharedMutex 
        // that allows locking two items simultaneously, which is complex.
        let chunk = self.read(usize::MAX);
        let len = chunk.len();

        if len > 0 {
            dest.write(&chunk);
        }
        len
    }

    // --- New `try_` Methods (non-panicking, return Result) ---

    pub fn try_get_data(&self) -> Result<Vec<u8>, NDataError> {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            return Err(NDataError::InvalidBytesRef);
        }
        let stream = heap_guard.get(self.data_ref);
        Ok(Vec::from(stream.data.clone()))
    }

    pub fn try_write(&mut self, buf:&[u8]) -> Result<(), NDataError> {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            return Err(NDataError::InvalidBytesRef);
        }
        let stream = heap_guard.get(self.data_ref);
        if !stream.write_open {
            return Err(NDataError::StreamNotWritable);
        }
        stream.data.extend(buf.iter().copied());
        Ok(())
    }

    pub fn try_read(&mut self, n:usize) -> Result<Vec<u8>, NDataError> {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            return Err(NDataError::InvalidBytesRef);
        }
        let stream = heap_guard.get(self.data_ref);
        if !stream.read_open {
            return Err(NDataError::StreamNotReadable);
        }
        let num_to_read = cmp::min(n, stream.data.len());
        let d: Vec<u8> = stream.data.drain(0..num_to_read).collect();

        if !stream.write_open && stream.data.is_empty() {
            stream.read_open = false;
        }

        // MEMORY RECOVERY
        if stream.data.is_empty() && stream.data.capacity() > 1024 * 64 {
            stream.data.shrink_to_fit();
        }

        Ok(d)
    }

    pub fn try_set_data(&mut self, buf:&Vec<u8>) -> Result<(), NDataError> {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            return Err(NDataError::InvalidBytesRef);
        }
        let stream = heap_guard.get(self.data_ref);
        let len = buf.len();
        stream.data.clear();
        stream.data.extend(buf.iter().copied());

        stream.len = len;
        stream.write_open = false;
        Ok(())
    }

    pub fn try_current_len(&self) -> Result<usize, NDataError> {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            return Err(NDataError::InvalidBytesRef);
        }
        let stream = heap_guard.get(self.data_ref);
        Ok(stream.data.len())
    }

    pub fn try_stream_len(&self) -> Result<usize, NDataError> {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            return Err(NDataError::InvalidBytesRef);
        }
        let stream = heap_guard.get(self.data_ref);
        Ok(stream.len)
    }

    pub fn try_set_stream_len(&mut self, len: usize) -> Result<(), NDataError> {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            return Err(NDataError::InvalidBytesRef);
        }
        let stream = heap_guard.get(self.data_ref);
        stream.len = len;
        Ok(())
    }

    pub fn try_is_write_open(&self) -> Result<bool, NDataError> {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            return Err(NDataError::InvalidBytesRef);
        }
        let stream = heap_guard.get(self.data_ref);
        Ok(stream.write_open)
    }

    pub fn try_is_read_open(&self) -> Result<bool, NDataError> {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            return Err(NDataError::InvalidBytesRef);
        }
        let stream = heap_guard.get(self.data_ref);
        Ok(stream.read_open)
    }

    pub fn try_close_write(&mut self) -> Result<(), NDataError> {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            return Err(NDataError::InvalidBytesRef);
        }
        let stream = heap_guard.get(self.data_ref);
        stream.write_open = false;
        Ok(())
    }

    pub fn try_close_read(&mut self) -> Result<(), NDataError> {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            return Err(NDataError::InvalidBytesRef);
        }
        let stream = heap_guard.get(self.data_ref);
        stream.read_open = false;
        Ok(())
    }

    pub fn try_set_mime_type(&mut self, mime:Option<String>) -> Result<(), NDataError> {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            return Err(NDataError::InvalidBytesRef);
        }
        let stream = heap_guard.get(self.data_ref);
        stream.mime_type = mime;
        Ok(())
    }

    pub fn try_get_mime_type(&self) -> Result<Option<String>, NDataError> {
        let mut heap_guard = bheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            return Err(NDataError::InvalidBytesRef);
        }
        let stream = heap_guard.get(self.data_ref);
        Ok(stream.mime_type.as_ref().map(|s| s.to_string()))
    }

    // --- Static and other existing methods ---
    pub fn get(data_ref: usize) -> DataBytes {
        let _ = bheap().lock().incr(data_ref);
        DataBytes{
            data_ref,
        }
    }

    pub fn incr(&self) {
        let _ = bheap().lock().incr(self.data_ref);
    }

    pub fn decr(&self) {
        let _ = bheap().lock().decr(self.data_ref);
    }

    #[deprecated(since="0.3.0", note="please use `clone` instead")]
    pub fn duplicate(&self) -> DataBytes {
        self.clone()
    }

    #[cfg(not(feature="no_std_support"))]
    pub fn print_heap() {
        println!("bytes {:?}", bheap().lock().keys());
    }

    /// Garbage collects dropped streams.
    ///
    /// # Deadlock Safety
    /// This method uses a "swap-and-drop" strategy to avoid deadlock risks.
    /// 1. We acquire the drop list (`BD`), efficiently swap its contents with an empty list (O(1)), and release the lock immediately.
    /// 2. We then acquire the heap lock (`BH`) to process the decrements.
    ///
    /// By never holding `BD` and `BH` simultaneously, we prevent any "Hold and Wait" deadlock cycles
    /// involving these two resources.
    pub fn gc() {
        // Step 1: Atomic Swap (O(1))
        // We take the list of refs to decrement and release the BD lock immediately.
        let to_process = {
            let mut bdrop_guard = bdrop().lock();
            if bdrop_guard.is_empty() {
                return;
            }
            mem::take(&mut *bdrop_guard)
        }; // BD lock is released here.

        // Step 2: Process Decrements
        // We now hold only the BH lock.
        let mut bheap_guard = bheap().lock();
        for data_ref in to_process {
            if bheap_guard.contains_key(data_ref) {
                 bheap_guard.decr(data_ref);
            } else {
                #[cfg(not(feature = "no_std_support"))]
                if cfg!(debug_assertions) {
                    println!("Warning: DataBytes::gc trying to decr non-existent ref {}", data_ref);
                }
            }
        }
    }
}

impl Drop for DataBytes {
    fn drop(&mut self) {
        bdrop().lock().push(self.data_ref);
    }
}
