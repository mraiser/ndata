use crate::data::*;
use crate::dataarray::*;
use crate::dataobject::*;

/// Create a JSON string from a DataObject.
pub fn object_to_string(o:DataObject) -> String {
  let mut s = "{".to_string();
  let mut i = 0;
  for key in o.duplicate().keys(){
    if i>0 { s += "," }
    s += "\"";
    s += &escape(&key);
    s += "\":";
    let p = o.get_property(&key);
    if p.is_string() {
      s += "\"";
      s += &escape(&p.string());
      s += "\"";
    }
    else if p.is_object() {
      s += &object_to_string(p.object());
    }
    else if p.is_array() {
      s += &array_to_string(p.array());
    }
    else { s += &Data::as_string(p); }
    i += 1;
  }
  s += "}";
  s
}

/// Create a JSON string from a DataArray.
pub fn array_to_string(o:DataArray) -> String {
  let mut s = "[".to_string();
  let mut i = 0;
  for p in o.duplicate().objects(){
    if i>0 { s += "," }
    if p.is_string() {
      s += "\"";
      s += &escape(&p.string());
      s += "\"";
    }
    else if p.is_object() {
      s += &object_to_string(p.object());
    }
    else if p.is_array() {
      s += &array_to_string(p.array());
    }
    else { s += &Data::as_string(p); }
    i += 1;
  }
  s += "]";
  s
}

/// Create a new DataObject from a JSON string.
pub fn object_from_string(s:&str) -> DataObject {
  let (o, n) = extract_object(s);
  if n<s.len() { panic!("Error parsing DataObject, extra characters: {}", &s[n..]); }
  o
}

fn extract_object(s:&str) -> (DataObject, usize) {
  let mut o = DataObject::new();
  let s = s.trim();
  if !s.starts_with("{") { panic!("Error parsing DataObject from: {}", s); }
  
  let nn = s.len();
  let mut s = s[1..].trim();
  
  loop {
    if s.starts_with("}") { break; }
    let key = extract_string(s, '\"', '\"');
    if key.is_none() {
      if s != "" { panic!("Error parsing DataObject, unexpected characters: {}", s); }
      break;
    }
    let key = key.unwrap();
    let n = key.len();
    s = &s[n..].trim();
    let key = &key[1..n-1];

    if !s.starts_with(":") { panic!("Error parsing DataObject, expected ':' but got: {}", s); }
    s = s[1..].trim();
    
    let (val, n) = extract_value(s);
    o.set_property(&key, val.clone());
    if val.is_object() { val.object().decr(); }
    if val.is_array() { val.array().decr(); }
    s = s[n..].trim();
    if s.starts_with("}") { break; }
    if !s.starts_with(",") { panic!("Error parsing DataObject, expected ',' but got: {}", s); }
    s = s[1..].trim();
  }
  s = s[1..].trim();
  (o, nn-s.len())
}

/// Create a new DataArray from a JSON string.
pub fn array_from_string(s:&str) -> DataArray {
  let (o, n) = extract_array(s);
  if n<s.len() { panic!("Error parsing DataObject, extra characters: {}", &s[n..]); }
  o
}

fn extract_array(s:&str) -> (DataArray, usize) {
  let mut o = DataArray::new();
  let s = s.trim();
  if !s.starts_with("[") { panic!("Error parsing DataArray from: {}", s); }
  
  let nn = s.len();
  let mut s = s[1..].trim();
  
  loop {
    if s.starts_with("]") { break; }
    let (val, n) = extract_value(s);
    o.push_property(val.clone());
    if val.is_object() { val.object().decr(); }
    if val.is_array() { val.array().decr(); }
    s = s[n..].trim();
    if s.starts_with("]") { break; }
    if !s.starts_with(",") { panic!("Error parsing DataObject, expected ',' but got: {}", s); }
    s = s[1..].trim();    
  }
  s = s[1..].trim();
  (o, nn-s.len())
}

fn extract_string(s:&str, c1:char, c2:char) -> Option<String> {
  let ba = s.as_bytes();
  if ba[0] as char != c1 { return None; }
  let mut i = 1;
  let n = ba.len();
  let mut out = c1.to_string();
  let mut ignore = false;
  loop {
    if i == n { break; }
    let c = ba[i] as char;
    out.push(c);
    if !ignore {
      if c == c2 { break; }
      ignore = c == '\\';
    }
    else { ignore = false; }
    i += 1;
  }
  Some(out)
}

fn extract_value(s:&str) -> (Data, usize) {
  let n = s.len();
  if s.starts_with("\"") {
    let s = extract_string(s, '\"', '\"').unwrap();
    let n = s.len();
    let s = &s[1..n-1];
    let s = unescape(s);
    return (Data::DString(s.to_string()), n);
  }
  if s.starts_with("{") {
    let (o, n) = extract_object(&s);
    o.incr();
    return (Data::DObject(o.data_ref), n);
  }
  if s.starts_with("[") {
    let (o, n) = extract_array(&s);
    o.incr();
    return (Data::DArray(o.data_ref), n);
  }
  if n>=4 && s[0..4].to_lowercase() == "null" {
    return (Data::DNull, 4);
  }
  if n>=4 && s[0..4].to_lowercase() == "true" {
    return (Data::DBoolean(true), 4);
  }
  if n>=5 && s[0..5].to_lowercase() == "false" {
    return (Data::DBoolean(false), 5);
  }
  
  let ba = s.as_bytes();
  let mut out = "".to_string();
  let mut i = 0;
  loop {
    if i == n { break; }
    let c = ba[i] as char;
    if c == ',' || c == '}' || c == ']' { break; }
    out.push(c);
    i += 1;
  }
  if out.contains(".") {
    let f = out.trim().parse::<f64>().unwrap();
    return (Data::DFloat(f), i);
  }
  else {
    let f = out.trim().parse::<i64>().unwrap();
    return (Data::DInt(f), i);
  }
}

fn unescape(s:&str) -> String {
  let s = str::replace(&s, "\\\"", "\"");
//  let s = str::replace(&s, "\\b", "\b");
//  let s = str::replace(&s, "\\f", "\f");
  let s = str::replace(&s, "\\n", "\n");
  let s = str::replace(&s, "\\r", "\r");
  let s = str::replace(&s, "\\t", "\t");
  let s = str::replace(&s, "\\\\", "\\");
  s
}

fn escape(s:&str) -> String {
  let s = str::replace(&s, "\\", "\\\\");
  let s = str::replace(&s, "\"", "\\\"");
//  let s = str::replace(&s, "\b", "\\b");
//  let s = str::replace(&s, "\f", "\\f");
  let s = str::replace(&s, "\n", "\\n");
  let s = str::replace(&s, "\r", "\\r");
  let s = str::replace(&s, "\t", "\\t");
  s
}

