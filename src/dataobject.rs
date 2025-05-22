//! This module defines the `DataObject` struct, a thread-safe, reference-counted
//! map-like data structure (`HashMap<String, Data>`) stored in a shared heap.

// Ensure code works in no_std environments if the feature is enabled.
#![cfg_attr(feature = "no_std_support", no_std)]

// Necessary imports from the standard library (or alloc crate for no_std).
extern crate alloc;
// Use std types when available (default)
#[cfg(not(feature = "no_std_support"))]
use std::collections::HashMap;
#[cfg(not(feature = "no_std_support"))]
use std::println;

// Use alloc types when only alloc is available and no_std_support is enabled
#[cfg(feature = "no_std_support")]
use alloc::collections::HashMap;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

// Imports from other modules within the ndata crate.
use crate::data::*;
use crate::dataarray::{self, DataArray};
use crate::databytes::{self, DataBytes};
use crate::heap::*;
use crate::sharedmutex::*;

// Conditional imports based on the `serde_support` feature flag.
#[cfg(feature = "serde_support")]
use serde_json::{json, Value};
#[cfg(not(feature = "serde_support"))]
use crate::json_util;

// --- Global Static Heaps ---
static mut OBJECT_HEAP: SharedMutex<Heap<HashMap<String, Data>>> = SharedMutex::new();
static mut OBJECT_DROP_QUEUE: SharedMutex<Vec<usize>> = SharedMutex::new();

// --- Heap Accessor Functions ---
#[doc(hidden)]
pub fn oheap() -> &'static mut SharedMutex<Heap<HashMap<String, Data>>> {
  #[allow(static_mut_refs)]
  unsafe { &mut OBJECT_HEAP }
}

fn odrop() -> &'static mut SharedMutex<Vec<usize>> {
  #[allow(static_mut_refs)]
  unsafe { &mut OBJECT_DROP_QUEUE }
}

// --- DataObject Definition ---
#[derive(Debug, Default)]
pub struct DataObject {
  pub data_ref: usize,
}

// --- Clone Implementation ---
impl Clone for DataObject {
  fn clone(&self) -> Self {
    let _ = oheap().lock().incr(self.data_ref);
    DataObject { data_ref: self.data_ref }
  }
}

// --- Core Functionality ---
impl DataObject {
  #[allow(static_mut_refs)]
  pub fn init() -> ((u64, u64), (u64, u64)) {
    unsafe {
      if !OBJECT_HEAP.is_initialized() {
        OBJECT_HEAP.set(Heap::new());
        OBJECT_DROP_QUEUE.set(Vec::new());
      }
    }
    Self::share()
  }

  #[allow(static_mut_refs)]
  pub fn share() -> ((u64, u64), (u64, u64)) {
    unsafe {
      let q = OBJECT_HEAP.share();
      let r = OBJECT_DROP_QUEUE.share();
      (q, r)
    }
  }

  #[allow(static_mut_refs)]
  pub fn mirror(q: (u64, u64), r: (u64, u64)) {
    unsafe {
      OBJECT_HEAP.mirror(q.0, q.1);
      OBJECT_DROP_QUEUE.mirror(r.0, r.1);
    }
  }

  pub fn new() -> Self {
    let data_ref = oheap().lock().push(HashMap::<String, Data>::new());
    DataObject { data_ref }
  }

  pub fn get(data_ref: usize) -> Self {
    let _ = oheap().lock().incr(data_ref);
    DataObject { data_ref }
  }

  pub fn incr(&self) {
    let _ = oheap().lock().incr(self.data_ref);
  }

  pub fn decr(&self) {
    let _ = oheap().lock().decr(self.data_ref);
  }

  // --- Serialization / Deserialization ---
  pub fn from_string(s: &str) -> Self {
    #[cfg(feature = "serde_support")]
    {
      let value = serde_json::from_str(s)
      .expect("Failed to parse JSON string in DataObject::from_string");
      DataObject::from_json(value)
    }
    #[cfg(not(feature = "serde_support"))]
    {
      json_util::object_from_string(s)
      .expect("Failed to parse JSON string using json_util in DataObject::from_string")
    }
  }

  pub fn to_string(&self) -> String {
    #[cfg(feature = "serde_support")]
    {
      self.to_json().to_string()
    }
    #[cfg(not(feature = "serde_support"))]
    {
      json_util::object_to_string(self.clone())
    }
  }

  #[cfg(feature = "serde_support")]
  pub fn from_json(value: Value) -> Self {
    let json_obj = value
    .as_object()
    .expect("DataObject::from_json requires a JSON object Value");

    let mut data_obj = DataObject::new();
    for (key, val) in json_obj.iter() {
      match val {
        Value::String(s) => data_obj.put_string(key, s),
        Value::Bool(b) => data_obj.put_boolean(key, *b),
        Value::Number(n) if n.is_i64() => data_obj.put_int(key, n.as_i64().unwrap()),
        Value::Number(n) if n.is_f64() => data_obj.put_float(key, n.as_f64().unwrap()),
        Value::Number(n) if n.is_u64() => data_obj.put_int(key, n.as_u64().unwrap() as i64),
        Value::Object(_) => data_obj.put_object(key, DataObject::from_json(val.clone())),
        Value::Array(_) => data_obj.put_array(key, DataArray::from_json(val.clone())),
        Value::Null => data_obj.put_null(key),
        _ => {
          #[cfg(not(feature = "no_std_support"))]
          println!("Warning: Unknown JSON type encountered for key '{}': {}", key, val);
        }
      }
    }
    data_obj
  }

  #[cfg(feature = "serde_support")]
  pub fn to_json(&self) -> Value {
    let mut map = serde_json::Map::new();
    let items_to_convert: Vec<(String, Data)> = {
      let heap = &mut oheap().lock();
      let data_map = heap.get(self.data_ref);
      data_map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    };

    for (key, data_value) in items_to_convert {
      let json_value = match data_value {
        Data::DInt(i) => json!(i),
        Data::DFloat(f) => json!(f),
        Data::DBoolean(b) => json!(b),
        Data::DString(s) => json!(s),
        Data::DObject(obj_ref) => DataObject::get(obj_ref).to_json(),
        Data::DArray(arr_ref) => DataArray::get(arr_ref).to_json(),
        Data::DBytes(bytes_ref) => json!(DataBytes::get(bytes_ref).to_hex_string()),
        Data::DNull => json!(null),
      };
      map.insert(key, json_value);
    }
    Value::Object(map)
  }


  // --- Copying ---
  #[deprecated(since = "0.3.0", note = "please use `clone` instead")]
  pub fn duplicate(&self) -> Self {
    self.clone()
  }

  pub fn shallow_copy(&self) -> Self {
    let mut new_obj = DataObject::new();
    for (key, value) in self.objects() {
      new_obj.set_property(&key, value);
    }
    new_obj
  }

  pub fn deep_copy(&self) -> Self {
    let mut new_obj = DataObject::new();
    for (key, value) in self.objects() {
      match value {
        Data::DObject(obj_ref) => {
          let nested_obj = DataObject::get(obj_ref);
          new_obj.put_object(&key, nested_obj.deep_copy());
        }
        Data::DArray(arr_ref) => {
          let nested_arr = DataArray::get(arr_ref);
          new_obj.put_array(&key, nested_arr.deep_copy());
        }
        Data::DBytes(bytes_ref) => {
          let nested_bytes = DataBytes::get(bytes_ref);
          new_obj.put_bytes(&key, nested_bytes.deep_copy());
        }
        _ => {
          new_obj.set_property(&key, value);
        }
      }
    }
    new_obj
  }

  // --- Accessors ---
  pub fn has(&self, key: &str) -> bool {
    let heap = &mut oheap().lock();
    heap.get(self.data_ref).contains_key(key)
  }

  pub fn keys(self) -> Vec<String> {
    let heap = &mut oheap().lock();
    let map = heap.get(self.data_ref);
    map.keys().cloned().collect()
  }

  pub fn get_property(&self, key: &str) -> Data {
    let heap = &mut oheap().lock();
    let map = heap.get(self.data_ref);
    map.get(key).cloned().unwrap_or_else(|| {
      panic!(
        "DataObject::get_property failed: Key '{}' not found in object at ref {}",
        key, self.data_ref
      );
    })
  }

  pub fn get_string(&self, key: &str) -> String { self.get_property(key).string() }
  #[deprecated(since = "0.3.0", note = "please use `get_boolean` instead")]
  pub fn get_bool(&self, key: &str) -> bool { self.get_boolean(key) }
  #[deprecated(since = "0.3.0", note = "please use `get_int` instead")]
  pub fn get_i64(&self, key: &str) -> i64 { self.get_int(key) }
  #[deprecated(since = "0.3.0", note = "please use `get_float` instead")]
  pub fn get_f64(&self, key: &str) -> f64 { self.get_float(key) }
  pub fn get_boolean(&self, key: &str) -> bool { self.get_property(key).boolean() }
  pub fn get_int(&self, key: &str) -> i64 { self.get_property(key).int() }
  pub fn get_float(&self, key: &str) -> f64 {
    let d = self.get_property(key);
    if d.is_int() { d.int() as f64 } else { d.float() }
  }
  pub fn get_object(&self, key: &str) -> DataObject { self.get_property(key).object() }
  pub fn get_array(&self, key: &str) -> DataArray { self.get_property(key).array() }
  pub fn get_bytes(&self, key: &str) -> DataBytes { self.get_property(key).bytes() }

  // --- Mutators ---
  pub fn remove_property(&mut self, key: &str) {
    let old_data_opt = {
      let heap = &mut oheap().lock();
      let map = heap.get(self.data_ref);
      map.remove(key)
    };

    if let Some(old_data) = old_data_opt {
      match old_data {
        Data::DObject(i) => { let _ = DataObject { data_ref: i }; }
        Data::DArray(i) => { let _ = DataArray { data_ref: i }; }
        Data::DBytes(i) => { let _ = DataBytes { data_ref: i }; }
        _ => {}
      }
    }
  }

  /// Sets or replaces the value for the given key. Handles ref counts correctly
  /// and acquires locks in a consistent order (oheap -> aheap -> bheap if needed).
  pub fn set_property(&mut self, key: &str, data: Data) {
    // Step 1 & 2: Acquire necessary locks and increment ref count for the *new* data.
    // Lock order: oheap -> aheap -> bheap (only acquire locks actually needed).
    let old_data_opt = match &data {
      Data::DObject(i) => {
        // Only need oheap lock for DObject
        let oheap_guard = &mut oheap().lock();
        oheap_guard.incr(*i);
        let map = oheap_guard.get(self.data_ref);
        map.insert(key.to_string(), data)
        // oheap lock released here
      }
      Data::DArray(i) => {
        // Need oheap (for map insert) and aheap (for incr)
        // Acquire in consistent order: oheap -> aheap
        let oheap_guard = &mut oheap().lock();
        { // Scope for aheap lock
          let aheap_guard = &mut dataarray::aheap().lock();
          aheap_guard.incr(*i);
        } // aheap lock released here
        let map = oheap_guard.get(self.data_ref);
        map.insert(key.to_string(), data)
        // oheap lock released here
      }
      Data::DBytes(i) => {
        // Need oheap (for map insert) and bheap (for incr)
        // Acquire in consistent order: oheap -> bheap (assuming this is the chosen global order)
        let oheap_guard = &mut oheap().lock();
        { // Scope for bheap lock
          let bheap_guard = &mut databytes::bheap().lock();
          bheap_guard.incr(*i);
        } // bheap lock released here
        let map = oheap_guard.get(self.data_ref);
        map.insert(key.to_string(), data)
        // oheap lock released here
      }
      // Primitive types: only need oheap lock for insertion
      _ => {
        let oheap_guard = &mut oheap().lock();
        let map = oheap_guard.get(self.data_ref);
        map.insert(key.to_string(), data)
        // oheap lock released here
      }
    };

    // Step 3: Queue the *old* data for GC decrement (if applicable) AFTER insertion.
    // This doesn't require locks here; the Drop impl handles queuing.
    if let Some(old_data) = old_data_opt {
      match old_data {
        Data::DObject(i) => { let _ = DataObject { data_ref: i }; }
        Data::DArray(i) => { let _ = DataArray { data_ref: i }; }
        Data::DBytes(i) => { let _ = DataBytes { data_ref: i }; }
        _ => {}
      }
    }
  }


  #[deprecated(since = "0.3.0", note = "please use `put_string` instead")]
  pub fn put_str(&mut self, key: &str, val: &str) { self.put_string(key, val); }
  #[deprecated(since = "0.3.0", note = "please use `put_boolean` instead")]
  pub fn put_bool(&mut self, key: &str, val: bool) { self.put_boolean(key, val); }
  #[deprecated(since = "0.3.0", note = "please use `put_int` instead")]
  pub fn put_i64(&mut self, key: &str, val: i64) { self.put_int(key, val); }
  pub fn put_string(&mut self, key: &str, val: &str) { self.set_property(key, Data::DString(val.to_string())); }
  pub fn put_boolean(&mut self, key: &str, val: bool) { self.set_property(key, Data::DBoolean(val)); }
  pub fn put_int(&mut self, key: &str, val: i64) { self.set_property(key, Data::DInt(val)); }
  pub fn put_float(&mut self, key: &str, val: f64) { self.set_property(key, Data::DFloat(val)); }
  /// Sets the value to the provided `DataObject`. Takes ownership (semantically).
  pub fn put_object(&mut self, key: &str, o: DataObject) {
    // set_property handles locking and ref counting
    self.set_property(key, Data::DObject(o.data_ref));
    // Prevent drop impl of 'o' from queueing its ref again
    core::mem::forget(o);
  }
  #[deprecated(since = "0.1.2", note = "please use `put_array` instead")]
  pub fn put_list(&mut self, key: &str, a: DataArray) { self.put_array(key, a); }
  /// Sets the value to the provided `DataArray`. Takes ownership (semantically).
  pub fn put_array(&mut self, key: &str, a: DataArray) {
    // set_property handles locking and ref counting
    self.set_property(key, Data::DArray(a.data_ref));
    // Prevent drop impl of 'a' from queueing its ref again
    core::mem::forget(a);
  }
  /// Sets the value to the provided `DataBytes`. Takes ownership (semantically).
  pub fn put_bytes(&mut self, key: &str, b: DataBytes) {
    // set_property handles locking and ref counting
    self.set_property(key, Data::DBytes(b.data_ref));
    // Prevent drop impl of 'b' from queueing its ref again
    core::mem::forget(b);
  }
  pub fn put_null(&mut self, key: &str) { self.set_property(key, Data::DNull); }

  // --- Internal GC Helper ---
  pub(crate) fn delete(
    oheap: &mut Heap<HashMap<String, Data>>,
    data_ref: usize,
    aheap: &mut Heap<Vec<Data>>,
  ) {
    if oheap.count(data_ref) == 0 {
      #[cfg(not(feature = "no_std_support"))]
      println!("Warning: DataObject::delete called on ref {} with count 0", data_ref);
      return;
    }

    let current_count = oheap.count(data_ref);

    if current_count == 1 {
      let mut objects_to_kill = Vec::<usize>::new();
      let mut arrays_to_kill = Vec::<usize>::new();

      let map = oheap.get(data_ref);
      for value in map.values() {
        match value {
          Data::DObject(i) => objects_to_kill.push(*i),
          Data::DArray(i) => arrays_to_kill.push(*i),
          Data::DBytes(_) => {} // Ignore DataBytes refs for recursive delete
          _ => {}
        }
      }

      oheap.decr(data_ref);

      for i in objects_to_kill {
        DataObject::delete(oheap, i, aheap);
      }
      for i in arrays_to_kill {
        dataarray::DataArray::delete(aheap, i, oheap);
      }

    } else if current_count > 1 {
      oheap.decr(data_ref);
    }
  }


  // --- Utility / Debug ---
  pub fn objects(&self) -> Vec<(String, Data)> {
    let heap = &mut oheap().lock();
    let map = heap.get(self.data_ref);
    map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
  }

  #[cfg(not(feature = "no_std_support"))]
  pub fn print_heap() {
    println!("Object Heap Keys: {:?}", oheap().lock().keys());
  }

  // --- Garbage Collection ---
  pub fn gc() {
    // Lock heaps in consistent order: oheap -> aheap
    let oheap_guard = &mut oheap().lock();
    let aheap_guard = &mut dataarray::aheap().lock();
    let odrop_guard = &mut odrop().lock();

    for data_ref in odrop_guard.drain(..) {
      DataObject::delete(&mut *oheap_guard, data_ref, &mut *aheap_guard);
    }
  }
}

// --- Drop Implementation ---
impl Drop for DataObject {
  fn drop(&mut self) {
    let _ = odrop().lock().push(self.data_ref);
  }
}
