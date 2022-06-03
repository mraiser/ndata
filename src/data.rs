use crate::dataobject::*;
use crate::dataarray::*;
use crate::databytes::*;

#[derive(Debug)]
pub enum Data {
  DObject(usize),
  DArray(usize),
  DBytes(usize),
  DString(String),
  DBoolean(bool),
  DFloat(f64),
  DInt(i64),
  DNull,
}

impl Data {
  pub fn clone(&self) -> Data {
    if let Data::DInt(d) = self { return Data::DInt(*d); } 
    if let Data::DFloat(d) = self { return Data::DFloat(*d); } 
    if let Data::DBoolean(d) = self { return Data::DBoolean(*d); } 
    if let Data::DString(d) = self { return Data::DString(d.to_owned()); } 
    if let Data::DObject(d) = self { return Data::DObject(*d); } 
    if let Data::DArray(d) = self { return Data::DArray(*d); } 
    if let Data::DBytes(d) = self { return Data::DBytes(*d); } 
    Data::DNull 
  }
  
  pub fn is_number(&self) -> bool {
    self.is_int() || self.is_float()
  }
  
  pub fn is_int(&self) -> bool {
    if let Data::DInt(_i) = self { true } else { false }
  }
  
  pub fn is_float(&self) -> bool {
    if let Data::DFloat(_i) = self { true } else { false }
  }
  
  pub fn is_string(&self) -> bool {
    if let Data::DString(_i) = self { true } else { false }
  }
  
  pub fn is_boolean(&self) -> bool {
    if let Data::DBoolean(_i) = self { true } else { false }
  }
  
  pub fn is_object(&self) -> bool {
    if let Data::DObject(_i) = self { true } else { false }
  }
  
  pub fn is_array(&self) -> bool {
    if let Data::DArray(_i) = self { true } else { false }
  }
  
  pub fn is_bytes(&self) -> bool {
    if let Data::DBytes(_i) = self { true } else { false }
  }
  
  pub fn is_null(self) -> bool {
    if let Data::DNull = self { true } else { false }
  }
  
  pub fn int(&self) -> i64 {
    if let Data::DInt(i) = self { *i } else { panic!("Not an int"); }
  }

  pub fn float(&self) -> f64 {
    if let Data::DFloat(f) = self { *f } else { panic!("Not a float"); }
  }

  pub fn boolean(&self) -> bool {
    if let Data::DBoolean(b) = self { *b } else { panic!("Not a boolean"); }
  }

  pub fn string(&self) -> String {
    if let Data::DString(s) = self { s.to_owned() } else { panic!("Not a string"); }
  }

  pub fn object(&self) -> DataObject {
    if let Data::DObject(i) = self { DataObject::get(*i) } else { panic!("Not an object {:?}", self); }
  }

  pub fn array(&self) -> DataArray {
    if let Data::DArray(i) = self { DataArray::get(*i) } else { panic!("Not an array {:?}", self); }
  }
  
  pub fn bytes(&self) -> DataBytes {
    if let Data::DBytes(i) = self { DataBytes::get(*i) } else { panic!("Not a byte array {:?}", self); }
  }
  
  pub fn as_string(a:Data) -> String {
    if a.is_float() { return a.float().to_string(); }
    if a.is_int() { return a.int().to_string(); }
    if a.is_string() { return a.string(); }
    if a.is_boolean() { return a.boolean().to_string(); }
    if a.is_object() { return a.object().to_json().to_string(); }
    if a.is_array() { return a.array().to_json().to_string(); }
    if a.is_bytes() { return a.bytes().to_hex_string(); }
    if a.is_null() { return "null".to_string(); }
    "".to_string()
  }
}

impl Default for Data {
  fn default() -> Data {
    Data::DNull
  }
}

