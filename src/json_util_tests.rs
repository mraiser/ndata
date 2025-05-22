use crate::json_util::object_to_string;
use crate::json_util::object_from_string;
use crate::json_util::array_to_string;
use crate::json_util::array_from_string;
use crate::json_util::ParseError;
use crate::json_util::unescape;

// Place this code within your src/json_util.rs file or a dedicated test module.

#[cfg(test)]
mod tests {
  // Import necessary items from the parent module (where your json_util code resides)
  use super::*;
  // Assuming DataObject, DataArray, Data, DataBytes can be constructed for testing
  // You might need to adjust imports based on your crate structure
  use crate::data::*;
  use crate::dataarray::DataArray;
  use crate::databytes::DataBytes; // Assuming DataBytes is constructible/mockable
  use crate::dataobject::DataObject;
  // No longer need `use crate::ndata;` if init is directly in crate root (ndata.rs)

  // Helper to create a simple string Data variant
  fn d_string(s: &str) -> Data {
    Data::DString(s.to_string())
  }

  // --- Serialization Tests ---

  #[test]
  fn test_object_to_string_simple() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let mut obj = DataObject::new();
    obj.set_property("name", d_string("test"));
    obj.set_property("value", Data::DInt(123));
    obj.set_property("active", Data::DBoolean(true));
    obj.set_property("nothing", Data::DNull);
    obj.set_property("price", Data::DFloat(99.99));

    let json_string = object_to_string(obj.clone()); // Clone here as object_to_string takes ownership implicitly via write_object

    // Note: JSON object key order is not guaranteed, so we need a more robust check
    // than direct string comparison if the order might vary.
    // For simplicity here, we assume a consistent (though not guaranteed) order
    // or check for containment. A better approach involves parsing the output back.
    assert!(json_string.starts_with('{') && json_string.ends_with('}'));
    assert!(json_string.contains("\"name\":\"test\""));
    assert!(json_string.contains("\"value\":123"));
    assert!(json_string.contains("\"active\":true"));
    assert!(json_string.contains("\"nothing\":null"));
    assert!(json_string.contains("\"price\":99.99"));
    // obj.decr(); // No need to decr original obj if cloned for serialization
  }

  #[test]
  fn test_object_to_string_nested() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let mut inner_obj = DataObject::new();
    inner_obj.set_property("inner_key", d_string("inner_value"));

    let mut obj = DataObject::new();
    obj.set_property("outer_key", d_string("outer_value"));
    // Use the reference stored in Data::DObject
    obj.set_property("nested", Data::DObject(inner_obj.data_ref));

    let json_string = object_to_string(obj.clone()); // Clone for serialization
    // Again, check containment due to potential order variance
    assert!(json_string.contains("\"outer_key\":\"outer_value\""));
    assert!(json_string.contains("\"nested\":{\"inner_key\":\"inner_value\"}"));
    // inner_obj.decr(); // Decr only if not managed by outer obj's drop
    // obj.decr();
  }

  #[test]
  fn test_object_to_string_with_array() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let mut arr = DataArray::new();
    arr.push_property(Data::DInt(1));
    arr.push_property(d_string("two"));

    let mut obj = DataObject::new();
    // Use the reference stored in Data::DArray
    obj.set_property("list", Data::DArray(arr.data_ref));

    let json_string = object_to_string(obj.clone()); // Clone for serialization
    assert_eq!(json_string, "{\"list\":[1,\"two\"]}");
    // arr.decr(); // If needed
    // obj.decr();
  }

  #[test]
  fn test_object_to_string_escaped() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let mut obj = DataObject::new();
    obj.set_property("key\"with\\quotes", d_string("value\nwith\tescapes"));

    let json_string = object_to_string(obj.clone()); // Clone for serialization
    assert_eq!(json_string, r#"{"key\"with\\quotes":"value\nwith\tescapes"}"#);
    // obj.decr();
  }

  #[test]
  fn test_object_to_string_empty() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let obj = DataObject::new();
    let json_string = object_to_string(obj.clone()); // Clone for serialization
    assert_eq!(json_string, "{}");
    // obj.decr();
  }

  #[test]
  fn test_array_to_string_simple() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let mut arr = DataArray::new();
    arr.push_property(d_string("hello"));
    arr.push_property(Data::DInt(42));
    arr.push_property(Data::DBoolean(false));
    arr.push_property(Data::DNull);

    let json_string = array_to_string(arr.clone()); // Clone for serialization
    assert_eq!(json_string, r#"["hello",42,false,null]"#);
    // arr.decr();
  }

  #[test]
  fn test_array_to_string_nested() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let mut inner_arr = DataArray::new();
    inner_arr.push_property(Data::DInt(1));

    let mut inner_obj = DataObject::new();
    inner_obj.set_property("a", Data::DInt(2));

    let mut arr = DataArray::new();
    arr.push_property(Data::DArray(inner_arr.data_ref));
    arr.push_property(Data::DObject(inner_obj.data_ref));

    let json_string = array_to_string(arr.clone()); // Clone for serialization
    assert!(json_string.contains(r#"[1]"#));
    assert!(json_string.contains(r#"{"a":2}"#));
    assert!(json_string.starts_with('[') && json_string.ends_with(']'));
    // Clean up refs if necessary
    // inner_arr.decr();
    // inner_obj.decr();
    // arr.decr();
  }

  #[test]
  fn test_array_to_string_escaped() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let mut arr = DataArray::new();
    arr.push_property(d_string("string with \"quotes\" and \\ backslash"));

    let json_string = array_to_string(arr.clone()); // Clone for serialization
    assert_eq!(json_string, r#"["string with \"quotes\" and \\ backslash"]"#);
    // arr.decr();
  }

  #[test]
  fn test_array_to_string_empty() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let arr = DataArray::new();
    let json_string = array_to_string(arr.clone()); // Clone for serialization
    assert_eq!(json_string, "[]");
    // arr.decr();
  }

  // --- Deserialization Tests ---

  #[test]
  fn test_object_from_string_simple() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let json = r#" { "name" : "test", "value": 123, "active": true, "nothing": null, "price": 99.9 } "#;
    let result = object_from_string(json);
    assert!(result.is_ok());
    let obj = result.unwrap(); // obj now owns the parsed data

    assert_eq!(obj.get_property("name"), d_string("test"));
    assert_eq!(obj.get_property("value"), Data::DInt(123));
    assert_eq!(obj.get_property("active"), Data::DBoolean(true));
    assert_eq!(obj.get_property("nothing"), Data::DNull);
    assert_eq!(obj.get_property("price"), Data::DFloat(99.9));
    obj.decr(); // Clean up ref count for the object returned by from_string
  }

  #[test]
  fn test_object_from_string_nested() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let json = r#" { "outer": "value", "nested": { "inner": 1 } } "#;
    let result = object_from_string(json);
    assert!(result.is_ok());
    let obj = result.unwrap();

    assert_eq!(obj.get_property("outer"), d_string("value"));
    let nested_data = obj.get_property("nested");
    assert!(nested_data.is_object());
    // Use pattern matching to extract the object reference
    if let Data::DObject(obj_ref) = nested_data {
      // Assume DataObject::get returns the object directly (based on previous fixes)
      let nested_obj = DataObject::get(obj_ref);
      assert_eq!(nested_obj.get_property("inner"), Data::DInt(1));
      // nested_obj is temporary borrow from get, no decr needed here
    } else {
      panic!("Nested data is not a Data::DObject variant");
    }

    obj.decr(); // Clean up outer ref count
  }

  #[test]
  fn test_object_from_string_with_array() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let json = r#" { "list": [1, "two", false, null] } "#;
    let result = object_from_string(json);
    assert!(result.is_ok());
    let obj = result.unwrap();

    let list_data = obj.get_property("list");
    assert!(list_data.is_array());
    // Use pattern matching to extract the array reference
    if let Data::DArray(arr_ref) = list_data {
      // Assume DataArray::get returns the array directly
      let list_arr = DataArray::get(arr_ref);
      assert_eq!(list_arr.len(), 4);
      assert_eq!(list_arr.get_property(0), Data::DInt(1));
      assert_eq!(list_arr.get_property(1), d_string("two"));
      assert_eq!(list_arr.get_property(2), Data::DBoolean(false));
      assert_eq!(list_arr.get_property(3), Data::DNull);
      // list_arr is temporary borrow from get
    } else {
      panic!("List data is not a Data::DArray variant");
    }
    obj.decr();
  }

  #[test]
  fn test_object_from_string_escaped() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let json = r#" { "key\"\\" : "value\n\t\r\/" } "#;
    let result = object_from_string(json);
    assert!(result.is_ok());
    let obj = result.unwrap();

    assert_eq!(obj.get_property("key\"\\"), d_string("value\n\t\r/"));
    obj.decr();
  }

  #[test]
  fn test_object_from_string_unicode_escape() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let json = r#" { "unicode": "Hello\u0020World\u2764" } "#; // Space and Heart
    let result = object_from_string(json);
    // Assert ok first, then check content
    assert!(result.is_ok(), "Parsing failed with: {:?}", result.err());
    let obj = result.unwrap();
    assert_eq!(obj.get_property("unicode"), d_string("Hello World❤"));
    obj.decr();
  }

  #[test]
  fn test_object_from_string_empty() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let json = r"{}";
    let result = object_from_string(json);
    assert!(result.is_ok());
    let obj = result.unwrap();
    // Clone obj before calling keys() because keys() takes ownership
    assert_eq!(obj.clone().keys().len(), 0);
    obj.decr(); // Now decr the original obj
  }

  #[test]
  fn test_object_from_string_invalid_syntax() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let json = r#" { "key": value_not_quoted }"#;
    let result = object_from_string(json);
    assert!(result.is_err());
    // *** FIX: Extract error before assert ***
    let err = result.err().unwrap();
    assert!(matches!(err, ParseError::UnexpectedCharacter('v')), "Expected UnexpectedCharacter('v'), got {:?}", err);
  }

  #[test]
  fn test_object_from_string_missing_colon() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let json = r#" { "key" "value" }"#;
    let result = object_from_string(json);
    assert!(result.is_err());
    // *** FIX: Extract error before assert ***
    let err = result.err().unwrap();
    assert!(matches!(err, ParseError::UnexpectedCharacter('"')), "Expected UnexpectedCharacter('\"'), got {:?}", err);
  }

  #[test]
  fn test_object_from_string_trailing_comma() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    // Standard JSON typically disallows trailing commas
    let json = r#" { "key": 1, } "#;
    let result = object_from_string(json);
    assert!(result.is_err());
    // *** FIX: Extract error before assert ***
    let err = result.err().unwrap();
    // After comma, parser expects a string key starting with '"'
    assert!(matches!(err, ParseError::ExpectedCharacter('"')), "Expected ExpectedCharacter('\"'), got {:?}", err);
  }

  #[test]
  fn test_object_from_string_trailing_chars() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let json = r#" { "key": 1 } extra stuff "#;
    let result = object_from_string(json);
    assert!(result.is_err());
    assert!(matches!(result.err().unwrap(), ParseError::TrailingCharacters(_)));
  }

  #[test]
  fn test_array_from_string_simple() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let json = r#"[ 1, "two", true, null, 3.14 ]"#;
    let result = array_from_string(json);
    assert!(result.is_ok());
    let arr = result.unwrap();
    assert_eq!(arr.len(), 5);
    assert_eq!(arr.get_property(0), Data::DInt(1));
    assert_eq!(arr.get_property(1), d_string("two"));
    assert_eq!(arr.get_property(2), Data::DBoolean(true));
    assert_eq!(arr.get_property(3), Data::DNull);
    assert_eq!(arr.get_property(4), Data::DFloat(3.14));
    arr.decr();
  }

  #[test]
  fn test_array_from_string_nested() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let json = r#"[ [1, 2], {"a": "b"} ]"#;
    let result = array_from_string(json);
    assert!(result.is_ok());
    let arr = result.unwrap();

    let inner_arr_data = arr.get_property(0);
    assert!(inner_arr_data.is_array());
    // Use pattern matching to extract the array reference
    if let Data::DArray(inner_arr_ref) = inner_arr_data {
      let inner_arr = DataArray::get(inner_arr_ref);
      assert_eq!(inner_arr.len(), 2);
      assert_eq!(inner_arr.get_property(0), Data::DInt(1));
      assert_eq!(inner_arr.get_property(1), Data::DInt(2)); // Corrected expectation
    } else {
      panic!("Inner array data is not a Data::DArray variant");
    }


    let inner_obj_data = arr.get_property(1);
    assert!(inner_obj_data.is_object());
    // Use pattern matching to extract the object reference
    if let Data::DObject(inner_obj_ref) = inner_obj_data {
      let inner_obj = DataObject::get(inner_obj_ref);
      assert_eq!(inner_obj.get_property("a"), d_string("b"));
    } else {
      panic!("Inner object data is not a Data::DObject variant");
    }

    arr.decr();
  }

  #[test]
  fn test_array_from_string_empty() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let json = r"[]";
    let result = array_from_string(json);
    assert!(result.is_ok());
    let arr = result.unwrap();
    assert_eq!(arr.len(), 0);
    arr.decr();
  }

  #[test]
  fn test_array_from_string_invalid_syntax() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let json = r#"[ 1, "two" false ]"#; // Missing comma
    let result = array_from_string(json);
    assert!(result.is_err());
    // *** FIX: Extract error before assert ***
    let err = result.err().unwrap();
    // After "two", parser expects ',' or ']'. Finds 'f'.
    assert!(matches!(err, ParseError::UnexpectedCharacter('f')), "Expected UnexpectedCharacter('f'), got {:?}", err);
  }

  #[test]
  fn test_array_from_string_trailing_comma() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    // Standard JSON typically disallows trailing commas
    let json = r#"[ 1, 2, ]"#;
    let result = array_from_string(json);
    assert!(result.is_err());
    // *** FIX: Extract error before assert ***
    let err = result.err().unwrap();
    // After comma, parser expects a value
    assert!(matches!(err, ParseError::ExpectedValue), "Expected ExpectedValue, got {:?}", err);
  }

  #[test]
  fn test_array_from_string_trailing_chars() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    let json = r#"[ 1, 2 ] extra"#;
    let result = array_from_string(json);
    assert!(result.is_err());
    assert!(matches!(result.err().unwrap(), ParseError::TrailingCharacters(_)));
  }

  // --- Unescape Tests ---
  // These tests should not require ndata initialization

  #[test]
  fn test_unescape_basic() {
    assert_eq!(unescape(r#"hello world"#).unwrap(), "hello world");
    assert_eq!(unescape(r#"hello\"world"#).unwrap(), "hello\"world");
    assert_eq!(unescape(r#"hello\\world"#).unwrap(), "hello\\world");
    assert_eq!(unescape(r#"hello\/world"#).unwrap(), "hello/world");
    assert_eq!(unescape(r#"hello\nworld"#).unwrap(), "hello\nworld");
    assert_eq!(unescape(r#"hello\rworld"#).unwrap(), "hello\rworld");
    assert_eq!(unescape(r#"hello\tworld"#).unwrap(), "hello\tworld");
    assert_eq!(unescape(r#"hello\bworld"#).unwrap(), "hello\x08world");
    assert_eq!(unescape(r#"hello\fworld"#).unwrap(), "hello\x0cworld");
  }

  #[test]
  fn test_unescape_unicode() {
    assert_eq!(unescape(r#"\u0048\u0065\u006C\u006C\u006F"#).unwrap(), "Hello"); // Hello
    assert_eq!(unescape(r#"\u2764"#).unwrap(), "❤"); // Heart
    // Rust strings handle valid surrogate pairs automatically during char::from_u32
    // Test with a character requiring surrogate pairs
    assert_eq!(unescape(r#"\uD83D\uDE00"#).unwrap(), "😀"); // Grinning Face
  }

  #[test]
  fn test_unescape_mixed() {
    let result = unescape(r#"\"\\\/\b\f\n\r\t\u0041"#);
    assert!(result.is_ok(), "Unescape failed: {:?}", result.err());
    assert_eq!(result.unwrap(), "\"\\/\x08\x0c\n\r\tA");
  }

  #[test]
  fn test_unescape_invalid_escape() {
    assert!(unescape(r#"\q"#).is_err());
    assert!(matches!(unescape(r#"\q"#).err().unwrap(), ParseError::InvalidEscapeSequence(_)));
    // Check EOF after backslash
    assert!(unescape(r#"hello\"#).is_err());
    assert!(matches!(unescape(r#"hello\"#).err().unwrap(), ParseError::UnexpectedEof));
  }

  #[test]
  fn test_unescape_invalid_unicode() {
    assert!(unescape(r#"\u123"#).is_err()); // Too short
    assert!(matches!(unescape(r#"\u123"#).err().unwrap(), ParseError::InvalidUnicodeEscape(_)));
    assert!(unescape(r#"\u123G"#).is_err()); // Invalid hex char
    assert!(matches!(unescape(r#"\u123G"#).err().unwrap(), ParseError::InvalidUnicodeEscape(_)));
    assert!(unescape(r#"\u"#).is_err());     // EOF after \u
    assert!(matches!(unescape(r#"\u"#).err().unwrap(), ParseError::InvalidUnicodeEscape(_)));
    // Invalid code point (lone high surrogate)
    assert!(unescape(r#"\uD800"#).is_err());
    assert!(matches!(unescape(r#"\uD800"#).err().unwrap(), ParseError::InvalidUnicodeEscape(_)));
    assert!(unescape(r#"\uZZZZ"#).is_err()); // Invalid hex
    assert!(matches!(unescape(r#"\uZZZZ"#).err().unwrap(), ParseError::InvalidUnicodeEscape(_)));
  }

  #[test]
  fn test_unescape_prohibited_chars() {
    // Control characters U+0000 to U+001F must be escaped
    assert!(unescape("\x01").is_err());
    assert!(matches!(unescape("\x1f").err().unwrap(), ParseError::UnexpectedCharacter(_)));
    // Should be fine if escaped
    assert_eq!(unescape(r#"\u0001"#).unwrap(), "\x01");
  }

  // --- DataBytes Serialization Test (Example) ---
  // This depends heavily on how DataBytes::to_hex_string() is implemented
  // And how DataBytes are created/managed
  #[test]
  #[ignore] // Ignore this test until DataBytes implementation is clear
  fn test_databytes_serialization() {
    crate::init(); // Initialize ndata shared state from crate root (ndata.rs)
    // Assuming DataBytes::from_vec exists and returns DataBytes or similar
    // let bytes_vec = vec![0xDE, 0xAD, 0xBE, 0xEF];
    // let bytes = DataBytes::from_vec(bytes_vec); // Creates and stores bytes, returns DataBytes instance
    // let mut obj = DataObject::new();
    // obj.set_property("raw", Data::DBytes(bytes.data_ref)); // Store the ref
    // let json_string = object_to_string(obj.clone()); // Clone obj for serialization
    // // Assuming DataBytes::get(ref) retrieves the DataBytes instance,
    // // and DataBytes::to_hex_string() exists on it.
    // assert_eq!(json_string, r#"{"raw":"deadbeef"}"#);
    // // obj.decr(); // Original obj ref count managed by drop
    // // bytes.decr(); // If DataBytes::from_vec increments ref count
  }

}
