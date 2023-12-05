//! [![github]](https://github.com/mraiser/ndata)&ensp;[![crates-io]](https://crates.io/crates/ndata)&ensp;[![docs-rs]](https://docs.rs/ndata)
//!
//! [github]: https://img.shields.io/badge/github-8da0cb?style=for-the-badge&labelColor=555555&logo=github
//! [crates-io]: https://img.shields.io/badge/crates.io-fc8d62?style=for-the-badge&labelColor=555555&logo=rust
//! [docs-rs]: https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logoColor=white&logo=data:image/svg+xml;base64,PHN2ZyByb2xlPSJpbWciIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgdmlld0JveD0iMCAwIDUxMiA1MTIiPjxwYXRoIGZpbGw9IiNmNWY1ZjUiIGQ9Ik00ODguNiAyNTAuMkwzOTIgMjE0VjEwNS41YzAtMTUtOS4zLTI4LjQtMjMuNC0zMy43bC0xMDAtMzcuNWMtOC4xLTMuMS0xNy4xLTMuMS0yNS4zIDBsLTEwMCAzNy41Yy0xNC4xIDUuMy0yMy40IDE4LjctMjMuNCAzMy43VjIxNGwtOTYuNiAzNi4yQzkuMyAyNTUuNSAwIDI2OC45IDAgMjgzLjlWMzk0YzAgMTMuNiA3LjcgMjYuMSAxOS45IDMyLjJsMTAwIDUwYzEwLjEgNS4xIDIyLjEgNS4xIDMyLjIgMGwxMDMuOS01MiAxMDMuOSA1MmMxMC4xIDUuMSAyMi4xIDUuMSAzMi4yIDBsMTAwLTUwYzEyLjItNi4xIDE5LjktMTguNiAxOS45LTMyLjJWMjgzLjljMC0xNS05LjMtMjguNC0yMy40LTMzLjd6TTM1OCAyMTQuOGwtODUgMzEuOXYtNjguMmw4NS0zN3Y3My4zek0xNTQgMTA0LjFsMTAyLTM4LjIgMTAyIDM4LjJ2LjZsLTEwMiA0MS40LTEwMi00MS40di0uNnptODQgMjkxLjFsLTg1IDQyLjV2LTc5LjFsODUtMzguOHY3NS40em0wLTExMmwtMTAyIDQxLjQtMTAyLTQxLjR2LS42bDEwMi0zOC4yIDEwMiAzOC4ydi42em0yNDAgMTEybC04NSA0Mi41di03OS4xbDg1LTM4Ljh2NzUuNHptMC0xMTJsLTEwMiA0MS40LTEwMi00MS40di0uNmwxMDItMzguMiAxMDIgMzguMnYuNnoiPjwvcGF0aD48L3N2Zz4K
//!
//! <br>
//!
//! This crate provides a a self-owned data structure with an internal heap and garbage collection.
//!
//! NData supports objects, arrays, strings, integers, floats, 
//! booleans, byte buffers, and null. DataObject, DataArray, and DataBytes instances 
//! maintain reference counts. Garbage collection is performed manually by calling the 
//! type's gc() function.

pub mod heap;
pub mod usizemap;
pub mod data;
pub mod dataobject;
pub mod dataarray;
pub mod databytes;
pub mod sharedmutex;

#[cfg(not(feature="serde_support"))]
pub mod json_util;

use crate::dataobject::*;
use crate::dataarray::*;
use crate::databytes::*;

#[derive(Debug, Default, Copy, Clone)]
pub struct NDataConfig{
  data: (((u64,u64),(u64,u64)),((u64,u64),(u64,u64)),((u64,u64),(u64,u64))),
}

/// Initialize global storage of data. Call only once at startup.
pub fn init() -> NDataConfig {
  NDataConfig{
    data: (DataObject::init(), DataArray::init(), DataBytes::init()),
  }
}

/// Mirror global storage of data from another process. Call only once at startup.
pub fn mirror(data_ref:NDataConfig) {
  DataObject::mirror(data_ref.data.0.0, data_ref.data.0.1);
  DataArray::mirror(data_ref.data.1.0, data_ref.data.1.1);
  DataBytes::mirror(data_ref.data.2.0, data_ref.data.2.1);
}

/// Perform garbage collection. Instances will not be removed from the heap until
/// ```NData::gc()``` is called.
pub fn gc() {
  DataObject::gc();
  DataArray::gc();
  DataBytes::gc();
}

/// Prints the objects currently stored in the heap
#[cfg(not(feature="no_std_support"))]
pub fn print_heap() {
  println!("------------ HEAP ------------");
  DataObject::print_heap();
  DataArray::print_heap();
  DataBytes::print_heap();
  println!("------------------------------");
}

