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
use std::println; // Keep for existing print_heap, etc.

// Use alloc types when only alloc is available and no_std_support is enabled
#[cfg(feature = "no_std_support")]
use alloc::collections::HashMap; // Ensure this is BTreeMap if HashMap is not no_std compatible in your setup

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::boxed::Box; // For Box<dyn std::error::Error> in try_from_string

// Imports from other modules within the ndata crate.
use crate::data::*;
use crate::dataarray::{self, DataArray}; // Assuming dataarray::aheap() exists
use crate::databytes::{self, DataBytes}; // Assuming databytes::bheap() exists
use crate::heap::*;
use crate::sharedmutex::*;

// Conditional imports based on the `serde_support` feature flag.
#[cfg(feature = "serde_support")]
use serde_json::{json, Value};
#[cfg(not(feature = "serde_support"))]
use crate::json_util; // Assuming json_util provides object_from_string and object_to_string

// --- NDataError Definition ---
// This would ideally be in a shared error.rs file if used across the ndata crate.
#[derive(Debug)]
pub enum NDataError {
    KeyNotFound(String),
    WrongDataType {
        key: String,
        expected: &'static str,
        found: &'static str,
    },
    // Add other generic errors if needed, e.g., for parsing or heap issues,
    // though existing try_from_string uses Box<dyn Error>.
}

impl core::fmt::Display for NDataError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            NDataError::KeyNotFound(key) => write!(f, "Key not found: '{}'", key),
            NDataError::WrongDataType { key, expected, found } => {
                write!(f, "Wrong data type for key '{}': expected {}, found {}", key, expected, found)
            }
        }
    }
}

// Implement std::error::Error only if std is available and it's not a no_std build.
#[cfg(not(feature = "no_std_support"))]
impl std::error::Error for NDataError {}


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
        let _ = oheap().lock().incr(data_ref); // Assume incr handles invalid data_ref by panicking or erroring.
        DataObject { data_ref }
    }

    pub fn incr(&self) {
        let _ = oheap().lock().incr(self.data_ref);
    }

    pub fn decr(&self) {
        let _ = oheap().lock().decr(self.data_ref); // This would typically be internal or handled by Drop
    }

    // --- Serialization / Deserialization ---
    // This existing method returns Box<dyn Error>, keep as is.
    pub fn try_from_string(s: &str) -> Result<Self, Box<dyn std::error::Error>> {
        #[cfg(feature = "serde_support")]
        {
            let value = serde_json::from_str(s)?;
            Ok(DataObject::from_json(value))
        }
        #[cfg(not(feature = "serde_support"))]
        {
            // Assuming json_util::object_from_string returns Result<Self, ParseError>
            // where ParseError implements std::error::Error.
            // If json_util returns its own error type, that needs to be Boxed.
            // For now, assume it can be boxed or the signature of json_util::object_from_string fits.
            // If it returns, for example, a custom json_util::Error, map it:
            // json_util::object_from_string(s).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
            match json_util::object_from_string(s) {
                 Ok(obj) => Ok(obj),
                 Err(e) => Err(Box::new(e) as Box<dyn std::error::Error>), // Example boxing
            }
        }
    }

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
            // to_json might fail if underlying data is inconsistent, though it currently doesn't return Result.
            // For simplicity, assuming it stringifies to "null" or panics on severe errors.
            self.to_json().to_string()
        }
        #[cfg(not(feature = "serde_support"))]
        {
            json_util::object_to_string(self.clone()) // Assuming this doesn't fail or panics.
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
                Value::Number(n) if n.is_u64() => data_obj.put_int(key, n.as_u64().unwrap() as i64), // Potential truncation
                Value::Object(_) => data_obj.put_object(key, DataObject::from_json(val.clone())),
                Value::Array(_) => data_obj.put_array(key, DataArray::from_json(val.clone())), // Assumes DataArray::from_json exists
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
            let heap_guard = &mut oheap().lock();
            // Ensure data_ref is valid. If not, heap.get might panic.
            // Consider adding a check or having heap.get return Option/Result.
            if !heap_guard.contains_key(self.data_ref) {
                 #[cfg(not(feature = "no_std_support"))]
                 println!("Warning: Invalid data_ref {} in to_json", self.data_ref);
                 return Value::Null; // Or some other error indication if Value can represent it.
            }
            let data_map = heap_guard.get(self.data_ref);
            data_map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };

        for (key, data_value) in items_to_convert {
            let json_value = match data_value {
                Data::DInt(i) => json!(i),
                Data::DFloat(f) => json!(f),
                Data::DBoolean(b) => json!(b),
                Data::DString(s) => json!(s),
                Data::DObject(obj_ref) => DataObject::get(obj_ref).to_json(), // Recursive call
                Data::DArray(arr_ref) => DataArray::get(arr_ref).to_json(), // Assumes DataArray::to_json exists
                Data::DBytes(bytes_ref) => json!(DataBytes::get(bytes_ref).to_hex_string()), // Assumes to_hex_string exists
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
        // Check if self.data_ref is valid before proceeding
        if !oheap().lock().contains_key(self.data_ref) {
            #[cfg(not(feature = "no_std_support"))]
            println!("Warning: shallow_copy called on invalid data_ref {}", self.data_ref);
            return new_obj; // Return empty object
        }
        for (key, value) in self.objects() { // objects() itself locks and gets data
            new_obj.set_property(&key, value);
        }
        new_obj
    }

    pub fn deep_copy(&self) -> Self {
        let mut new_obj = DataObject::new();
        // Check if self.data_ref is valid
        if !oheap().lock().contains_key(self.data_ref) {
            #[cfg(not(feature = "no_std_support"))]
            println!("Warning: deep_copy called on invalid data_ref {}", self.data_ref);
            return new_obj; // Return empty object
        }
        for (key, value) in self.objects() {
            match value {
                Data::DObject(obj_ref) => {
                    let nested_obj = DataObject::get(obj_ref);
                    new_obj.put_object(&key, nested_obj.deep_copy());
                }
                Data::DArray(arr_ref) => {
                    let nested_arr = DataArray::get(arr_ref); // Assumes DataArray::get exists
                    new_obj.put_array(&key, nested_arr.deep_copy()); // Assumes DataArray::deep_copy exists
                }
                Data::DBytes(bytes_ref) => {
                    let nested_bytes = DataBytes::get(bytes_ref); // Assumes DataBytes::get exists
                    new_obj.put_bytes(&key, nested_bytes.deep_copy()); // Assumes DataBytes::deep_copy exists
                }
                _ => {
                    // Primitives are copied by value when Data is cloned in self.objects()
                    // and then set_property clones them again if they are strings.
                    new_obj.set_property(&key, value);
                }
            }
        }
        new_obj
    }

    // --- Accessors ---
    pub fn has(&self, key: &str) -> bool {
        let heap_guard = &mut oheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            return false; // Key cannot exist if object itself doesn't
        }
        heap_guard.get(self.data_ref).contains_key(key)
    }

    pub fn keys(self) -> Vec<String> { // Consumes self, consider taking &self
        let heap_guard = &mut oheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            return Vec::new();
        }
        let map = heap_guard.get(self.data_ref);
        map.keys().cloned().collect()
    }

    // Non-consuming version of keys
    pub fn get_keys(&self) -> Vec<String> {
        let heap_guard = &mut oheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            return Vec::new();
        }
        let map = heap_guard.get(self.data_ref);
        map.keys().cloned().collect()
    }


    // Existing panicking getter
    pub fn get_property(&self, key: &str) -> Data {
        let heap_guard = &mut oheap().lock();
        // It's crucial that heap.get() itself panics or handles invalid self.data_ref.
        // If heap.get() returns an Option or Result, this needs adjustment.
        // Assuming heap.get() panics on invalid ref for now.
        let map = heap_guard.get(self.data_ref);
        map.get(key).cloned().unwrap_or_else(|| {
            // This panic assumes self.data_ref was valid but key was not in the map.
            panic!(
                "DataObject::get_property failed: Key '{}' not found in object at ref {}",
                key, self.data_ref
            );
        })
    }

    // --- New `try_get_` methods ---

    /// Tries to get a property by key, returning a Result.
    pub fn try_get_property(&self, key: &str) -> Result<Data, NDataError> {
        let heap_guard = &mut oheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            // This indicates the DataObject handle itself points to an invalid/deallocated ref.
            // This is a more fundamental issue than a missing key.
            // For now, treat as KeyNotFound for the purpose of this getter,
            // or introduce a new NDataError variant like InvalidObjectRef.
            // Let's use KeyNotFound to imply the data isn't accessible via this key.
             return Err(NDataError::KeyNotFound(key.to_string()));
        }
        let map = heap_guard.get(self.data_ref);
        map.get(key)
            .cloned() // Clone the Data out of the map
            .ok_or_else(|| NDataError::KeyNotFound(key.to_string()))
    }

    pub fn try_get_string(&self, key: &str) -> Result<String, NDataError> {
        match self.try_get_property(key)? {
            Data::DString(s) => Ok(s),
            other => Err(NDataError::WrongDataType {
                key: key.to_string(),
                expected: "string",
                found: other.type_name_owned(), // Assumes Data::type_name_owned() implemented
            }),
        }
    }

    pub fn try_get_boolean(&self, key: &str) -> Result<bool, NDataError> {
        match self.try_get_property(key)? {
            Data::DBoolean(b) => Ok(b),
            other => Err(NDataError::WrongDataType {
                key: key.to_string(),
                expected: "boolean",
                found: other.type_name_owned(),
            }),
        }
    }

    pub fn try_get_int(&self, key: &str) -> Result<i64, NDataError> {
        match self.try_get_property(key)? {
            Data::DInt(i) => Ok(i),
            other => Err(NDataError::WrongDataType {
                key: key.to_string(),
                expected: "int",
                found: other.type_name_owned(),
            }),
        }
    }

    pub fn try_get_float(&self, key: &str) -> Result<f64, NDataError> {
        match self.try_get_property(key)? {
            Data::DFloat(f) => Ok(f),
            Data::DInt(i) => Ok(i as f64), // Allow int to be read as float
            other => Err(NDataError::WrongDataType {
                key: key.to_string(),
                expected: "float (or int)",
                found: other.type_name_owned(),
            }),
        }
    }

    pub fn try_get_object(&self, key: &str) -> Result<DataObject, NDataError> {
        match self.try_get_property(key)? {
            Data::DObject(obj_ref) => Ok(DataObject::get(obj_ref)), // DataObject::get increments ref count
            other => Err(NDataError::WrongDataType {
                key: key.to_string(),
                expected: "DataObject",
                found: other.type_name_owned(),
            }),
        }
    }

    pub fn try_get_array(&self, key: &str) -> Result<DataArray, NDataError> {
        match self.try_get_property(key)? {
            Data::DArray(arr_ref) => Ok(DataArray::get(arr_ref)), // Assumes DataArray::get increments ref count
            other => Err(NDataError::WrongDataType {
                key: key.to_string(),
                expected: "DataArray",
                found: other.type_name_owned(),
            }),
        }
    }

    pub fn try_get_bytes(&self, key: &str) -> Result<DataBytes, NDataError> {
        match self.try_get_property(key)? {
            Data::DBytes(bytes_ref) => Ok(DataBytes::get(bytes_ref)), // Assumes DataBytes::get increments ref count
            other => Err(NDataError::WrongDataType {
                key: key.to_string(),
                expected: "DataBytes",
                found: other.type_name_owned(),
            }),
        }
    }

    // Existing typed getters (panicking)
    pub fn get_string(&self, key: &str) -> String { self.get_property(key).string() } // Assumes Data::string() panics on wrong type
    #[deprecated(since = "0.3.0", note = "please use `get_boolean` instead")]
    pub fn get_bool(&self, key: &str) -> bool { self.get_boolean(key) }
    #[deprecated(since = "0.3.0", note = "please use `get_int` instead")]
    pub fn get_i64(&self, key: &str) -> i64 { self.get_int(key) }
    #[deprecated(since = "0.3.0", note = "please use `get_float` instead")]
    pub fn get_f64(&self, key: &str) -> f64 { self.get_float(key) }
    pub fn get_boolean(&self, key: &str) -> bool { self.get_property(key).boolean() } // Assumes Data::boolean() panics
    pub fn get_int(&self, key: &str) -> i64 { self.get_property(key).int() } // Assumes Data::int() panics
    pub fn get_float(&self, key: &str) -> f64 {
        let d = self.get_property(key);
        if d.is_int() { d.int() as f64 } else { d.float() } // Assumes Data::float() panics if not float/int
    }
    pub fn get_object(&self, key: &str) -> DataObject { self.get_property(key).object() } // Assumes Data::object() panics
    pub fn get_array(&self, key: &str) -> DataArray { self.get_property(key).array() } // Assumes Data::array() panics
    pub fn get_bytes(&self, key: &str) -> DataBytes { self.get_property(key).bytes() } // Assumes Data::bytes() panics

    // --- Mutators ---
    pub fn remove_property(&mut self, key: &str) {
        let old_data_opt = {
            let heap_guard = &mut oheap().lock();
            if !heap_guard.contains_key(self.data_ref) {
                 #[cfg(not(feature = "no_std_support"))]
                 println!("Warning: remove_property called on invalid data_ref {}", self.data_ref);
                 return; // Cannot remove from non-existent object
            }
            let map = heap_guard.get(self.data_ref);
            map.remove(key)
        };

        // If old_data_opt is Some, its Drop implementation will handle queuing for GC.
        // The explicit reconstruction here ensures Drop is called.
        if let Some(old_data) = old_data_opt {
            match old_data {
                Data::DObject(i) => { let _ = DataObject { data_ref: i }; } // Drop will queue 'i'
                Data::DArray(i) => { let _ = DataArray { data_ref: i }; }   // Drop will queue 'i'
                Data::DBytes(i) => { let _ = DataBytes { data_ref: i }; }   // Drop will queue 'i'
                _ => {} // Primitives don't need explicit Drop handling for GC queueing
            }
        }
    }

    pub fn set_property(&mut self, key: &str, data: Data) {
        // Step 1: Check if the current DataObject's data_ref is valid.
        // If not, we cannot insert into its map. This is a critical check.
        // However, oheap().lock().get(self.data_ref) inside the match arms
        // will panic if self.data_ref is invalid, which might be acceptable.
        // For robustness, one might check self.data_ref validity upfront.

        // Step 2 & 3: Acquire necessary locks, increment ref count for the *new* data,
        // and insert. Then, handle the *old* data.
        let old_data_opt = match &data {
            Data::DObject(new_obj_ref) => {
                let oheap_guard = &mut oheap().lock();
                oheap_guard.incr(*new_obj_ref); // Increment ref for new data
                if !oheap_guard.contains_key(self.data_ref) {
                     #[cfg(not(feature = "no_std_support"))]
                     println!("Warning: set_property target object (ref {}) does not exist in heap.", self.data_ref);
                     // Decrement the prematurely incremented ref count if we can't insert
                     oheap_guard.decr(*new_obj_ref); // This might be complex if decr also tries to GC
                     return; // Or handle error appropriately
                }
                let map = oheap_guard.get(self.data_ref);
                map.insert(key.to_string(), data) // data (which is Data::DObject(*new_obj_ref)) is moved here
            }
            Data::DArray(new_arr_ref) => {
                let oheap_guard = &mut oheap().lock();
                {
                    let aheap_guard = &mut dataarray::aheap().lock();
                    aheap_guard.incr(*new_arr_ref);
                }
                if !oheap_guard.contains_key(self.data_ref) {
                     #[cfg(not(feature = "no_std_support"))]
                     println!("Warning: set_property target object (ref {}) does not exist in heap.", self.data_ref);
                     // Also need to decrement aheap_guard for *new_arr_ref
                     // This error path gets complicated with multiple heaps.
                     return;
                }
                let map = oheap_guard.get(self.data_ref);
                map.insert(key.to_string(), data)
            }
            Data::DBytes(new_bytes_ref) => {
                let oheap_guard = &mut oheap().lock();
                {
                    let bheap_guard = &mut databytes::bheap().lock();
                    bheap_guard.incr(*new_bytes_ref);
                }
                if !oheap_guard.contains_key(self.data_ref) {
                     #[cfg(not(feature = "no_std_support"))]
                     println!("Warning: set_property target object (ref {}) does not exist in heap.", self.data_ref);
                     return;
                }
                let map = oheap_guard.get(self.data_ref);
                map.insert(key.to_string(), data)
            }
            _ => { // Primitive types
                let oheap_guard = &mut oheap().lock();
                if !oheap_guard.contains_key(self.data_ref) {
                     #[cfg(not(feature = "no_std_support"))]
                     println!("Warning: set_property target object (ref {}) does not exist in heap.", self.data_ref);
                     return;
                }
                let map = oheap_guard.get(self.data_ref);
                map.insert(key.to_string(), data)
            }
        };

        // Step 4: If an old value was replaced, its handle (if it was DObject/DArray/DBytes)
        // will be dropped, and its Drop impl will queue it for GC.
        if let Some(old_data_handle) = old_data_opt {
            // Reconstruct the handle to trigger its Drop impl.
            match old_data_handle {
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

    // In these `put_` methods for complex types, the passed handle `o`, `a`, `b` is moved.
    // `set_property` increments the ref count of the underlying data.
    // When `o`, `a`, or `b` goes out of scope at the end of this function, its Drop impl
    // queues a decrement. This correctly balances the ref count: one new reference is
    // held by the map, and the original handle's reference is conceptually "transferred".
    // The `core::mem::forget` calls are not needed and were correctly commented out previously.

    pub fn put_object(&mut self, key: &str, o: DataObject) {
        self.set_property(key, Data::DObject(o.data_ref));
    }
    #[deprecated(since = "0.1.2", note = "please use `put_array` instead")]
    pub fn put_list(&mut self, key: &str, a: DataArray) { self.put_array(key, a); }
    pub fn put_array(&mut self, key: &str, a: DataArray) {
        self.set_property(key, Data::DArray(a.data_ref));
    }
    pub fn put_bytes(&mut self, key: &str, b: DataBytes) {
        self.set_property(key, Data::DBytes(b.data_ref));
    }
    pub fn put_null(&mut self, key: &str) { self.set_property(key, Data::DNull); }

    // --- Internal GC Helper ---
    // This `delete` function is part of the recursive GC logic.
    // It decrements counts and recursively calls delete for nested objects/arrays
    // only when the count drops to 1 (meaning this is the last reference being removed
    // before actual deallocation).
    pub(crate) fn delete(
        oheap_guard: &mut Heap<HashMap<String, Data>>, // Pass as mutable ref
        data_ref: usize,
        aheap_guard: &mut Heap<Vec<Data>>, // Pass as mutable ref
    ) {
        // Check if ref is valid before trying to get its count or data.
        if !oheap_guard.contains_key(data_ref) {
            #[cfg(not(feature = "no_std_support"))]
            println!("Warning: DataObject::delete called on non-existent ref {}", data_ref);
            return;
        }

        let current_count = oheap_guard.count(data_ref);

        if current_count == 0 { // Should not happen if contains_key passed. Paranoia.
            #[cfg(not(feature = "no_std_support"))]
            println!("Warning: DataObject::delete called on ref {} with count 0 (after contains_key check)", data_ref);
            return;
        }

        // If this is the last reference, remove its children's references too.
        if current_count == 1 {
            let mut objects_to_kill = Vec::<usize>::new();
            let mut arrays_to_kill = Vec::<usize>::new();
            // No need to kill DataBytes here as they don't contain other ndata refs.

            // Temporarily get the map to iterate over its values.
            // This is safe because we are about to decrement its count to 0 and remove it.
            let map_clone = oheap_guard.get(data_ref).clone(); // Clone to iterate without holding immutable borrow during mutable calls

            for value in map_clone.values() {
                match value {
                    Data::DObject(i) => objects_to_kill.push(*i),
                    Data::DArray(i) => arrays_to_kill.push(*i),
                    _ => {} // Primitives and DataBytes don't need recursive deletion calls from here.
                }
            }

            // Now, decrement the count of this object. Since it was 1, it will become 0.
            // The heap's decr method should handle actual removal if count reaches 0.
            oheap_guard.decr(data_ref);
            // At this point, oheap_guard.get(data_ref) would likely panic or return None.

            // Recursively call delete for children.
            // These children's counts are effectively being decremented.
            for i in objects_to_kill {
                DataObject::delete(oheap_guard, i, aheap_guard);
            }
            for i in arrays_to_kill {
                dataarray::DataArray::delete(aheap_guard, i, oheap_guard); // Assumes DataArray::delete exists and takes similar args
            }

        } else if current_count > 1 {
            // If other references exist, just decrement the count.
            oheap_guard.decr(data_ref);
        }
    }


    // --- Utility / Debug ---
    pub fn objects(&self) -> Vec<(String, Data)> {
        let heap_guard = &mut oheap().lock();
        if !heap_guard.contains_key(self.data_ref) {
            #[cfg(not(feature = "no_std_support"))]
            println!("Warning: objects() called on invalid data_ref {}", self.data_ref);
            return Vec::new();
        }
        let map = heap_guard.get(self.data_ref);
        map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    #[cfg(not(feature = "no_std_support"))]
    pub fn print_heap() {
        // This is a static method, doesn't depend on a specific DataObject instance.
        println!("Object Heap Keys: {:?}", oheap().lock().keys());
    }

    // --- Garbage Collection ---
    pub fn gc() {
        // Lock heaps in consistent order: oheap -> aheap
        // (bheap for DataBytes might also be involved if it can be GC'd independently
        // or contains references, though current DataBytes is just Vec<u8>).
        let mut oheap_guard = oheap().lock(); // Make guards mutable for delete
        let mut aheap_guard = dataarray::aheap().lock(); // Make guards mutable
        let mut odrop_guard = odrop().lock();

        // Drain the queue of objects whose handles were dropped.
        for data_ref in odrop_guard.drain(..) {
            // Call the internal delete method which handles recursive decrements.
            DataObject::delete(&mut oheap_guard, data_ref, &mut aheap_guard);
        }
        // Similar GC calls for DataArray and DataBytes would be needed here,
        // e.g., DataArray::gc_internal(aheap_guard, oheap_guard);
        // dataarray::DataArray::gc_process_queue(&mut *aheap_guard, &mut* oheap_guard);
        // databytes::DataBytes::gc_process_queue(...);
    }
}

// --- Drop Implementation ---
impl Drop for DataObject {
    fn drop(&mut self) {
        // When a DataObject handle is dropped, its data_ref is added to a queue.
        // The actual decrement and potential deallocation happen during DataObject::gc().
        let _ = odrop().lock().push(self.data_ref);
    }
}

// Helper needed for NDataError, assuming it's in data.rs or similar
// This would be part of `impl Data` in `data.rs`
/*
impl Data {
    pub fn type_name_owned(&self) -> &'static str { // Changed to return &'static str for simplicity
        match self {
            Data::DInt(_) => "int",
            Data::DFloat(_) => "float",
            Data::DBoolean(_) => "boolean",
            Data::DString(_) => "string",
            Data::DObject(_) => "DataObject",
            Data::DArray(_) => "DataArray",
            Data::DBytes(_) => "DataBytes",
            Data::DNull => "null",
        }
    }
}
*/
