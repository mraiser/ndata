extern crate alloc;
use std::collections::HashMap;
use crate::heap::*;
use crate::data::*;
use crate::dataarray::*;
use crate::databytes::*;
use crate::sharedmutex::*;

#[cfg(feature="serde_support")]
use serde_json::Value;
#[cfg(feature="serde_support")]
use serde_json::json;
#[cfg(not(feature="serde_support"))]
use crate::json_util::*;

/// Storage for runtime object values
static mut OH:SharedMutex<Heap<HashMap<String,Data>>> = SharedMutex::new();

/// Storage for runtime reference count reductions
static mut OD:SharedMutex<Vec<usize>> = SharedMutex::new();

/// **DO NOT USE**
///
/// This function should only be used externally by DataArray
pub fn oheap() -> &'static mut SharedMutex<Heap<HashMap<String,Data>>> {
  unsafe { &mut OH }
}

fn odrop() -> &'static mut SharedMutex<Vec<usize>> {
  unsafe { &mut OD }
}

/// Represents a map of type ```<String, ndata.Data>```. 
#[derive(Debug, Default)]
pub struct DataObject {
  /// The pointer to the object in the object heap.
  pub data_ref: usize,
}

impl Clone for DataObject{
  /// Returns another DataObject pointing to the same value.
  fn clone(&self) -> Self {
    let o = DataObject{
      data_ref: self.data_ref,
    };
    let _x = &mut oheap().lock().incr(self.data_ref);
    o
  }
}

impl DataObject {
  /// Initialize global storage of objects. Call only once at startup.
  pub fn init() -> ((u64, u64),(u64, u64)){
    unsafe {
      OH.set(Heap::new());
      OD.set(Vec::new());
    }
    DataObject::share()
  }
  
  pub fn share() -> ((u64, u64),(u64, u64)){
    unsafe {
      let q = OH.share();
      let r = OD.share();
      (q, r)
    }
  }
  
  /// Mirror global storage of objects from another process. Call only once at startup.
  pub fn mirror(q:(u64, u64), r:(u64, u64)){
    unsafe {
      OH.mirror(q.0, q.1);
      OD.mirror(r.0, r.1);
    }
  }
  
  /// Create a new (empty) object.
  pub fn new() -> DataObject {
    let data_ref = &mut oheap().lock().push(HashMap::<String,Data>::new());
    return DataObject {
      data_ref: *data_ref,
    };
  }
  
  /// Get a reference to the object from the heap
  pub fn get(data_ref: usize) -> DataObject {
    let o = DataObject{
      data_ref: data_ref,
    };
    let _x = &mut oheap().lock().incr(data_ref);
    o
  }
  
  /// Increase the reference count for this DataObject.
  pub fn incr(&self) {
    let oheap = &mut oheap().lock();
    oheap.incr(self.data_ref); 
  }

  /// Decrease the reference count for this DataObject.
  pub fn decr(&self) {
    let oheap = &mut oheap().lock();
    oheap.decr(self.data_ref); 
  }

  /// Create a new DataObject from a JSON string.
  pub fn from_string(s:&str) -> DataObject {
    #[cfg(feature="serde_support")]
    return DataObject::from_json(serde_json::from_str(s).unwrap());
    #[cfg(not(feature="serde_support"))]
    return object_from_string(s);
 }  
  
  /// Create a JSON string from a DataObject.
  pub fn to_string(&self) -> String {
    #[cfg(feature="serde_support")]
    return self.to_json().to_string();
    #[cfg(not(feature="serde_support"))]
    return object_to_string(self.clone());
  }  
  
  /// Create a new object from the ```serde_json::Value```.
  #[cfg(feature="serde_support")]
  pub fn from_json(value:Value) -> DataObject {
    let mut o = DataObject::new();
    for (key, val) in value.as_object().unwrap().iter() {
      if val.is_string(){ o.put_string(key, val.as_str().unwrap()); }
      else if val.is_boolean() { o.put_boolean(key, val.as_bool().unwrap()); }
      else if val.is_i64() { o.put_int(key, val.as_i64().unwrap()); }
      else if val.is_f64() { o.put_float(key, val.as_f64().unwrap()); }
      else if val.is_object() { o.put_object(key, DataObject::from_json(val.to_owned())); }
      else if val.is_array() { o.put_array(key, DataArray::from_json(val.to_owned())); }      
      else if val.is_null() { o.put_null(key); }
      else { println!("Unknown type {}", val) };
    }
    o
  }
  
  /// Return the object as a ```serde_json::Value```.
  #[cfg(feature="serde_support")]
  pub fn to_json(&self) -> Value {
    let mut val = json!({});
    for (keystr,old) in self.objects() {
      if old.is_int() { val[keystr] = json!(self.get_int(&keystr)); }
      else if old.is_float() { val[keystr] = json!(self.get_float(&keystr)); }
      else if old.is_boolean() { val[keystr] = json!(self.get_boolean(&keystr)); }
      else if old.is_string() { val[keystr] = json!(self.get_string(&keystr)); }
      else if old.is_object() { val[keystr] = self.get_object(&keystr).to_json(); }
      else if old.is_array() { val[keystr] = self.get_array(&keystr).to_json(); }
      else if old.is_bytes() { val[keystr] = json!(self.get_bytes(&keystr).to_hex_string()); }
      else { val[keystr] = json!(null); }
    }
    val
  }
  
  /// Returns a new ```DataObject``` that points to the same underlying object instance.
  #[deprecated(since="0.3.0", note="please use `clone` instead")]
  pub fn duplicate(&self) -> DataObject {
    self.clone()
  }
  
  /// Returns a new ```DataObject``` that points to a new object instance, which contains the 
  /// same underlying data as the original. 
  pub fn shallow_copy(&self) -> DataObject {
    let mut o = DataObject::new();
    for (k,v) in self.objects() {
      o.set_property(&k, v.clone());
    }
    o
  }

  /// Returns a new ```DataObject``` that points to a new object instance, which contains a 
  /// recursively deep copy of the original underlying data.
  pub fn deep_copy(&self) -> DataObject {
    let mut o = DataObject::new();
    for (key,v) in self.objects() {
      if v.is_object() {
        o.put_object(&key, v.object().deep_copy());
      }
      else if v.is_array() {
        o.put_array(&key, v.array().deep_copy());
      }
      else if v.is_bytes() {
        o.put_bytes(&key, v.bytes().deep_copy());
      }
      else {
        o.set_property(&key, v.clone());
      }
    }
    o
  }
  
  /// Returns ```true``` if this object contains the given key.
  pub fn has(&self, key:&str) -> bool {
    let heap = &mut oheap().lock();
    let map = heap.get(self.data_ref);
    map.contains_key(key)
  }
  
  /// Returns a list (```Vec<String>```) of the keys in this object.
  pub fn keys(self) -> Vec<String> {
    let mut vec = Vec::<String>::new();
    for (key, _val) in self.objects() {
      vec.push(key)
    }
    vec
  }
  
  /// Returns the stored value for the given key.
  pub fn get_property(&self, key:&str) -> Data {
    let heap = &mut oheap().lock();
    let map = heap.get(self.data_ref);
    let data = map.get(key);
    if data.is_none() { panic!("Object {:?} does not have key {}", map, key); }
    data.unwrap().clone()
  }
  
  /// Returns the stored value for the given key as a ```String```.
  pub fn get_string(&self, key:&str) -> String {
    self.get_property(key).string()
  }
  
  /// Returns the stored value for the given key as a ```bool```.
  #[deprecated(since="0.3.0", note="please use `get_boolean` instead")]
  pub fn get_bool(&self, key:&str) -> bool {
    self.get_boolean(key)
  }
  
  /// Returns the stored value for the given key as an ```i64```.
  #[deprecated(since="0.3.0", note="please use `get_int` instead")]
  pub fn get_i64(&self, key:&str) -> i64 {
    self.get_int(key)
  }
  
  /// Returns the stored value for the given key as an ```f64```.
  #[deprecated(since="0.3.0", note="please use `get_float` instead")]
  pub fn get_f64(&self, key:&str) -> f64 {
    self.get_float(key)
  }
  
  /// Returns the stored value for the given key as a ```bool```.
  pub fn get_boolean(&self, key:&str) -> bool {
    self.get_property(key).boolean()
  }

  /// Returns the stored value for the given key as an ```i64```.
  pub fn get_int(&self, key:&str) -> i64 {
    self.get_property(key).int()
  }

  /// Returns the stored value for the given key as an ```f64```.
  pub fn get_float(&self, key:&str) -> f64 {
    let d = self.get_property(key);
    if d.is_int() { return d.int() as f64; }
    d.float()
  }

  /// Returns the stored value for the given key as a ```DataObject```.
  pub fn get_object(&self, key:&str) -> DataObject {
    self.get_property(key).object()
  }
  
  /// Returns the stored value for the given key as a ```DataArray```.
  pub fn get_array(&self, key:&str) -> DataArray {
    self.get_property(key).array()
  }
  
  /// Returns the stored value for the given key as a ```DataBytes```.
  pub fn get_bytes(&self, key:&str) -> DataBytes {
    self.get_property(key).bytes()
  }
  
  /// Remove the value from the object for the given key.
  pub fn remove_property(&mut self, key:&str) {
    let oheap = &mut oheap().lock();
    let map = oheap.get(self.data_ref);
    if let Some(old) = map.remove(key){
      if let Data::DObject(i) = &old {
        let _x = DataObject {
          data_ref: *i,
        };
      }
      else if let Data::DArray(i) = &old {
        let _x = DataArray {
          data_ref: *i,
        };
      }
      else if let Data::DBytes(i) = &old {
        let _x = DataBytes {
          data_ref: *i,
        };
      }
    }
  }
  
  /// Set the given value for the given key.
  pub fn set_property(&mut self, key:&str, data:Data) {
    if let Data::DObject(i) = &data {
      let oheap = &mut oheap().lock();
      oheap.incr(*i); 
    }
    else if let Data::DArray(i) = &data {
      let aheap = &mut aheap().lock();
      aheap.incr(*i);
    }
    else if let Data::DBytes(i) = &data {
      let bheap = &mut bheap().lock();
      bheap.incr(*i);
    }
    
    let oheap = &mut oheap().lock();
    let map = oheap.get(self.data_ref);
    if let Some(old) = map.insert(key.to_string(),data){
      if let Data::DObject(i) = &old {
        let _x = DataObject {
          data_ref: *i,
        };
      }
      else if let Data::DArray(i) = &old {
        let _x = DataArray {
          data_ref: *i,
        };
      }
      else if let Data::DBytes(i) = &old {
        let _x = DataBytes {
          data_ref: *i,
        };
      }
    }
  }
  
  /// Set the given ```String``` value for the given key.
  #[deprecated(since="0.3.0", note="please use `put_string` instead")]
  pub fn put_str(&mut self, key:&str, val:&str) {
    self.put_string(key,val);
  }
  
  /// Set the given ```bool``` value for the given key.
  #[deprecated(since="0.3.0", note="please use `put_booolean` instead")]
  pub fn put_bool(&mut self, key:&str, val:bool) {
    self.put_boolean(key, val);
  }
  
  /// Set the given ```i64``` value for the given key.
  #[deprecated(since="0.3.0", note="please use `put_int` instead")]
  pub fn put_i64(&mut self, key:&str, val:i64) {
    self.put_int(key, val);
  }
  
  /// Set the given ```String``` value for the given key.
  pub fn put_string(&mut self, key:&str, val:&str) {
    self.set_property(key,Data::DString(val.to_string()));
  }

  /// Set the given ```bool``` value for the given key.
  pub fn put_boolean(&mut self, key:&str, val:bool) {
    self.set_property(key,Data::DBoolean(val));
  }

  /// Set the given ```i64``` value for the given key.
  pub fn put_int(&mut self, key:&str, val:i64) {
    self.set_property(key,Data::DInt(val));
  }

  /// Set the given ```f64``` value for the given key.
  pub fn put_float(&mut self, key:&str, val:f64) {
    self.set_property(key,Data::DFloat(val));
  }

  /// Set the given ```DataObject``` value for the given key.
  pub fn put_object(&mut self, key:&str, o:DataObject) {
    self.set_property(key, Data::DObject(o.data_ref));
  }
  
  #[deprecated(since="0.1.2", note="please use `put_array` instead")]  
  pub fn put_list(&mut self, key:&str, a:DataArray) {
    self.set_property(key, Data::DArray(a.data_ref));
  }
  
  /// Set the given ```DataArray``` value for the given key.
  pub fn put_array(&mut self, key:&str, a:DataArray) {
    self.set_property(key, Data::DArray(a.data_ref));
  }
  
  /// Set the given ```DataBytes``` value for the given key.
  pub fn put_bytes(&mut self, key:&str, a:DataBytes) {
    self.set_property(key, Data::DBytes(a.data_ref));
  }
  
  /// Set the for the given key to ```DNull```.
  pub fn put_null(&mut self, key:&str) {
    self.set_property(key, Data::DNull);
  }
  
  /// **DO NOT USE**
  ///
  /// Reduces the reference count for this object by one, as well as the reference counts of any
  /// objects, arrays, or byte buffers contained in this object. This function should only be used
  /// externally by ```DataArray::gc()```.
  pub fn delete(oheap:&mut Heap<HashMap<String,Data>>, data_ref:usize, aheap:&mut Heap<Vec<Data>>) {
    let mut objects_to_kill = Vec::<usize>::new();
    let mut arrays_to_kill = Vec::<usize>::new();
    
    let n = oheap.count(data_ref);
    if n == 1 {
      let map = oheap.get(data_ref);
      for (_k,v) in map {
        if let Data::DObject(i) = v {
          objects_to_kill.push(*i);
        }
        else if let Data::DArray(i) = v {
          arrays_to_kill.push(*i);
        }
        else if let Data::DBytes(i) = v {
          let _x = DataBytes {
            data_ref: *i,
          };
        }
      }
    }
    oheap.decr(data_ref);
    
    for i in objects_to_kill {
      DataObject::delete(oheap, i, aheap);
    }
    for i in arrays_to_kill {
      DataArray::delete(aheap, i, oheap);
    }
  }
  
  /// Returns the key value pairs in this object as a ```Vec<String, Data>```. 
  pub fn objects(&self) -> Vec<(String, Data)> {
    let heap = &mut oheap().lock();
    let map = heap.get(self.data_ref);
    let mut vec = Vec::<(String, Data)>::new();
    for (k,v) in map {
      vec.push((k.to_string(),v.clone()));
    }
    vec
  }
  
  /// Prints the objects currently stored in the heap
  #[cfg(not(feature="no_std_support"))]
  pub fn print_heap() {
    println!("object {:?}", &mut oheap().lock().keys());
  }
  
  /// Perform garbage collection. Objects will not be removed from the heap until
  /// ```DataObject::gc()``` is called.
  pub fn gc() {
    let oheap = &mut oheap().lock();
    let aheap = &mut aheap().lock();
    let odrop = &mut odrop().lock();
    let mut i = odrop.len();
    while i>0 {
      i = i - 1;
      let x = odrop.remove(0);
      DataObject::delete(oheap, x, aheap);
    }
  }
}

/// Adds this ```DataObject```'s data_ref to ADROP. Reference counts are adjusted when
/// ```DataObject::gc()``` is called.
impl Drop for DataObject {
  fn drop(&mut self) {
    let _x = &mut odrop().lock().push(self.data_ref);
  }
}

