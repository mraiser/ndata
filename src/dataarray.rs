#[cfg(feature="no_std_support")]
use hashbrown::hash_map::HashMap;
#[cfg(not(feature="no_std_support"))]
use std::collections::HashMap;
use crate::heap::*;
use crate::data::*;
use crate::dataobject::*;
use crate::databytes::*;
use crate::sharedmutex::*;

#[cfg(feature="serde_support")]
use serde_json::Value;
#[cfg(feature="serde_support")]
use serde_json::json;
#[cfg(not(feature="serde_support"))]
use crate::json_util::*;

/// Storage for runtime array values
static mut AH:SharedMutex<Heap<Vec<Data>>> = SharedMutex::new();

/// Storage for runtime reference count reductions
static mut AD:SharedMutex<Vec<usize>> = SharedMutex::new();

/// **DO NOT USE**
///
/// This function should only be used externally by DataObject
pub fn aheap() -> &'static mut SharedMutex<Heap<Vec<Data>>> {
  unsafe { &mut AH }
}

fn adrop() -> &'static mut SharedMutex<Vec<usize>> {
  unsafe { &mut AD }
}

/// Represents an array of type ```ndata.Data```. 
#[derive(Debug, Default)]
pub struct DataArray {
  /// The pointer to the array in the array heap.
  pub data_ref: usize,
}

impl DataArray {
  /// Initialize global storage of arrays. Call only once at startup.
  #[cfg(not(feature="mirror"))]
  pub fn init(){
    unsafe {
      AH.set(Heap::new());
      AD.set(Vec::new());
    }
  }
  #[cfg(feature="mirror")]
  pub fn init() -> ((u64, u64), (u64, u64)){
    unsafe{
      AH.init();
      AD.init();
      let q = AH.share();
      let r = AD.share();
      (q, r)
    }
  }
  
  /// Mirror global storage of arrays from another process. Call only once at startup.
  #[cfg(feature="mirror")]
  pub fn mirror(q:(u64, u64), r:(u64, u64)){
    unsafe { 
      AH = SharedMutex::mirror(q.0, q.1);
      AD = SharedMutex::mirror(r.0, r.1);
    }
  }
  
  /// Create a new (empty) array.
  pub fn new() -> DataArray {
    let data_ref = &mut aheap().lock().push(Vec::<Data>::new());
    return DataArray {
      data_ref: *data_ref,
    };
  }
  
  /// Get a reference to the array from the heap
  pub fn get(data_ref: usize) -> DataArray {
    let o = DataArray{
      data_ref: data_ref,
    };
    let _x = &mut aheap().lock().incr(data_ref);
    o
  }
  
  /// Increase the reference count for this DataArray.
  pub fn incr(&self) {
    let aheap = &mut aheap().lock();
    aheap.incr(self.data_ref); 
  }

  /// Decrease the reference count for this DataArray.
  pub fn decr(&self) {
    let aheap = &mut aheap().lock();
    aheap.decr(self.data_ref); 
  }

  /// Create a new DataArray from a JSON string.
  pub fn from_string(s:&str) -> DataArray {
    #[cfg(not(feature="serde_support"))]
    return array_from_string(s);
    #[cfg(feature="serde_support")]
    DataArray::from_json(serde_json::from_str(s).unwrap())
  }  
  
  /// Create a JSON string from a DataArray.
  pub fn to_string(&self) -> String {
    #[cfg(not(feature="serde_support"))]
    return array_to_string(self.duplicate());
    #[cfg(feature="serde_support")]
    self.to_json().to_string()
  }  
  
  /// Create a new array from the ```serde_json::Value```.
  #[cfg(feature="serde_support")]
  pub fn from_json(value:Value) -> DataArray {
    let mut o = DataArray::new();
    
    for val in value.as_array().unwrap().iter() {
      if val.is_string(){ o.push_str(val.as_str().unwrap()); }
      else if val.is_boolean() { o.push_bool(val.as_bool().unwrap()); }
      else if val.is_i64() { o.push_i64(val.as_i64().unwrap()); }
      else if val.is_f64() { o.push_float(val.as_f64().unwrap()); }
      else if val.is_object() { o.push_object(DataObject::from_json(val.to_owned())); }
      else if val.is_array() { o.push_array(DataArray::from_json(val.to_owned())); }      
      else { println!("Unknown type {}", val) };
    }
      
    o
  }
  
  /// Return the array as a ```serde_json::Value```.
  #[cfg(feature="serde_support")]
  pub fn to_json(&self) -> Value {
    let mut val = Vec::<Value>::new();
    let mut id = 0;
    for old in self.objects() {
      if old.is_int() { val.push(json!(self.get_i64(id))); }
      else if old.is_float() { val.push(json!(self.get_f64(id))); }
      else if old.is_boolean() { val.push(json!(self.get_bool(id))); }
      else if old.is_string() { val.push(json!(self.get_string(id))); }
      else if old.is_object() { val.push(self.get_object(id).to_json()); }
      else if old.is_array() { val.push(self.get_array(id).to_json()); }
      else if old.is_bytes() { val.push(json!(self.get_bytes(id).to_hex_string())); }
      else { val.push(json!(null)); }
      id = id + 1;
    }
    json!(val)
  }
  
  /// Returns a new ```DataArray``` that points to the same underlying array instance.
  pub fn duplicate(&self) -> DataArray {
    let o = DataArray{
      data_ref: self.data_ref,
    };
    let _x = &mut aheap().lock().incr(self.data_ref);
    o
  }
  
  /// Returns a new ```DataArray``` that points to a new array instance, which contains the 
  /// same underlying data as the original. 
  pub fn shallow_copy(self) -> DataArray {
    let mut o = DataArray::new();
    for v in self.objects() {
      o.push_property(v.clone());
    }
    o
  }

  /// Returns a new ```DataArray``` that points to a new array instance, which contains a 
  /// recursively deep copy of the original underlying data.
  pub fn deep_copy(&self) -> DataArray {
    let mut o = DataArray::new();
    let mut id = 0;
    for v in self.objects() {
      if v.is_object() {
        o.push_object(self.get_object(id).deep_copy());
      }
      else if v.is_array() {
        o.push_array(self.get_array(id).deep_copy());
      }
      else if v.is_bytes() {
        o.push_bytes(self.get_bytes(id).deep_copy());
      }
      else {
        o.push_property(v.clone());
      }
      id = id + 1;
    }
    o
  }

  /// Returns the length of the array.
  pub fn len(&self) -> usize {
    let heap = &mut aheap().lock();
    let vec = heap.get(self.data_ref);
    vec.len()
  }
  
  /// Returns the index of a Data in the array
  pub fn index_of(&self, b: Data) -> i64 {
    let heap = &mut aheap().lock();
    let vec = heap.get(self.data_ref);
    let mut i = 0;
    let n = vec.len();
    while i<n {
      let d = vec.get(i).unwrap();
      if Data::equals(d.clone(),b.clone()) { return i as i64; }
      i = i + 1;
    }
    -1
  }
  
  /// Push data if not already in array
  pub fn push_unique(&self, b: Data) -> bool {
    {
      let heap = &mut aheap().lock();
      let vec = heap.get(self.data_ref);
      let mut i = 0;
      let n = vec.len();
      while i<n {
        let d = vec.get(i).unwrap();
        if Data::equals(d.clone(),b.clone()) { return false; }
        i = i + 1;
      }
      vec.push(b.clone());
    }
    if let Data::DObject(i) = &b {
      let _x = &mut oheap().lock().incr(*i);
    }
    else if let Data::DBytes(i) = &b {
      bheap().lock().incr(*i);
    }
    else if let Data::DArray(i) = &b {
      aheap().lock().incr(*i); 
    }
    true
  }
  
  /// Returns the index of a Data in the array
  pub fn remove_data(&self, b: Data) -> bool {
    let heap = &mut aheap().lock();
    let vec = heap.get(self.data_ref);
    let mut i = 0;
    let n = vec.len();
    while i<n {
      let d = vec.get(i).unwrap();
      if Data::equals(d.clone(),b.clone()) { 
        let old = vec.remove(i);
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
        return true;
      }
      i = i + 1;
    }
    false
  }
  
  /// Returns the indexed value from the array
  pub fn get_property(&self, id:usize) -> Data {
    let heap = &mut aheap().lock();
    let vec = heap.get(self.data_ref);
    let data = vec.get_mut(id).unwrap();
    data.clone()
  }
  
  /// Returns the indexed value from the array as a String
  pub fn get_string(&self, id:usize) -> String {
    self.get_property(id).string()
  }
  
  /// Returns the indexed value from the array as a bool
  pub fn get_bool(&self, id:usize) -> bool {
    self.get_property(id).boolean()
  }
  
  /// Returns the indexed value from the array as an i64
  pub fn get_i64(&self, id:usize) -> i64 {
    self.get_property(id).int()
  }
  
  /// Returns the indexed value from the array as an f64
  pub fn get_f64(&self, id:usize) -> f64 {
    let d = self.get_property(id);
    if d.is_int() { return d.int() as f64; }
    d.float()
  }

  /// Returns the indexed value from the array as a DataArray
  pub fn get_array(&self, id:usize) -> DataArray {
    self.get_property(id).array()
  }

  /// Returns the indexed value from the array as a DataObject
  pub fn get_object(&self, id:usize) -> DataObject {
    self.get_property(id).object()
  }

  /// Returns the indexed value from the array as a DataBytes
  pub fn get_bytes(&self, id:usize) -> DataBytes {
    self.get_property(id).bytes()
  }
  
  /// Append all values from another array
  pub fn join(&mut self, a:DataArray) {
    for val in a.objects() {
      self.push_property(val);
    }
  }
  
  /// Append the given value to the end of the array
  pub fn push_property(&mut self, data:Data) {
    if let Data::DObject(i) = &data {
      let _x = &mut oheap().lock().incr(*i);
    }
    else if let Data::DBytes(i) = &data {
      bheap().lock().incr(*i);
    }
    else if let Data::DArray(i) = &data {
      aheap().lock().incr(*i); 
    }
  
    let aheap = &mut aheap().lock();
    let vec = aheap.get(self.data_ref);
    vec.push(data);
  }

  /// Append the given ```String``` to the end of the array
  pub fn push_str(&mut self, val:&str) {
    self.push_property(Data::DString(val.to_string()));
  }
  
  /// Append the given ```bool``` to the end of the array
  pub fn push_bool(&mut self, val:bool) {
    self.push_property(Data::DBoolean(val));
  }
  
  /// Append the given ```i64``` to the end of the array
  pub fn push_i64(&mut self, val:i64) {
    self.push_property(Data::DInt(val));
  }
  
  /// Append the given ```f64``` to the end of the array
  pub fn push_float(&mut self, val:f64) {
    self.push_property(Data::DFloat(val));
  }

  /// Append the given ```DataObject``` to the end of the array
  pub fn push_object(&mut self, o:DataObject) {
    self.push_property(Data::DObject(o.data_ref));
  }
  
  #[deprecated(since="0.1.2", note="please use `push_array` instead")]  
  pub fn push_list(&mut self, a:DataArray) {
    self.push_property(Data::DArray(a.data_ref));
  }
  
  /// Append the given ```DataArray``` to the end of the array
  pub fn push_array(&mut self, a:DataArray) {
    self.push_property(Data::DArray(a.data_ref));
  }
  
  /// Append the given ```DataBytes``` to the end of the array
  pub fn push_bytes(&mut self, a:DataBytes) {
    self.push_property(Data::DBytes(a.data_ref));
  }
  
  /// Replace the indexed value in the array
  pub fn set_property(&mut self, id:usize, data:Data) {
    if let Data::DObject(i) = &data {
      let _x = &mut oheap().lock().incr(*i);
    }
    else if let Data::DBytes(i) = &data {
      bheap().lock().incr(*i);
    }
    else if let Data::DArray(i) = &data {
      aheap().lock().incr(*i); 
    }
  
    let aheap = &mut aheap().lock();
    let vec = aheap.get(self.data_ref);
    let old = vec[id].clone();
    vec[id] = data;
    
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
  
  /// Replace the indexed value in the array with the given ```String```.
  pub fn put_str(&mut self, id:usize, val:&str) {
    self.set_property(id, Data::DString(val.to_string()));
  }
  
  /// Replace the indexed value in the array with the given ```bool```.
  pub fn put_bool(&mut self, id:usize, val:bool) {
    self.set_property(id, Data::DBoolean(val));
  }
  
  /// Replace the indexed value in the array with the given ```i64```.
  pub fn put_i64(&mut self, id:usize, val:i64) {
    self.set_property(id, Data::DInt(val));
  }
  
  /// Replace the indexed value in the array with the given ```f64```.
  pub fn put_float(&mut self, id:usize, val:f64) {
    self.set_property(id, Data::DFloat(val));
  }

  /// Replace the indexed value in the array with the given ```DataObject```.
  pub fn put_object(&mut self, id:usize, o:DataObject) {
    self.set_property(id, Data::DObject(o.data_ref));
  }
  
  /// Replace the indexed value in the array with the given ```DataArray```.
  pub fn put_array(&mut self, id:usize, a:DataArray) {
    self.set_property(id, Data::DArray(a.data_ref));
  }
  
  /// Replace the indexed value in the array with the given ```DataBytes```.
  pub fn put_bytes(&mut self, id:usize, a:DataBytes) {
    self.set_property(id, Data::DBytes(a.data_ref));
  }
  
  /// Remove the indexed value from the array
  pub fn remove_property(&mut self, id:usize) {
    let aheap = &mut aheap().lock();
    let vec = aheap.get(self.data_ref);
    let old = vec.remove(id);
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
  
  /// **DO NOT USE**
  ///
  /// Reduces the reference count for this array by one, as well as the reference counts of any
  /// objects, arrays, or byte buffers contained in this array. This function should only be used
  /// externally by ```DataObject::gc()```.
  pub fn delete(aheap:&mut Heap<Vec<Data>>, data_ref:usize, oheap:&mut Heap<HashMap<String,Data>>) {
    let mut objects_to_kill = Vec::<usize>::new();
    let mut arrays_to_kill = Vec::<usize>::new();
    
    let n = aheap.count(data_ref);
    if n == 1 {
      let map = aheap.get(data_ref);
      for v in map {
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
    aheap.decr(data_ref);
    
    for i in objects_to_kill {
      DataObject::delete(oheap, i, aheap);
    }
    for i in arrays_to_kill {
      DataArray::delete(aheap, i, oheap);
    }
  }
  
  /// Returns this array as a ```Vec<Data>```. 
  pub fn objects(&self) -> Vec<Data> {
    let heap = &mut aheap().lock();
    let map = heap.get(self.data_ref);
    let mut vec = Vec::<Data>::new();
    for v in map {
      vec.push(v.clone());
    }
    vec
  }
  
  /// Prints the arrays currently stored in the heap
  pub fn print_heap() {
    println!("array {:?}", &aheap().lock());
  }
  
  /// Perform garbage collection. Arrays will not be removed from the heap until
  /// ```DataArray::gc()``` is called.
  pub fn gc() {
    let oheap = &mut &mut oheap().lock();
    let aheap = &mut aheap().lock();
    let adrop = &mut adrop().lock();
    let mut i = adrop.len();
    while i>0 {
      i = i - 1;
      let x = adrop.remove(0);
      DataArray::delete(aheap, x, oheap);
    }
  }
}

/// Adds this ```DataArray```'s data_ref to ODROP. Reference counts are adjusted when
/// ```DataArray::gc()``` is called.
impl Drop for DataArray {
  fn drop(&mut self) {
    adrop().lock().push(self.data_ref);
  }
}

