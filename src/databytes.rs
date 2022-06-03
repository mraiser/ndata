use std::sync::RwLock;
use state::Storage;

use crate::heap::*;

pub static BHEAP:Storage<RwLock<Heap<Vec<u8>>>> = Storage::new();
pub static BDROP:Storage<RwLock<Vec<usize>>> = Storage::new();

#[derive(Debug, Default)]
pub struct DataBytes {
  pub data_ref: usize,
}

impl DataBytes {
  pub fn init(){
    BHEAP.set(RwLock::new(Heap::new()));
    BDROP.set(RwLock::new(Vec::new()));
  }
  
  pub fn new() -> DataBytes {
    let data_ref = &mut BHEAP.get().write().unwrap().push(Vec::<u8>::new());
    return DataBytes {
      data_ref: *data_ref,
    };
  }
  
  pub fn get(data_ref: usize) -> DataBytes {
    let o = DataBytes{
      data_ref: data_ref,
    };
    let _x = &mut BHEAP.get().write().unwrap().incr(data_ref);
    o
  }
  
  pub fn duplicate(&self) -> DataBytes {
    let o = DataBytes{
      data_ref: self.data_ref,
    };
    let _x = &mut BHEAP.get().write().unwrap().incr(self.data_ref);
    o
  }
  
  pub fn deep_copy(&self) -> DataBytes {
    let heap = &mut BHEAP.get().write().unwrap();
    let bytes = heap.get(self.data_ref);
    let vec = bytes.to_owned();
    let data_ref = &mut BHEAP.get().write().unwrap().push(vec);
    return DataBytes {
      data_ref: *data_ref,
    };
  }
    
  pub fn to_hex_string(&self) -> String {
    let heap = &mut BHEAP.get().write().unwrap();
    let bytes = heap.get(self.data_ref);
    let strs: Vec<String> = bytes.iter()
                                 .map(|b| format!("{:02X}", b))
                                 .collect();
    strs.join(" ")    
  }
  
  pub fn print_heap() {
    println!("object {:?}", &mut BHEAP.get().write().unwrap());
  }
  
  pub fn gc() {
    let bheap = &mut BHEAP.get().write().unwrap();
    let bdrop = &mut BDROP.get().write().unwrap();
    let mut i = bdrop.len();
    while i>0 {
      i = i - 1;
      let x = bdrop.remove(0);
      bheap.decr(x);
    }
  }
}

impl Drop for DataBytes {
  fn drop(&mut self) {
    BDROP.get().write().unwrap().push(self.data_ref);
  }
}

