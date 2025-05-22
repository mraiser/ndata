// #[cfg(feature="no_std_support")] // Keep this if you need it for alloc
extern crate alloc;

// Keep existing imports, assuming they are correct for your crate structure
use crate::data::*;
use crate::dataarray::*;
use crate::databytes::*;
use crate::dataobject::*;

use core::fmt; // Use core::fmt for no_std compatibility if needed, otherwise std::fmt
use alloc::string::{String, ToString};
//use alloc::vec::Vec; // Needed for character collection in unescaping and format!


// --- Error Type ---

/// Error type for JSON parsing failures.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ParseError {
  /// Unexpected end of input data.
  UnexpectedEof,
  /// Unexpected character encountered.
  UnexpectedCharacter(char),
  /// Expected a specific character, but found something else.
  ExpectedCharacter(char),
  /// Expected a JSON value (string, number, bool, null, object, array).
  ExpectedValue,
  /// Expected a comma separator in an array or object.
  ExpectedComma,
  /// Expected a colon separator between key and value in an object.
  ExpectedColon,
  /// Invalid JSON string escape sequence.
  InvalidEscapeSequence(String),
  /// Invalid Unicode escape sequence (\uXXXX).
  InvalidUnicodeEscape(String),
  /// Invalid number format.
  InvalidNumber(String),
  /// Trailing characters found after the main JSON value.
  TrailingCharacters(String),
  /// General parsing failure with a message.
  Message(String), // Use alloc::string::String for no_std
}

// Implement Display for ParseError (optional but helpful)
impl fmt::Display for ParseError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      ParseError::UnexpectedEof => write!(f, "Unexpected end of input"),
      ParseError::UnexpectedCharacter(c) => write!(f, "Unexpected character: '{}'", c),
      ParseError::ExpectedCharacter(c) => write!(f, "Expected character: '{}'", c),
      ParseError::ExpectedValue => write!(f, "Expected JSON value"),
      ParseError::ExpectedComma => write!(f, "Expected comma separator"),
      ParseError::ExpectedColon => write!(f, "Expected colon separator"),
      ParseError::InvalidEscapeSequence(s) => write!(f, "Invalid escape sequence: {}", s),
      ParseError::InvalidUnicodeEscape(s) => write!(f, "Invalid Unicode escape sequence: {}", s),
      ParseError::InvalidNumber(s) => write!(f, "Invalid number format: {}", s),
      ParseError::TrailingCharacters(s) => write!(f, "Trailing characters after JSON value: {}", s),
      ParseError::Message(msg) => write!(f, "JSON parsing error: {}", msg),
    }
  }
}

// If not using std, you might need to implement Error trait manually or conditionally
#[cfg(not(feature="no_std_support"))]
impl std::error::Error for ParseError {}


// --- Serialization ---

/// Create a JSON string from a DataObject.
///
/// Note: This function currently serializes `Data::DBytes` by attempting to
/// interpret them as UTF-8 strings, similar to `Data::DString`. This may
/// lead to errors or Mojibake if the bytes are not valid UTF-8.
/// Consider Base64 encoding for robust binary data handling if needed.
pub fn object_to_string(o: DataObject) -> String {
  let mut s = String::new(); // Consider String::with_capacity for estimation
  // Use a helper that takes a Write trait object
  // Clone the object here if write_object needs to consume it,
  // but write_object takes &DataObject, so cloning happens inside if needed.
  write_object(&mut s, &o).expect("Writing to String should not fail");
  s
}

/// Create a JSON string from a DataArray.
pub fn array_to_string(a: DataArray) -> String {
  let mut s = String::new(); // Consider String::with_capacity for estimation
  // Clone the array here if write_array needs to consume it,
  // but write_array takes &DataArray, so cloning happens inside if needed.
  write_array(&mut s, &a).expect("Writing to String should not fail");
  s
}

// Helper function using fmt::Write for efficient string building
fn write_object<W: fmt::Write>(writer: &mut W, o: &DataObject) -> fmt::Result {
  writer.write_char('{')?;
  let mut first = true;
  // Clone `o` because `keys()` takes ownership (self) and `o` is a shared reference.
  for key in o.clone().keys() {
    if !first {
      writer.write_char(',')?;
    }
    first = false;
    writer.write_char('"')?;
    write_escaped_str(writer, &key)?;
    writer.write_char('"')?;
    writer.write_char(':')?;
    // Assume get_property returns a borrow or cheap clone of Data
    let p = o.get_property(&key);
    write_data(writer, &p)?;
  }
  writer.write_char('}')
}

// Helper function using fmt::Write for efficient string building
fn write_array<W: fmt::Write>(writer: &mut W, a: &DataArray) -> fmt::Result {
  writer.write_char('[')?;
  let mut first = true;
  // Clone `a` because `objects()` likely takes ownership (self) and `a` is a shared reference.
  // Assuming `objects()` signature is similar to `keys()`.
  for p in a.clone().objects() {
    if !first {
      writer.write_char(',')?;
    }
    first = false;
    write_data(writer, &p)?;
  }
  writer.write_char(']')
}

// Recursive helper to write any Data variant
fn write_data<W: fmt::Write>(writer: &mut W, data: &Data) -> fmt::Result {
  match data {
    Data::DNull => writer.write_str("null"),
    Data::DBoolean(b) => writer.write_str(if *b { "true" } else { "false" }),
    Data::DInt(i) => write!(writer, "{}", i),
    Data::DFloat(f) => write!(writer, "{}", f), // Consider precision/format needs
    Data::DString(s) => {
      writer.write_char('"')?;
      write_escaped_str(writer, s)?;
      writer.write_char('"')
    }
    // Compatibility: Treat bytes as string. Might fail or corrupt if not UTF-8.
    Data::DBytes(bytes_ref) => {
      // Retrieve the actual bytes. Assuming DataBytes::get exists and returns Vec<u8> or &[u8]
      // This part depends heavily on how DataBytes works internally.
      // Example:
      let bytes_data = DataBytes::get(*bytes_ref); // Get actual bytes
      // Convert bytes to hex string for JSON compatibility
      let s = bytes_data.to_hex_string(); // Assuming DataBytes has this method
      writer.write_char('"')?;
      // Hex strings don't need JSON escaping beyond the surrounding quotes
      writer.write_str(&s)?;
      // write_escaped_str(writer, &s)?; // Escaping hex might be incorrect
      writer.write_char('"')
    }
    Data::DObject(obj_ref) => {
      // Retrieve the actual object.
      // Assuming DataObject::get returns DataObject directly based on compiler error.
      // If get fails on invalid ref, it might panic or return a default object.
      let obj = DataObject::get(*obj_ref);
      // Pass by reference, write_object will clone internally if needed for iteration
      write_object(writer, &obj)
    }
    Data::DArray(arr_ref) => {
      // Retrieve the actual array.
      // Assuming DataArray::get returns DataArray directly based on compiler error pattern.
      let arr = DataArray::get(*arr_ref);
      // Pass by reference, write_array will clone internally if needed for iteration
      write_array(writer, &arr)
    }
    // Handle other Data variants if they exist
    // _ => writer.write_str("\"<unsupported data type>\"")
  }
}

// Helper to write JSON escaped string efficiently
fn write_escaped_str<W: fmt::Write>(writer: &mut W, s: &str) -> fmt::Result {
  for c in s.chars() {
    match c {
      '"'  => writer.write_str("\\\"")?,
      '\\' => writer.write_str("\\\\")?,
      // Optional but good practice:
      '/'  => writer.write_str("\\/")?, // Avoids issues with </script>
      // Required by JSON spec:
      '\x08' => writer.write_str("\\b")?, // Backspace
      '\x0c' => writer.write_str("\\f")?, // Form feed
      '\n' => writer.write_str("\\n")?, // Newline
      '\r' => writer.write_str("\\r")?, // Carriage return
      '\t' => writer.write_str("\\t")?, // Tab
      // Handle control characters (U+0000 to U+001F)
      '\x00'..='\x1f' => write!(writer, "\\u{:04x}", c as u32)?,
      // All other characters are safe
      _ => writer.write_char(c)?,
    }
  }
  Ok(())
}


// --- Deserialization ---

/// Create a new DataObject from a JSON string. Returns `ParseError` on failure.
pub fn object_from_string(s: &str) -> Result<DataObject, ParseError> {
  let mut input = s.trim();
  if input.is_empty() {
    // Handle empty input specifically if needed, maybe return an empty object?
    // Or return an error. Current parse_object expects '{'.
    return Err(ParseError::UnexpectedEof);
  }
  let (obj, remaining) = parse_object(&mut input)?;
  if !remaining.trim().is_empty() {
    // Decrement refs if object creation succeeded but there's trailing data
    obj.decr(); // Decrement the ref count taken by parse_object
    Err(ParseError::TrailingCharacters(remaining.trim().to_string()))
  } else {
    Ok(obj)
  }
}

/// Create a new DataArray from a JSON string. Returns `ParseError` on failure.
pub fn array_from_string(s: &str) -> Result<DataArray, ParseError> {
  let mut input = s.trim();
  if input.is_empty() {
    return Err(ParseError::UnexpectedEof);
  }
  let (arr, remaining) = parse_array(&mut input)?;
  if !remaining.trim().is_empty() {
    // Decrement refs if array creation succeeded but there's trailing data
    arr.decr(); // Decrement the ref count taken by parse_array
    Err(ParseError::TrailingCharacters(remaining.trim().to_string()))
  } else {
    Ok(arr)
  }
}

// --- Unescape Function ---

/// Helper function to parse 4 hex digits from a character iterator.
fn parse_hex4<I>(chars: &mut I) -> Result<u32, ParseError>
where
I: Iterator<Item = char>,
{
  let mut hex_str = String::with_capacity(4);
  for _ in 0..4 {
    match chars.next() {
      Some(hc) if hc.is_ascii_hexdigit() => {
        hex_str.push(hc);
      }
      Some(bad_char) => {
        return Err(ParseError::InvalidUnicodeEscape(format!(
          "\\u{}<-- invalid char '{}'", hex_str, bad_char
        )));
      }
      None => {
        return Err(ParseError::InvalidUnicodeEscape(format!(
          "\\u{} (unexpected EOF)", hex_str
        )));
      }
    }
  }
  u32::from_str_radix(&hex_str, 16)
  .map_err(|_| ParseError::InvalidUnicodeEscape(format!("\\u{} (internal parsing failed)", hex_str)))
}


/// Unescapes a string slice that represents the *content* of a JSON string
/// (without the surrounding quotes). Handles standard JSON escapes like \n, \t, \\, \", \uXXXX etc.
/// Also handles UTF-16 surrogate pairs (e.g., \uD83D\uDE00).
///
/// Returns an error if an invalid escape sequence is found.
pub fn unescape(s: &str) -> Result<String, ParseError> {
  let mut output = String::with_capacity(s.len()); // Pre-allocate close to final size
  let mut chars = s.chars().peekable(); // Use peekable iterator

  while let Some(c) = chars.next() {
    if c == '\\' {
      // Escape sequence
      match chars.next() {
        Some('"') => output.push('"'),
        Some('\\') => output.push('\\'),
        Some('/') => output.push('/'),
        Some('b') => output.push('\x08'), // Backspace
        Some('f') => output.push('\x0c'), // Form feed
        Some('n') => output.push('\n'),   // Newline
        Some('r') => output.push('\r'),   // Carriage return
        Some('t') => output.push('\t'),   // Tab
        Some('u') => {
          // *** SURROGATE PAIR HANDLING START ***
          let code1 = parse_hex4(&mut chars)?;

          // Check if it's a high surrogate (U+D800 to U+DBFF)
          if (0xD800..=0xDBFF).contains(&code1) {
            // Look ahead for a low surrogate pair: \uXXXX
            if chars.peek() == Some(&'\\') {
              chars.next(); // Consume '\'
              if chars.peek() == Some(&'u') {
                chars.next(); // Consume 'u'
                let code2 = parse_hex4(&mut chars)?;

                // Check if it's a low surrogate (U+DC00 to U+DFFF)
                if (0xDC00..=0xDFFF).contains(&code2) {
                  // Combine the surrogate pair
                  let combined = (((code1 - 0xD800) * 0x400) + (code2 - 0xDC00)) + 0x10000;
                  match core::char::from_u32(combined) {
                    Some(unicode_char) => output.push(unicode_char),
                    None => return Err(ParseError::InvalidUnicodeEscape(format!(
                      "\\u{:04X}\\u{:04X} (combined to invalid code point {})", code1, code2, combined
                    ))),
                  }
                } else {
                  // High surrogate was followed by \u but not a low surrogate
                  return Err(ParseError::InvalidUnicodeEscape(format!(
                    "\\u{:04X} followed by non-low surrogate \\u{:04X}", code1, code2
                  )));
                }
              } else {
                // High surrogate was followed by \ but not u
                return Err(ParseError::InvalidUnicodeEscape(format!(
                  "\\u{:04X} followed by invalid escape sequence", code1
                )));
              }
            } else {
              // High surrogate was not followed by another escape sequence (\u...)
              return Err(ParseError::InvalidUnicodeEscape(format!(
                "Lone high surrogate \\u{:04X}", code1
              )));
            }
          } else {
            // Not a surrogate, just a regular \uXXXX sequence
            match core::char::from_u32(code1) {
              Some(unicode_char) => output.push(unicode_char),
              None => return Err(ParseError::InvalidUnicodeEscape(format!(
                "\\u{:04X} (invalid code point)", code1
              ))),
            }
          }
          // *** SURROGATE PAIR HANDLING END ***
        }
        Some(other) => return Err(ParseError::InvalidEscapeSequence(format!("\\{}", other))),
        None => return Err(ParseError::UnexpectedEof), // EOF after backslash
      }
    } else {
      // Regular character - check for prohibited control characters
      if ('\x00'..='\x1f').contains(&c) {
        return Err(ParseError::UnexpectedCharacter(c));
      }
      output.push(c);
    }
  }

  Ok(output)
}


// --- Parsing Helper Functions ---

// Consume whitespace from the beginning of the slice.
fn skip_whitespace(input: &mut &str) {
  *input = input.trim_start();
}

// Consume the next character if it matches `expected`.
fn consume_char(input: &mut &str, expected: char) -> Result<(), ParseError> {
  if input.starts_with(expected) {
    *input = &input[expected.len_utf8()..];
    Ok(())
  } else {
    // Provide the character found for better error messages
    let found = input.chars().next();
    match found {
      Some(c) => Err(ParseError::UnexpectedCharacter(c)), // More specific than ExpectedCharacter
      None => Err(ParseError::UnexpectedEof),
    }
    // Err(ParseError::ExpectedCharacter(expected)) // Original less specific error
  }
}

// Parse a JSON string, handling escapes, and return the *unescaped* content.
// Input slice `input` should start *after* the opening quote.
// *** REFACTORED to unescape directly ***
fn parse_string_content(input: &mut &str) -> Result<String, ParseError> {
  let mut output = String::new(); // Consider with_capacity if average length is known
  let mut consumed_bytes = 0;
  //let initial_len = input.len();

  // Helper to parse 4 hex digits directly from the input slice
  fn parse_hex4_slice(slice: &str) -> Result<(u32, usize), ParseError> {
    if slice.len() < 4 {
      return Err(ParseError::InvalidUnicodeEscape(format!(
        "\\u{}... (unexpected EOF)", slice
      )));
    }
    let hex_str = &slice[..4];
    match u32::from_str_radix(hex_str, 16) {
      Ok(code) => Ok((code, 4)),
      Err(_) => Err(ParseError::InvalidUnicodeEscape(format!(
        "\\u{} (parsing failed)", hex_str
      ))),
    }
  }

  loop {
    // Find the next special character (\ or ") or end of input
    let current_slice = &input[consumed_bytes..];
    let next_special = current_slice.find(|c: char| c == '\\' || c == '"');

    match next_special {
      Some(index) => {
        // Process the character at the index
        let special_char = current_slice[index..].chars().next().unwrap(); // Safe due to find()

        // Append the segment before the special character
        output.push_str(&current_slice[..index]);
        consumed_bytes += index; // Move past the appended segment

        if special_char == '"' {
          // End of string found
          consumed_bytes += '"'.len_utf8(); // Consume the closing quote
          break; // Exit loop
        } else {
          // It must be a backslash (\)
          consumed_bytes += '\\'.len_utf8(); // Consume the backslash

          // Now process the escape sequence
          let escape_slice = &input[consumed_bytes..];
          let mut escape_chars = escape_slice.chars();
          match escape_chars.next() {
            Some('"') => { output.push('"'); consumed_bytes += '"'.len_utf8(); }
            Some('\\') => { output.push('\\'); consumed_bytes += '\\'.len_utf8(); }
            Some('/') => { output.push('/'); consumed_bytes += '/'.len_utf8(); }
            Some('b') => { output.push('\x08'); consumed_bytes += 'b'.len_utf8(); }
            Some('f') => { output.push('\x0c'); consumed_bytes += 'f'.len_utf8(); }
            Some('n') => { output.push('\n'); consumed_bytes += 'n'.len_utf8(); }
            Some('r') => { output.push('\r'); consumed_bytes += 'r'.len_utf8(); }
            Some('t') => { output.push('\t'); consumed_bytes += 't'.len_utf8(); }
            Some('u') => { // Unicode escape \uXXXX
              consumed_bytes += 'u'.len_utf8(); // Consume 'u'
              let (code1, hex_len1) = parse_hex4_slice(&input[consumed_bytes..])?;
              consumed_bytes += hex_len1;

              // Check for surrogate pair
              if (0xD800..=0xDBFF).contains(&code1) {
                // Check if the next chars are \u
                if input.get(consumed_bytes..consumed_bytes + 2) == Some("\\u") {
                  consumed_bytes += 2; // Consume \u
                  let (code2, hex_len2) = parse_hex4_slice(&input[consumed_bytes..])?;
                  consumed_bytes += hex_len2;

                  if (0xDC00..=0xDFFF).contains(&code2) {
                    // Valid surrogate pair
                    let combined = (((code1 - 0xD800) * 0x400) + (code2 - 0xDC00)) + 0x10000;
                    match core::char::from_u32(combined) {
                      Some(unicode_char) => output.push(unicode_char),
                      None => return Err(ParseError::InvalidUnicodeEscape(format!(
                        "\\u{:04X}\\u{:04X} (combined to invalid code point {})", code1, code2, combined
                      ))),
                    }
                  } else {
                    // High surrogate followed by \u but not a low surrogate
                    return Err(ParseError::InvalidUnicodeEscape(format!(
                      "\\u{:04X} followed by non-low surrogate \\u{:04X}", code1, code2
                    )));
                  }
                } else {
                  // High surrogate not followed by \u
                  return Err(ParseError::InvalidUnicodeEscape(format!(
                    "Lone high surrogate \\u{:04X}", code1
                  )));
                }
              } else {
                // Not a surrogate, just a regular \uXXXX
                match core::char::from_u32(code1) {
                  Some(unicode_char) => output.push(unicode_char),
                  None => return Err(ParseError::InvalidUnicodeEscape(format!(
                    "\\u{:04X} (invalid code point)", code1
                  ))),
                }
              }
            }
            Some(other) => return Err(ParseError::InvalidEscapeSequence(format!("\\{}", other))),
            None => return Err(ParseError::UnexpectedEof), // EOF after backslash
          }
        }
      }
      None => {
        // No more special characters found, but string hasn't terminated
        return Err(ParseError::UnexpectedEof); // Unterminated string
      }
    }
  }

  // Update the input slice to point after the consumed part (content + closing quote)
  *input = &input[consumed_bytes..];
  Ok(output)
}


// Parse a JSON number (integer or float)
fn parse_number(input: &mut &str) -> Result<Data, ParseError> {
  skip_whitespace(input);

  let mut len = 0;
  let mut has_dot = false;
  let mut has_exp = false;

  // Find the end of the number sequence according to JSON rules
  for c in input.chars() {
    match c {
      '0'..='9' => len += c.len_utf8(),
      '.' if !has_dot => { // Allow only one dot
        has_dot = true;
        len += c.len_utf8();
      }
      'e' | 'E' if !has_exp => { // Allow only one exponent
        has_exp = true;
        has_dot = true; // Exponent makes it a float implicitly
        len += c.len_utf8();
        // Check for optional '+' or '-' after 'e'/'E'
        if let Some(sign) = input.get(len..).and_then(|s| s.chars().next()) {
          if sign == '+' || sign == '-' {
            len += sign.len_utf8();
          }
        }
      }
      _ => break, // End of number
    }
  }

  if len == 0 {
    return Err(ParseError::ExpectedValue); // Or a more specific number error
  }

  let num_str = &input[..len];
  *input = &input[len..]; // Consume the number string

  // Try parsing as i64 first if it looks like an integer
  if !has_dot && !has_exp {
    if let Ok(i) = num_str.parse::<i64>() {
      return Ok(Data::DInt(i));
    }
    // Fall through to f64 if i64 parsing failed (e.g., too large)
    // but it looked like an integer
  }

  // Try parsing as f64
  if let Ok(f) = num_str.parse::<f64>() {
    Ok(Data::DFloat(f))
  } else {
    Err(ParseError::InvalidNumber(num_str.to_string()))
  }
}


// Parse a JSON value (string, number, boolean, null, object, array)
// Returns the parsed Data and the remaining slice with the input's lifetime.
fn parse_value<'a>(input: &mut &'a str) -> Result<(Data, &'a str), ParseError> {
  skip_whitespace(input);

  if input.is_empty() {
    return Err(ParseError::UnexpectedEof);
  }

  // Use peekable to check without consuming yet, helps with number vs other cases
  let first_char = match input.chars().next() {
    Some(c) => c,
    None => return Err(ParseError::UnexpectedEof), // Should be caught by is_empty, but defensive
  };

  match first_char {
    // String
    '"' => {
      consume_char(input, '"')?; // Consume opening quote
      let content = parse_string_content(input)?; // Consumes content and closing quote
      // Return the remaining input slice with its original lifetime 'a
      Ok((Data::DString(content), *input))
    }
    // Object
    '{' => {
    let (obj, remaining) = parse_object(input)?;
    // IMPORTANT: Increment ref count for the returned object
    // Assuming `Data::DObject` stores the ref (`usize`)
    obj.incr();
    // Return the remaining input slice with its original lifetime 'a
    Ok((Data::DObject(obj.data_ref), remaining))
    }
    // Array
    '[' => {
      let (arr, remaining) = parse_array(input)?;
      // IMPORTANT: Increment ref count for the returned array
      arr.incr();
      // Return the remaining input slice with its original lifetime 'a
      Ok((Data::DArray(arr.data_ref), remaining))
    }
    // Boolean or Null
    't' => {
      if input.starts_with("true") {
        *input = &input["true".len()..];
        // Return the remaining input slice with its original lifetime 'a
        Ok((Data::DBoolean(true), *input))
      } else {
        // If it starts with 't' but isn't 'true', it's unexpected
        Err(ParseError::UnexpectedCharacter('t')) // More specific error
      }
    }
    'f' => {
      if input.starts_with("false") {
        *input = &input["false".len()..];
        // Return the remaining input slice with its original lifetime 'a
        Ok((Data::DBoolean(false), *input))
      } else {
        Err(ParseError::UnexpectedCharacter('f'))
      }
    }
    'n' => {
      if input.starts_with("null") {
        *input = &input["null".len()..];
        // Return the remaining input slice with its original lifetime 'a
        Ok((Data::DNull, *input))
      } else {
        Err(ParseError::UnexpectedCharacter('n'))
      }
    }
    // Number
    '-' | '0'..='9' => {
      let num_data = parse_number(input)?;
      // Return the remaining input slice with its original lifetime 'a
      Ok((num_data, *input))
    }
    // Invalid start character for a value
    _ => Err(ParseError::UnexpectedCharacter(first_char)), // Changed from ExpectedValue
  }
}

// Parse a JSON object: { "key": value, ... }
// Returns the parsed DataObject and the remaining slice
#[allow(unused_assignments)]
fn parse_object<'a>(input: &mut &'a str) -> Result<(DataObject, &'a str), ParseError> {
  consume_char(input, '{')?;
  skip_whitespace(input);

  let mut obj = DataObject::new(); // Create the object

  let mut first = true;

  // Check for empty object
  if input.starts_with('}') {
    consume_char(input, '}')?;
    return Ok((obj, *input));
  }

  loop {
    if !first {
      // Expect a comma
      skip_whitespace(input);
      // Check for closing brace before consuming comma
      if input.starts_with('}') {
        obj.decr(); // Clean up
        return Err(ParseError::ExpectedComma); // Comma was expected before }
      }
      consume_char(input, ',')?;
      skip_whitespace(input);
    }

    // Check for closing brace after comma (or for first element)
    if input.starts_with('}') {
      if first { // Cannot have '}' as the first element after '{' unless empty
        obj.decr();
        return Err(ParseError::ExpectedCharacter('"')); // Expecting a key string
      } else { // Trailing comma case - standard JSON forbids this
        obj.decr();
        return Err(ParseError::ExpectedCharacter('"')); // Expecting key after comma
      }
    }

    // Parse key (must be a string)
    skip_whitespace(input);
    // Check if it starts with quote, using consume_char for better error reporting
    consume_char(input, '"')?; // Consume opening quote
    let key = parse_string_content(input)?; // Consumes content and closing quote

    // Parse colon separator
    skip_whitespace(input);
    // Use consume_char which now returns UnexpectedCharacter if colon is not found
    consume_char(input, ':')?;
    skip_whitespace(input);

    // Parse value
    let (val, _) = parse_value(input)?; // parse_value updates the input slice

    // Set property in object
    // Clone val for insertion; parse_value returns an owned Data
    // If val is Object/Array, its ref count was incremented by parse_value
    obj.set_property(&key, val.clone()); // Requires `obj` to be mutable

    // IMPORTANT: Decrement ref count of the original `val` returned by parse_value
    // as it's now owned/referenced by the `obj`.
    // This matches the original code's `decr` pattern after insertion.
    if val.is_object() { val.object().decr(); }
    if val.is_array() { val.array().decr(); }

    // Check for end of object or next comma
    skip_whitespace(input);
    if input.starts_with('}') {
      consume_char(input, '}')?;
      first = false; // Mark that we've processed at least one element or it was empty
      break; // Successfully parsed object
    } else if input.starts_with(',') {
      first = false; // Ready for next key-value pair
      // continue loop handled by loop structure
    }
    else {
      // Found something other than '}' or ',' after a value
      obj.decr(); // Clean up created object
      let found = input.chars().next();
      match found {
        Some(c) => return Err(ParseError::UnexpectedCharacter(c)), // More specific
        None => return Err(ParseError::UnexpectedEof),
      }
      // return Err(ParseError::ExpectedComma); // Or ExpectedCharacter('}') - less specific
    }
  } // End loop

  // No need for `if first` check here, loop logic covers valid exits

  Ok((obj, *input))
}

// Parse a JSON array: [ value, ... ]
// Returns the parsed DataArray and the remaining slice
#[allow(unused_assignments)]
fn parse_array<'a>(input: &mut &'a str) -> Result<(DataArray, &'a str), ParseError> {
  consume_char(input, '[')?;
  skip_whitespace(input);

  let mut arr = DataArray::new(); // Create the array

  let mut first = true;

  // Check for empty array
  if input.starts_with(']') {
    consume_char(input, ']')?;
    return Ok((arr, *input));
  }

  loop {
    if !first {
      // Expect a comma
      skip_whitespace(input);
      // Check for closing bracket before consuming comma
      if input.starts_with(']') {
        arr.decr(); // Clean up
        return Err(ParseError::ExpectedComma); // Comma was expected before ]
      }
      consume_char(input, ',')?;
      skip_whitespace(input);
    }


    // Check for closing bracket after comma (or for first element)
    if input.starts_with(']') {
      if first { // Cannot have ']' as the first element after '[' unless empty
        arr.decr();
        return Err(ParseError::ExpectedValue); // Expecting a value
      } else { // Trailing comma case - standard JSON forbids this
        arr.decr();
        return Err(ParseError::ExpectedValue); // Expecting value after comma
      }
    }


    // Parse value
    skip_whitespace(input); // Needed if value follows comma immediately
    let (val, _) = parse_value(input)?; // parse_value updates the input slice

    // Push property to array
    arr.push_property(val.clone()); // Requires `arr` to be mutable

    // IMPORTANT: Decrement ref count of the original `val` returned by parse_value
    // This matches the original code's `decr` pattern after insertion.
    if val.is_object() { val.object().decr(); }
    if val.is_array() { val.array().decr(); }

    // Check for end of array or next comma
    skip_whitespace(input);
    if input.starts_with(']') {
      consume_char(input, ']')?;
      first = false; // Mark that we've processed at least one element or it was empty
      break; // Successfully parsed array
    } else if input.starts_with(',') {
      first = false; // Ready for next element
      // continue loop handled by loop structure
    }
    else {
      // Found something other than ']' or ',' after a value
      arr.decr(); // Clean up created array
      let found = input.chars().next();
      match found {
        Some(c) => return Err(ParseError::UnexpectedCharacter(c)), // More specific
        None => return Err(ParseError::UnexpectedEof),
      }
      // return Err(ParseError::ExpectedComma); // Or ExpectedCharacter(']') - less specific
    }
  } // End loop

  // No need for `if first` check here, loop logic covers valid exits

  Ok((arr, *input))
}

// --- Original Escape/Unescape (Kept for reference/compatibility if needed) ---
// --- Note: The new implementation (write_escaped_str/parse_string_content) is preferred ---

/// Unescape the string (Original version - potentially incomplete/buggy)
/// Note: `unescape` provides improved unescaping during parsing.
pub fn unescape_original(s:&str) -> String {
  // FIXME - Known issues with double-escaped strings (from original comment)
  // FIXME - Doesn't handle \uXXXX, \b, \f, \/
  let s = str::replace(&s, "\\\"", "\"");
  // let s = str::replace(&s, "\\b", "\b"); // Original commented out
  // let s = str::replace(&s, "\\f", "\f"); // Original commented out
  let s = str::replace(&s, "\\n", "\n");
  let s = str::replace(&s, "\\r", "\r");
  let s = str::replace(&s, "\\t", "\t");
  let s = str::replace(&s, "\\\\", "\\");
  s
}

/// Escape the string (Original version - potentially incomplete/buggy)
/// Note: `write_escaped_str` provides improved escaping during serialization.
pub fn escape_original(s:&str) -> String {
  // FIXME - Known issues with double-escaped strings (from original comment)
  // FIXME - Doesn't handle \b, \f, \/, control chars
  let s = str::replace(&s, "\\", "\\\\");
  let s = str::replace(&s, "\"", "\\\"");
  // let s = str::replace(&s, "\b", "\\b"); // Original commented out
  // let s = str::replace(&s, "\f", "\\f"); // Original commented out
  let s = str::replace(&s, "\n", "\\n");
  let s = str::replace(&s, "\r", "\r");
  let s = str::replace(&s, "\t", "\\t");
  s
}


// --- Potentially needed `str::replace` if std is not available ---
// If truly `no_std` without `alloc`, string manipulation becomes much harder.
// Assuming `alloc` is available based on `extern crate alloc`.
#[cfg(feature="no_std_support")]
mod str {
  use alloc::string::String;
  use alloc::vec::Vec; // Needed for join
  // Basic replace functionality if std::str::replace isn't available
  // This is very basic and less efficient than std's version.
  pub fn replace(s: &str, from: &str, to: &str) -> String {
    s.split(from).collect::<Vec<&str>>().join(to)
    // Note: Requires Vec and join, relies on alloc
  }
}
#[cfg(not(feature="no_std_support"))]
use std::str; // Use standard library's str module if available
