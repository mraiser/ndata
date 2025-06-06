//! This module defines the `DataArray` struct, a thread-safe, reference-counted
//! list-like data structure (`Vec<Data>`) stored in a shared heap.

// Ensure code works in no_std environments if the feature is enabled.
#![cfg_attr(feature = "no_std_support", no_std)]

// Necessary imports from the standard library (or alloc crate for no_std).
extern crate alloc;
// Use std types when available (default)
#[cfg(not(feature = "no_std_support"))]
use std::collections::HashMap; // Needed for DataObject::delete call
#[cfg(not(feature = "no_std_support"))]
use std::println;

// Use alloc types when only alloc is available and no_std_support is enabled
#[cfg(feature = "no_std_support")]
use alloc::collections::HashMap; // Needed for DataObject::delete call

use alloc::string::{String, ToString};
use alloc::vec::Vec;
// Removed: use alloc::boxed::Box;


// Imports from other modules within the ndata crate.
use crate::data::*;
use crate::dataobject::{self, DataObject}; // Import module and struct
use crate::databytes::{self, DataBytes};   // Import module and struct (needed for incr/decr)
use crate::heap::*;
use crate::sharedmutex::*;

// Conditional imports based on the `serde_support` feature flag.
#[cfg(feature = "serde_support")]
use serde_json::{json, Value};
#[cfg(not(feature = "serde_support"))]
use crate::json_util; // Import the module directly


// --- NDataError Definition ---
// This would ideally be in a shared error.rs file if used across the ndata crate.
// For this exercise, it's redefined here. Ensure it's consistent with DataObject's NDataError.
#[derive(Debug)]
pub enum NDataError {
    KeyNotFound(String), // Used by DataObject, less relevant for DataArray's indexed access
    IndexOutOfBounds {
        index: usize,
        len: usize,
    },
    WrongDataType {
        // For DataArray, 'key' might be an index conceptually
        index: usize,
        expected: &'static str,
        found: &'static str,
    },
    InvalidArrayRef, // Specific error if the DataArray handle itself is stale
}

impl core::fmt::Display for NDataError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            NDataError::KeyNotFound(key) => write!(f, "Key not found: '{}'", key), // Retain for consistency
            NDataError::IndexOutOfBounds { index, len } => {
                write!(f, "Index out of bounds: index is {}, but length is {}", index, len)
            }
            NDataError::WrongDataType { index, expected, found } => {
                write!(f, "Wrong data type at index {}: expected {}, found {}", index, expected, found)
            }
            NDataError::InvalidArrayRef => write!(f, "DataArray reference is invalid or points to deallocated memory"),
        }
    }
}

// Implement std::error::Error only if std is available and it's not a no_std build.
#[cfg(not(feature = "no_std_support"))]
impl std::error::Error for NDataError {}


// --- Global Static Heaps ---

/// Global storage heap for `DataArray` instances (Vec<Data>).
/// Uses a custom `SharedMutex` for thread-safe access, potentially across processes.
static mut ARRAY_HEAP: SharedMutex<Heap<Vec<Data>>> = SharedMutex::new();

/// Global queue for `DataArray` references (`usize`) whose reference counts
/// should be decremented during the next garbage collection cycle (`DataArray::gc()`).
static mut ARRAY_DROP_QUEUE: SharedMutex<Vec<usize>> = SharedMutex::new();

// --- Heap Accessor Functions ---

/// Provides mutable access to the global `DataArray` heap (`ARRAY_HEAP`).
#[doc(hidden)]
pub(crate) fn aheap() -> &'static mut SharedMutex<Heap<Vec<Data>>> {
    #[allow(static_mut_refs)]
    unsafe { &mut ARRAY_HEAP }
}

/// Provides mutable access to the global `DataArray` drop queue (`ARRAY_DROP_QUEUE`).
fn adrop() -> &'static mut SharedMutex<Vec<usize>> {
    #[allow(static_mut_refs)]
    unsafe { &mut ARRAY_DROP_QUEUE }
}

// --- DataArray Definition ---
#[derive(Debug, Default)]
pub struct DataArray {
    pub data_ref: usize,
}

// --- Clone Implementation ---
impl Clone for DataArray {
    fn clone(&self) -> Self {
        let _ = aheap().lock().incr(self.data_ref);
        DataArray {
            data_ref: self.data_ref,
        }
    }
}

// --- Core Functionality ---
impl DataArray {
    #[allow(static_mut_refs)]
    pub fn init() -> ((u64, u64), (u64, u64)) {
        unsafe {
            if !ARRAY_HEAP.is_initialized() {
                ARRAY_HEAP.set(Heap::new());
                ARRAY_DROP_QUEUE.set(Vec::new());
            }
        }
        Self::share()
    }

    #[allow(static_mut_refs)]
    pub fn share() -> ((u64, u64), (u64, u64)) {
        unsafe {
            let q = ARRAY_HEAP.share();
            let r = ARRAY_DROP_QUEUE.share();
            (q, r)
        }
    }

    #[allow(static_mut_refs)]
    pub fn mirror(q: (u64, u64), r: (u64, u64)) {
        unsafe {
            ARRAY_HEAP.mirror(q.0, q.1);
            ARRAY_DROP_QUEUE.mirror(r.0, r.1);
        }
    }

    pub fn new() -> DataArray {
        let data_ref = aheap().lock().push(Vec::<Data>::new());
        DataArray { data_ref }
    }

    pub fn get(data_ref: usize) -> DataArray {
        let _ = aheap().lock().incr(data_ref);
        DataArray { data_ref }
    }

    pub fn incr(&self) {
        let _ = aheap().lock().incr(self.data_ref);
    }

    pub fn decr(&self) {
        let _ = aheap().lock().decr(self.data_ref);
    }

    // --- Serialization / Deserialization ---
    pub fn from_string(s: &str) -> DataArray {
        #[cfg(feature = "serde_support")]
        {
            let value = serde_json::from_str(s)
                .expect("Failed to parse JSON string in DataArray::from_string");
            DataArray::from_json(value)
        }
        #[cfg(not(feature = "serde_support"))]
        {
            json_util::array_from_string(s)
                .expect("Failed to parse JSON string using json_util in DataArray::from_string")
        }
    }

    pub fn to_string(&self) -> String {
        if !aheap().lock().contains_key(self.data_ref) {
            #[cfg(not(feature = "no_std_support"))]
            println!("Warning: to_string called on invalid DataArray ref {}", self.data_ref);
            return "[]".to_string();
        }
        #[cfg(feature = "serde_support")]
        {
            self.to_json().to_string()
        }
        #[cfg(not(feature = "serde_support"))]
        {
            json_util::array_to_string(self.clone())
        }
    }

    #[cfg(feature = "serde_support")]
    pub fn from_json(value: Value) -> DataArray {
        let json_arr = value
            .as_array()
            .expect("DataArray::from_json requires a JSON array Value");

        let mut data_arr = DataArray::new();
        for val in json_arr.iter() {
            match val {
                Value::String(s) => data_arr.push_string(s),
                Value::Bool(b) => data_arr.push_boolean(*b),
                Value::Number(n) if n.is_i64() => data_arr.push_int(n.as_i64().unwrap()),
                Value::Number(n) if n.is_f64() => data_arr.push_float(n.as_f64().unwrap()),
                Value::Number(n) if n.is_u64() => data_arr.push_int(n.as_u64().unwrap() as i64),
                Value::Object(_) => data_arr.push_object(DataObject::from_json(val.clone())),
                Value::Array(_) => data_arr.push_array(DataArray::from_json(val.clone())),
                Value::Null => data_arr.push_null(),
                _ => {
                    #[cfg(not(feature = "no_std_support"))]
                    println!("Warning: Unknown JSON type encountered in array: {}", val);
                }
            }
        }
        data_arr
    }

    #[cfg(feature = "serde_support")]
    pub fn to_json(&self) -> Value {
        if !aheap().lock().contains_key(self.data_ref) {
             #[cfg(not(feature = "no_std_support"))]
             println!("Warning: to_json called on invalid DataArray ref {}", self.data_ref);
             return json!([]);
        }
        let items_to_convert = self.objects();
        let mut json_vec = Vec::with_capacity(items_to_convert.len());

        for data_value in items_to_convert {
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
            json_vec.push(json_value);
        }
        json!(json_vec)
    }


    // --- Copying ---
    #[deprecated(since = "0.3.0", note = "please use `clone` instead")]
    pub fn duplicate(&self) -> DataArray {
        self.clone()
    }

    pub fn shallow_copy(&self) -> DataArray {
        let mut new_arr = DataArray::new();
        if !aheap().lock().contains_key(self.data_ref) {
            #[cfg(not(feature = "no_std_support"))]
            println!("Warning: shallow_copy called on invalid DataArray ref {}", self.data_ref);
            return new_arr;
        }
        for v in self.objects() {
            new_arr.push_property(v);
        }
        new_arr
    }

    pub fn deep_copy(&self) -> DataArray {
        let mut new_arr = DataArray::new();
        if !aheap().lock().contains_key(self.data_ref) {
            #[cfg(not(feature = "no_std_support"))]
            println!("Warning: deep_copy called on invalid DataArray ref {}", self.data_ref);
            return new_arr;
        }
        let items_to_copy = self.objects();

        for data_value in items_to_copy {
            match data_value {
                Data::DObject(obj_ref) => {
                    let nested_obj = DataObject::get(obj_ref);
                    new_arr.push_object(nested_obj.deep_copy());
                }
                Data::DArray(arr_ref) => {
                    let nested_arr = DataArray::get(arr_ref);
                    new_arr.push_array(nested_arr.deep_copy());
                }
                Data::DBytes(bytes_ref) => {
                    let nested_bytes = DataBytes::get(bytes_ref);
                    new_arr.push_bytes(nested_bytes.deep_copy());
                }
                _ => {
                    new_arr.push_property(data_value);
                }
            }
        }
        new_arr
    }

    // --- Accessors ---
    pub fn len(&self) -> usize {
        let heap_guard = &mut aheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            #[cfg(not(feature = "no_std_support"))]
            println!("Warning: len() called on invalid DataArray ref {}", self.data_ref);
            return 0;
        }
        let vec = heap_guard.get(self.data_ref);
        vec.len()
    }

    pub fn index_of(&self, b: Data) -> i64 {
        let heap_guard = &mut aheap().lock();
         if !heap_guard.contains_key(self.data_ref) {
            #[cfg(not(feature = "no_std_support"))]
            println!("Warning: index_of called on invalid DataArray ref {}", self.data_ref);
            return -1;
        }
        let vec = heap_guard.get(self.data_ref);
        vec.iter().position(|d| Data::equals(d.clone(), b.clone())).map_or(-1, |i| i as i64)
    }

    pub fn push_unique(&mut self, b: Data) -> bool {
        let initial_check_exists = { // Scope for initial read-only borrow of vec
            let aheap_guard = &mut aheap().lock();
            if !aheap_guard.contains_key(self.data_ref) {
                #[cfg(not(feature = "no_std_support"))]
                println!("Warning: push_unique target array (ref {}) does not exist in heap.", self.data_ref);
                return false;
            }
            let vec = aheap_guard.get(self.data_ref);
            vec.iter().any(|d| Data::equals(d.clone(), b.clone()))
        };

        if initial_check_exists {
            return false; // Already exists
        }

        // Item not found in initial check. Now proceed with locking and incrementing.
        // The original `b: Data` is passed by value.
        match &b {
            Data::DObject(obj_ref_val) => {
                let _ = dataobject::oheap().lock().incr(*obj_ref_val);
                // Now push, with a fresh lock on aheap
                let aheap_guard = &mut aheap().lock();
                // Check self.data_ref validity again inside the new lock scope if paranoid,
                // though it was checked above.
                if !aheap_guard.contains_key(self.data_ref) { // Should be rare if passed first check
                    #[cfg(not(feature = "no_std_support"))]
                    println!("Warning: push_unique target array (ref {}) disappeared before push.", self.data_ref);
                    dataobject::oheap().lock().decr(*obj_ref_val); // Rollback incr
                    return false;
                }
                let target_vec = aheap_guard.get(self.data_ref);
                // Double check for race condition
                if !target_vec.iter().any(|d_in_target| Data::equals(d_in_target.clone(), b.clone())) {
                    target_vec.push(b);
                    return true;
                } else {
                    dataobject::oheap().lock().decr(*obj_ref_val); // Rollback incr
                    return false;
                }
            }
            Data::DArray(arr_ref_val) => {
                // The aheap is about to be locked for `incr` and then for `get` and `push`.
                // The critical point is that `aheap().lock().incr()` and `aheap().lock().get()`
                // are distinct operations on the *same lock guard* if structured poorly.
                // Here, we do incr, then get a new guard for the push.

                // It's better to perform incr *within* the same lock guard scope as the push,
                // but ensure no conflicting borrows.
                let aheap_guard = &mut aheap().lock();
                if !aheap_guard.contains_key(self.data_ref) {
                     #[cfg(not(feature = "no_std_support"))]
                     println!("Warning: push_unique target array (ref {}) disappeared before DArray push.", self.data_ref);
                    return false; // `b`'s count not incremented yet.
                }

                aheap_guard.incr(*arr_ref_val); // Increment count of array 'b'

                let target_vec_for_push = aheap_guard.get(self.data_ref); // Get target vec under same lock

                if !target_vec_for_push.iter().any(|d_in_target| Data::equals(d_in_target.clone(), b.clone())) {
                    target_vec_for_push.push(b);
                    return true;
                } else {
                    aheap_guard.decr(*arr_ref_val);
                    return false;
                }
            }
            Data::DBytes(bytes_ref_val) => {
                let _ = databytes::bheap().lock().incr(*bytes_ref_val);
                let aheap_guard = &mut aheap().lock();
                 if !aheap_guard.contains_key(self.data_ref) {
                    #[cfg(not(feature = "no_std_support"))]
                    println!("Warning: push_unique target array (ref {}) disappeared before DBytes push.", self.data_ref);
                    databytes::bheap().lock().decr(*bytes_ref_val);
                    return false;
                }
                let target_vec = aheap_guard.get(self.data_ref);
                if !target_vec.iter().any(|d_in_target| Data::equals(d_in_target.clone(), b.clone())) {
                    target_vec.push(b);
                    return true;
                } else {
                    databytes::bheap().lock().decr(*bytes_ref_val);
                    return false;
                }
            }
            _ => { // Primitive types
                let aheap_guard = &mut aheap().lock();
                if !aheap_guard.contains_key(self.data_ref) {
                    #[cfg(not(feature = "no_std_support"))]
                    println!("Warning: push_unique target array (ref {}) disappeared before primitive push.", self.data_ref);
                    return false;
                }
                let target_vec = aheap_guard.get(self.data_ref);
                // Double check for primitives is still good due to race potential
                if !target_vec.iter().any(|d_in_target| Data::equals(d_in_target.clone(), b.clone())) {
                    target_vec.push(b);
                    return true;
                } else {
                    return false;
                }
            }
        }
    }


    pub fn remove_data(&mut self, b: Data) -> bool {
        let aheap_guard = &mut aheap().lock();
        if !aheap_guard.contains_key(self.data_ref) {
            #[cfg(not(feature = "no_std_support"))]
            println!("Warning: remove_data target array (ref {}) does not exist in heap.", self.data_ref);
            return false;
        }
        let vec = aheap_guard.get(self.data_ref);

        let index_opt = vec.iter().position(|d| Data::equals(d.clone(), b.clone()));

        if let Some(index) = index_opt {
            let old_data = vec.remove(index);
            match old_data {
                Data::DObject(i) => { let _ = DataObject { data_ref: i }; }
                Data::DArray(i) => { let _ = DataArray { data_ref: i }; }
                Data::DBytes(i) => { let _ = DataBytes { data_ref: i }; }
                _ => {}
            }
            true
        } else {
            false
        }
    }

    pub fn get_property(&self, id: usize) -> Data {
        let heap_guard = &mut aheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            panic!("DataArray::get_property failed: Array ref {} not found in heap", self.data_ref);
        }
        let vec = heap_guard.get(self.data_ref);
        if id >= vec.len() {
            panic!("Index out of bounds in DataArray::get_property: index {}, len {}", id, vec.len());
        }
        vec[id].clone()
    }

    // --- New `try_get_` methods ---
    pub fn try_get_property(&self, index: usize) -> Result<Data, NDataError> {
        let heap_guard = &mut aheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            return Err(NDataError::InvalidArrayRef);
        }
        let vec = heap_guard.get(self.data_ref);
        if index >= vec.len() {
            Err(NDataError::IndexOutOfBounds { index, len: vec.len() })
        } else {
            Ok(vec[index].clone())
        }
    }

    pub fn try_get_string(&self, index: usize) -> Result<String, NDataError> {
        match self.try_get_property(index)? {
            Data::DString(s) => Ok(s),
            other => Err(NDataError::WrongDataType {
                index,
                expected: "string",
                found: other.type_name_owned(),
            }),
        }
    }

    pub fn try_get_boolean(&self, index: usize) -> Result<bool, NDataError> {
        match self.try_get_property(index)? {
            Data::DBoolean(b) => Ok(b),
            other => Err(NDataError::WrongDataType {
                index,
                expected: "boolean",
                found: other.type_name_owned(),
            }),
        }
    }

    pub fn try_get_int(&self, index: usize) -> Result<i64, NDataError> {
        match self.try_get_property(index)? {
            Data::DInt(i) => Ok(i),
            other => Err(NDataError::WrongDataType {
                index,
                expected: "int",
                found: other.type_name_owned(),
            }),
        }
    }

    pub fn try_get_float(&self, index: usize) -> Result<f64, NDataError> {
        match self.try_get_property(index)? {
            Data::DFloat(f) => Ok(f),
            Data::DInt(i) => Ok(i as f64),
            other => Err(NDataError::WrongDataType {
                index,
                expected: "float (or int)",
                found: other.type_name_owned(),
            }),
        }
    }

    pub fn try_get_object(&self, index: usize) -> Result<DataObject, NDataError> {
        match self.try_get_property(index)? {
            Data::DObject(obj_ref) => Ok(DataObject::get(obj_ref)),
            other => Err(NDataError::WrongDataType {
                index,
                expected: "DataObject",
                found: other.type_name_owned(),
            }),
        }
    }

    pub fn try_get_array(&self, index: usize) -> Result<DataArray, NDataError> {
        match self.try_get_property(index)? {
            Data::DArray(arr_ref) => Ok(DataArray::get(arr_ref)),
            other => Err(NDataError::WrongDataType {
                index,
                expected: "DataArray",
                found: other.type_name_owned(),
            }),
        }
    }

    pub fn try_get_bytes(&self, index: usize) -> Result<DataBytes, NDataError> {
        match self.try_get_property(index)? {
            Data::DBytes(bytes_ref) => Ok(DataBytes::get(bytes_ref)),
            other => Err(NDataError::WrongDataType {
                index,
                expected: "DataBytes",
                found: other.type_name_owned(),
            }),
        }
    }

    // --- Simple Getters (delegate to get_property) ---
    pub fn get_string(&self, id: usize) -> String { self.get_property(id).string() }
    #[deprecated(since = "0.3.0", note = "please use `get_boolean` instead")]
    pub fn get_bool(&self, id: usize) -> bool { self.get_boolean(id) }
    #[deprecated(since = "0.3.0", note = "please use `get_int` instead")]
    pub fn get_i64(&self, id: usize) -> i64 { self.get_int(id) }
    #[deprecated(since = "0.3.0", note = "please use `get_float` instead")]
    pub fn get_f64(&self, id: usize) -> f64 { self.get_float(id) }
    pub fn get_boolean(&self, id: usize) -> bool { self.get_property(id).boolean() }
    pub fn get_int(&self, id: usize) -> i64 { self.get_property(id).int() }
    pub fn get_float(&self, id: usize) -> f64 {
        let d = self.get_property(id);
        if d.is_int() { d.int() as f64 } else { d.float() }
    }
    pub fn get_array(&self, id: usize) -> DataArray { self.get_property(id).array() }
    pub fn get_object(&self, id: usize) -> DataObject { self.get_property(id).object() }
    pub fn get_bytes(&self, id: usize) -> DataBytes { self.get_property(id).bytes() }

    // --- Mutators ---
    pub fn join(&mut self, a: DataArray) {
        if !aheap().lock().contains_key(self.data_ref) {
            #[cfg(not(feature = "no_std_support"))]
            println!("Warning: join target array (ref {}) does not exist in heap.", self.data_ref);
            return;
        }
        if !aheap().lock().contains_key(a.data_ref) {
             #[cfg(not(feature = "no_std_support"))]
            println!("Warning: join source array (ref {}) does not exist in heap.", a.data_ref);
            return;
        }

        let items_to_join = a.objects();
        for val in items_to_join {
            self.push_property(val);
        }
    }

    pub fn push_property(&mut self, data: Data) {
        match &data {
            Data::DObject(i) => { dataobject::oheap().lock().incr(*i); }
            Data::DArray(i) => { aheap().lock().incr(*i); }
            Data::DBytes(i) => { databytes::bheap().lock().incr(*i); }
            _ => {}
        }

        let heap_guard = &mut aheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            #[cfg(not(feature = "no_std_support"))]
            println!("Warning: push_property target array (ref {}) does not exist in heap.", self.data_ref);
            match &data { // Rollback increment if push fails
                Data::DObject(i) => { dataobject::oheap().lock().decr(*i); }
                Data::DArray(i) => { aheap().lock().decr(*i); }
                Data::DBytes(i) => { databytes::bheap().lock().decr(*i); }
                _ => {}
            }
            return;
        }
        let vec = heap_guard.get(self.data_ref);
        vec.push(data);
    }

    // --- Simple Pushers (delegate to push_property) ---
    #[deprecated(since = "0.3.0", note = "please use `push_string` instead")]
    pub fn push_str(&mut self, val: &str) { self.push_string(val); }
    #[deprecated(since = "0.3.0", note = "please use `push_boolean` instead")]
    pub fn push_bool(&mut self, val: bool) { self.push_boolean(val); }
    #[deprecated(since = "0.3.0", note = "please use `push_int` instead")]
    pub fn push_i64(&mut self, val: i64) { self.push_int(val); }
    pub fn push_string(&mut self, val: &str) { self.push_property(Data::DString(val.to_string())); }
    pub fn push_boolean(&mut self, val: bool) { self.push_property(Data::DBoolean(val)); }
    pub fn push_int(&mut self, val: i64) { self.push_property(Data::DInt(val)); }
    pub fn push_float(&mut self, val: f64) { self.push_property(Data::DFloat(val)); }
    pub fn push_object(&mut self, o: DataObject) {
        self.push_property(Data::DObject(o.data_ref));
    }
    #[deprecated(since = "0.1.2", note = "please use `push_array` instead")]
    pub fn push_list(&mut self, a: DataArray) { self.push_array(a); }
    pub fn push_array(&mut self, a: DataArray) {
        self.push_property(Data::DArray(a.data_ref));
    }
    pub fn push_bytes(&mut self, b: DataBytes) {
        self.push_property(Data::DBytes(b.data_ref));
    }
    pub fn push_null(&mut self) { self.push_property(Data::DNull); }

    pub fn set_property(&mut self, id: usize, data: Data) {
        match &data {
            Data::DObject(i) => { dataobject::oheap().lock().incr(*i); }
            Data::DArray(i) => { aheap().lock().incr(*i); }
            Data::DBytes(i) => { databytes::bheap().lock().incr(*i); }
            _ => {}
        }

        let old_data_opt: Option<Data>;

        {
            let heap_guard = &mut aheap().lock();
            if !heap_guard.contains_key(self.data_ref) {
                #[cfg(not(feature = "no_std_support"))]
                println!("Warning: set_property target array (ref {}) does not exist in heap.", self.data_ref);
                match &data { // Rollback increment
                    Data::DObject(i) => { dataobject::oheap().lock().decr(*i); }
                    Data::DArray(i) => { aheap().lock().decr(*i); }
                    Data::DBytes(i) => { databytes::bheap().lock().decr(*i); }
                    _ => {}
                }
                return;
            }
            let vec = heap_guard.get(self.data_ref);
            if id >= vec.len() {
                match &data { // Rollback increment
                    Data::DObject(i) => { dataobject::oheap().lock().decr(*i); }
                    Data::DArray(i) => { aheap().lock().decr(*i); }
                    Data::DBytes(i) => { databytes::bheap().lock().decr(*i); }
                    _ => {}
                }
                panic!("Index out of bounds in DataArray::set_property: index {}, len {}", id, vec.len());
            }
            old_data_opt = Some(core::mem::replace(&mut vec[id], data));
        }

        if let Some(old_data_handle) = old_data_opt {
            match old_data_handle {
                Data::DObject(i) => { let _ = DataObject { data_ref: i }; }
                Data::DArray(i) => { let _ = DataArray { data_ref: i }; }
                Data::DBytes(i) => { let _ = DataBytes { data_ref: i }; }
                _ => {}
            }
        }
    }

    // --- Simple Setters (delegate to set_property) ---
    #[deprecated(since = "0.3.0", note = "please use `put_string` instead")]
    pub fn put_str(&mut self, id: usize, val: &str) { self.put_string(id, val); }
    #[deprecated(since = "0.3.0", note = "please use `put_boolean` instead")]
    pub fn put_bool(&mut self, id: usize, val: bool) { self.put_boolean(id, val); }
    #[deprecated(since = "0.3.0", note = "please use `put_int` instead")]
    pub fn put_i64(&mut self, id: usize, val: i64) { self.put_int(id, val); }
    pub fn put_string(&mut self, id: usize, val: &str) { self.set_property(id, Data::DString(val.to_string())); }
    pub fn put_boolean(&mut self, id: usize, val: bool) { self.set_property(id, Data::DBoolean(val)); }
    pub fn put_int(&mut self, id: usize, val: i64) { self.set_property(id, Data::DInt(val)); }
    pub fn put_float(&mut self, id: usize, val: f64) { self.set_property(id, Data::DFloat(val)); }
    pub fn put_object(&mut self, id: usize, o: DataObject) {
        self.set_property(id, Data::DObject(o.data_ref));
    }
    pub fn put_array(&mut self, id: usize, a: DataArray) {
        self.set_property(id, Data::DArray(a.data_ref));
    }
    pub fn put_bytes(&mut self, id: usize, b: DataBytes) {
        self.set_property(id, Data::DBytes(b.data_ref));
    }
    pub fn put_null(&mut self, id: usize) { self.set_property(id, Data::DNull); }


    pub fn remove_property(&mut self, id: usize) {
        let old_data = {
            let heap_guard = &mut aheap().lock();
            if !heap_guard.contains_key(self.data_ref) {
                panic!("DataArray::remove_property failed: Array ref {} not found in heap", self.data_ref);
            }
            let vec = heap_guard.get(self.data_ref);
            if id >= vec.len() {
                panic!("Index out of bounds in DataArray::remove_property: index {}, len {}", id, vec.len());
            }
            vec.remove(id)
        };

        match old_data {
            Data::DObject(i) => { let _ = DataObject { data_ref: i }; }
            Data::DArray(i) => { let _ = DataArray { data_ref: i }; }
            Data::DBytes(i) => { let _ = DataBytes { data_ref: i }; }
            _ => {}
        }
    }

    pub fn pop_property(&mut self, id: usize) -> Data {
        let old_data = {
            let heap_guard = &mut aheap().lock();
            if !heap_guard.contains_key(self.data_ref) {
                panic!("DataArray::pop_property failed: Array ref {} not found in heap", self.data_ref);
            }
            let vec = heap_guard.get(self.data_ref);
            if id >= vec.len() {
                panic!("Index out of bounds in DataArray::pop_property: index {}, len {}", id, vec.len());
            }
            vec.remove(id)
        };
        old_data
    }


    // --- Internal GC Helper ---
    pub(crate) fn delete(
        aheap_guard: &mut Heap<Vec<Data>>,
        data_ref: usize,
        oheap_guard: &mut Heap<HashMap<String, Data>>,
    ) {
        if !aheap_guard.contains_key(data_ref) {
            #[cfg(not(feature = "no_std_support"))]
            println!("Warning: DataArray::delete called on non-existent ref {}", data_ref);
            return;
        }

        let current_count = aheap_guard.count(data_ref);

        if current_count == 0 {
            #[cfg(not(feature = "no_std_support"))]
            println!("Warning: DataArray::delete called on ref {} with count 0 (after contains_key check)", data_ref);
            return;
        }

        if current_count == 1 {
            let mut objects_to_kill = Vec::<usize>::new();
            let mut arrays_to_kill = Vec::<usize>::new();

            let vec_clone = aheap_guard.get(data_ref).clone();

            for value in vec_clone.iter() {
                match value {
                    Data::DObject(i) => objects_to_kill.push(*i),
                    Data::DArray(i) => arrays_to_kill.push(*i),
                    _ => {}
                }
            }

            aheap_guard.decr(data_ref);

            for i in objects_to_kill {
                dataobject::DataObject::delete(oheap_guard, i, aheap_guard);
            }
            for i in arrays_to_kill {
                DataArray::delete(aheap_guard, i, oheap_guard);
            }

        } else if current_count > 1 {
            aheap_guard.decr(data_ref);
        }
    }

    pub fn objects(&self) -> Vec<Data> {
        let heap_guard = &mut aheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            #[cfg(not(feature = "no_std_support"))]
            println!("Warning: objects() called on invalid DataArray ref {}", self.data_ref);
            return Vec::new();
        }
        let vec = heap_guard.get(self.data_ref);
        vec.clone()
    }

    #[cfg(not(feature = "no_std_support"))]
    pub fn print_heap() {
        println!("Array Heap Keys: {:?}", aheap().lock().keys());
    }

    // --- Garbage Collection ---
    pub fn gc() {
        let mut oheap_guard = dataobject::oheap().lock();
        let mut aheap_guard = aheap().lock();
        let mut adrop_guard = adrop().lock();

        for data_ref in adrop_guard.drain(..) {
            DataArray::delete(&mut aheap_guard, data_ref, &mut oheap_guard);
        }
    }
}

// --- Drop Implementation ---
impl Drop for DataArray {
    fn drop(&mut self) {
        let _ = adrop().lock().push(self.data_ref);
    }
}
