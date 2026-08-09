//! JSON serialization and deserialization for [`DataObject`] and [`DataArray`]
//! without external dependencies (used when the `serde_support` feature is off).
//!
//! # Design notes
//!
//! Both the parser and the serializer are **iterative** (they maintain an
//! explicit stack instead of recursing), so arbitrarily deep input or data can
//! not overflow the call stack.
//!
//! ## Reference counting
//!
//! This module never calls `incr()`/`decr()` directly. The underlying structs
//! already manage reference counts:
//!
//! * `DataObject::new()`/`DataArray::new()` allocate with a count of 1.
//! * `set_property()`/`push_property()` increment the count of any
//!   `DObject`/`DArray`/`DBytes` value being inserted.
//! * Dropping a `DataObject`/`DataArray` handle queues a decrement that is
//!   applied on the next `gc()` call, which also recursively releases children.
//!
//! So when a parsed child container is attached to its parent, the parent's
//! insert adds one count and the child handle's `Drop` queues the release of
//! the count it was born with — settling at exactly the one count held by the
//! parent. On a parse error, every partially built container handle is simply
//! dropped; the next `gc()` reclaims the whole partial tree. (Calling
//! `Heap::decr` directly here instead would be wrong twice over: it frees the
//! slot without releasing the container's children, and it leaves the handle's
//! queued decrement pointing at a slot that may have been reused.)
//!
//! ## Strictness
//!
//! Parsing follows RFC 8259 strictly: only space, tab, CR and LF are
//! whitespace; numbers may not have leading zeros or bare trailing dots;
//! strings may not contain unescaped control characters; trailing commas and
//! trailing input are errors.
//!
//! ## Serialization edge cases
//!
//! * Object keys are written in sorted order, matching the `serde_support`
//!   code path (whose `serde_json::Map` is a `BTreeMap`).
//! * Non-finite floats (`NaN`, `±inf`) are written as `null`, matching
//!   `serde_json`.
//! * Finite floats always round-trip as floats: a value like `1.0` is written
//!   as `1.0`, not `1`.
//! * A reference cycle (a container that contains itself, directly or
//!   indirectly) is written as `null` at the point of the cycle instead of
//!   recursing forever.
//! * A dangling `DObject`/`DArray`/`DBytes` reference is written as `null`,
//!   matching the serde path's `to_json()` behavior for invalid refs.
//! * `DBytes` values are written as their hex string (see
//!   `DataBytes::to_hex_string`); they are *not* restored as bytes when the
//!   text is parsed back — they come back as a `DString`.

extern crate alloc;

use crate::data::Data;
use crate::dataarray::{aheap, DataArray};
use crate::databytes::{bheap, DataBytes};
use crate::dataobject::{oheap, DataObject};

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use core::fmt::Write as _;

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
  Message(String),
}

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

#[cfg(not(feature = "no_std_support"))]
impl std::error::Error for ParseError {}

// --- Serialization ---

/// One in-progress container during iterative serialization. The frames on the
/// stack are also the ancestor path used for cycle detection.
enum WriteFrame {
  Object {
    data_ref: usize,
    entries: Vec<(String, Data)>,
    index: usize,
  },
  Array {
    data_ref: usize,
    items: Vec<Data>,
    index: usize,
  },
}

/// Create a JSON string from a DataObject.
///
/// Never panics and always terminates: see the module docs for how cycles,
/// dangling references and non-finite floats are rendered. If the handle's own
/// ref is invalid (already freed), returns `"{}"`.
pub fn object_to_string(o: DataObject) -> String {
  match snapshot_object(o.data_ref) {
    Some(entries) => {
      let mut out = String::new();
      out.push('{');
      let mut stack = Vec::new();
      stack.push(WriteFrame::Object { data_ref: o.data_ref, entries, index: 0 });
      drive_writer(&mut out, &mut stack);
      out
    }
    None => "{}".to_string(),
  }
}

/// Create a JSON string from a DataArray.
///
/// Same guarantees as [`object_to_string`]; an invalid ref returns `"[]"`.
pub fn array_to_string(a: DataArray) -> String {
  match snapshot_array(a.data_ref) {
    Some(items) => {
      let mut out = String::new();
      out.push('[');
      let mut stack = Vec::new();
      stack.push(WriteFrame::Array { data_ref: a.data_ref, items, index: 0 });
      drive_writer(&mut out, &mut stack);
      out
    }
    None => "[]".to_string(),
  }
}

/// Copy an object's entries out of the heap under a single lock acquisition,
/// sorted by key for deterministic output (matching the serde path, whose
/// `serde_json::Map` is a `BTreeMap`). Returns `None` for an invalid ref.
fn snapshot_object(data_ref: usize) -> Option<Vec<(String, Data)>> {
  let heap = &mut oheap().lock();
  let map = heap.try_get(data_ref)?;
  let mut entries: Vec<(String, Data)> =
    map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
  entries.sort_by(|a, b| a.0.cmp(&b.0));
  Some(entries)
}

/// Copy an array's items out of the heap under a single lock acquisition.
/// Returns `None` for an invalid ref.
fn snapshot_array(data_ref: usize) -> Option<Vec<Data>> {
  let heap = &mut aheap().lock();
  Some(heap.try_get(data_ref)?.clone())
}

/// Render a DBytes ref as its hex string, or `None` for an invalid ref.
fn snapshot_bytes(data_ref: usize) -> Option<String> {
  // The byte heap's element type has private fields, so the bytes must be read
  // through a DataBytes handle. The handle API locks internally, so the
  // validity check must release its guard first.
  {
    let heap = &mut bheap().lock();
    if !heap.contains_key(data_ref) {
      return None;
    }
  }
  // DataBytes::get adds a count; dropping the handle queues the matching
  // decrement for the next gc().
  Some(DataBytes::get(data_ref).to_hex_string())
}

/// Pump the writer stack until it is empty. Each iteration emits one scalar,
/// opens one nested container, or closes the current one.
fn drive_writer(out: &mut String, stack: &mut Vec<WriteFrame>) {
  enum Step {
    ObjectEntry(usize, String, Data),
    ArrayItem(usize, Data),
    Close(char),
  }
  loop {
    let step = match stack.last_mut() {
      None => break,
      Some(WriteFrame::Object { entries, index, .. }) => {
        if *index < entries.len() {
          let i = *index;
          *index += 1;
          let (key, value) = core::mem::take(&mut entries[i]);
          Step::ObjectEntry(i, key, value)
        } else {
          Step::Close('}')
        }
      }
      Some(WriteFrame::Array { items, index, .. }) => {
        if *index < items.len() {
          let i = *index;
          *index += 1;
          Step::ArrayItem(i, core::mem::take(&mut items[i]))
        } else {
          Step::Close(']')
        }
      }
    };
    match step {
      Step::ObjectEntry(i, key, value) => {
        if i > 0 {
          out.push(',');
        }
        out.push('"');
        write_escaped_str(out, &key);
        out.push_str("\":");
        write_value(out, value, stack);
      }
      Step::ArrayItem(i, value) => {
        if i > 0 {
          out.push(',');
        }
        write_value(out, value, stack);
      }
      Step::Close(c) => {
        out.push(c);
        stack.pop();
      }
    }
  }
}

/// Write one value. Scalars are emitted directly; containers push a new frame
/// (or emit `null` if they would revisit an ancestor or their ref is invalid).
fn write_value(out: &mut String, data: Data, stack: &mut Vec<WriteFrame>) {
  match data {
    Data::DNull => out.push_str("null"),
    Data::DBoolean(b) => out.push_str(if b { "true" } else { "false" }),
    Data::DInt(i) => {
      let _ = write!(out, "{}", i);
    }
    Data::DFloat(f) => write_float(out, f),
    Data::DString(s) => {
      out.push('"');
      write_escaped_str(out, &s);
      out.push('"');
    }
    Data::DBytes(r) => match snapshot_bytes(r) {
      Some(hex) => {
        out.push('"');
        write_escaped_str(out, &hex);
        out.push('"');
      }
      None => out.push_str("null"),
    },
    Data::DObject(r) => {
      let is_ancestor = stack
        .iter()
        .any(|f| matches!(f, WriteFrame::Object { data_ref, .. } if *data_ref == r));
      if is_ancestor {
        out.push_str("null");
      } else {
        match snapshot_object(r) {
          Some(entries) => {
            out.push('{');
            stack.push(WriteFrame::Object { data_ref: r, entries, index: 0 });
          }
          None => out.push_str("null"),
        }
      }
    }
    Data::DArray(r) => {
      let is_ancestor = stack
        .iter()
        .any(|f| matches!(f, WriteFrame::Array { data_ref, .. } if *data_ref == r));
      if is_ancestor {
        out.push_str("null");
      } else {
        match snapshot_array(r) {
          Some(items) => {
            out.push('[');
            stack.push(WriteFrame::Array { data_ref: r, items, index: 0 });
          }
          None => out.push_str("null"),
        }
      }
    }
  }
}

/// Non-finite floats have no JSON representation; emit `null` like serde_json.
/// Finite floats that would print as integers get a `.0` suffix so they parse
/// back as floats.
fn write_float(out: &mut String, f: f64) {
  if !f.is_finite() {
    out.push_str("null");
    return;
  }
  let start = out.len();
  let _ = write!(out, "{}", f);
  let printed_as_integer = !out.as_bytes()[start..]
    .iter()
    .any(|b| matches!(b, b'.' | b'e' | b'E'));
  if printed_as_integer {
    out.push_str(".0");
  }
}

/// Append `s` to `out` with JSON escaping applied.
fn write_escaped_str(out: &mut String, s: &str) {
  for c in s.chars() {
    match c {
      '"' => out.push_str("\\\""),
      '\\' => out.push_str("\\\\"),
      '\u{08}' => out.push_str("\\b"),
      '\u{0c}' => out.push_str("\\f"),
      '\n' => out.push_str("\\n"),
      '\r' => out.push_str("\\r"),
      '\t' => out.push_str("\\t"),
      c if (c as u32) < 0x20 => {
        let _ = write!(out, "\\u{:04x}", c as u32);
      }
      c => out.push(c),
    }
  }
}

/// JSON-escape a string (the inverse of [`unescape`]).
pub fn escape(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  write_escaped_str(&mut out, s);
  out
}

// --- Deserialization ---

/// Create a new DataObject from a JSON string. Returns `ParseError` on failure.
///
/// The returned handle owns one reference count, exactly as if it came from
/// `DataObject::new()`. On failure nothing needs to be cleaned up by the
/// caller; any partially built data is reclaimed on the next `gc()`.
pub fn object_from_string(s: &str) -> Result<DataObject, ParseError> {
  let mut pos = 0usize;
  let root = parse_root(s, &mut pos, true)?;
  check_trailing(s, &mut pos)?;
  match root {
    ParseFrame::Object { obj, .. } => Ok(obj),
    // parse_root with root_is_object=true can only produce an object frame.
    ParseFrame::Array { .. } => Err(ParseError::Message("internal: root type mismatch".to_string())),
  }
}

/// Create a new DataArray from a JSON string. Returns `ParseError` on failure.
///
/// Same ownership contract as [`object_from_string`].
pub fn array_from_string(s: &str) -> Result<DataArray, ParseError> {
  let mut pos = 0usize;
  let root = parse_root(s, &mut pos, false)?;
  check_trailing(s, &mut pos)?;
  match root {
    ParseFrame::Array { arr } => Ok(arr),
    ParseFrame::Object { .. } => Err(ParseError::Message("internal: root type mismatch".to_string())),
  }
}

fn check_trailing(s: &str, pos: &mut usize) -> Result<(), ParseError> {
  skip_whitespace(s, pos);
  if *pos < s.len() {
    // The caller's root handle is dropped on this path; gc() reclaims it.
    Err(ParseError::TrailingCharacters(s[*pos..].trim().to_string()))
  } else {
    Ok(())
  }
}

/// One in-progress container during iterative parsing. Holding real handles
/// here means a parse error cleans itself up: unwinding drops every frame,
/// each Drop queues its container for the next gc(), and gc() releases
/// children recursively.
enum ParseFrame {
  Object {
    obj: DataObject,
    pending_key: Option<String>,
  },
  Array {
    arr: DataArray,
  },
}

#[derive(Clone, Copy, PartialEq)]
enum ParserState {
  /// Just after `{`: a key or `}` may follow.
  ObjectKeyOrClose,
  /// After a comma in an object: a key must follow (no trailing comma).
  ObjectKeyAfterComma,
  /// After a key: `:` must follow.
  ObjectColon,
  /// After `:`: a value must follow.
  ObjectValue,
  /// Just after `[`: a value or `]` may follow.
  ArrayValueOrClose,
  /// After a comma in an array: a value must follow (no trailing comma).
  ArrayValueAfterComma,
  /// After a completed value: `,` or the container's closer must follow.
  AfterValue,
}

/// Parse one complete JSON container (object or array) starting at `*pos`,
/// leaving `*pos` just past its closing brace/bracket.
fn parse_root(s: &str, pos: &mut usize, root_is_object: bool) -> Result<ParseFrame, ParseError> {
  skip_whitespace(s, pos);
  let mut stack: Vec<ParseFrame> = Vec::new();
  let mut state;
  if root_is_object {
    expect_byte(s, pos, b'{')?;
    stack.push(ParseFrame::Object { obj: DataObject::new(), pending_key: None });
    state = ParserState::ObjectKeyOrClose;
  } else {
    expect_byte(s, pos, b'[')?;
    stack.push(ParseFrame::Array { arr: DataArray::new() });
    state = ParserState::ArrayValueOrClose;
  }

  loop {
    skip_whitespace(s, pos);
    match state {
      ParserState::ObjectKeyOrClose | ParserState::ObjectKeyAfterComma => {
        match s.as_bytes().get(*pos) {
          Some(&b'}') => {
            if state == ParserState::ObjectKeyAfterComma {
              // Trailing comma: a key was promised.
              return Err(ParseError::ExpectedCharacter('"'));
            }
            *pos += 1;
            match close_container(&mut stack) {
              Some(root) => return Ok(root),
              None => state = ParserState::AfterValue,
            }
          }
          Some(&b'"') => {
            *pos += 1;
            let key = parse_string_body(s, pos, true)?;
            match stack.last_mut() {
              Some(ParseFrame::Object { pending_key, .. }) => *pending_key = Some(key),
              _ => return Err(ParseError::Message("internal: key outside object".to_string())),
            }
            state = ParserState::ObjectColon;
          }
          Some(_) => return Err(ParseError::UnexpectedCharacter(char_at(s, *pos))),
          None => return Err(ParseError::UnexpectedEof),
        }
      }
      ParserState::ObjectColon => {
        expect_byte(s, pos, b':')?;
        state = ParserState::ObjectValue;
      }
      ParserState::ObjectValue => {
        state = parse_value(s, pos, &mut stack)?;
      }
      ParserState::ArrayValueOrClose | ParserState::ArrayValueAfterComma => {
        if s.as_bytes().get(*pos) == Some(&b']') {
          if state == ParserState::ArrayValueAfterComma {
            // Trailing comma: a value was promised.
            return Err(ParseError::ExpectedValue);
          }
          *pos += 1;
          match close_container(&mut stack) {
            Some(root) => return Ok(root),
            None => state = ParserState::AfterValue,
          }
        } else {
          state = parse_value(s, pos, &mut stack)?;
        }
      }
      ParserState::AfterValue => {
        let closer = match stack.last() {
          Some(ParseFrame::Object { .. }) => b'}',
          Some(ParseFrame::Array { .. }) => b']',
          None => return Err(ParseError::Message("internal: empty parse stack".to_string())),
        };
        match s.as_bytes().get(*pos) {
          Some(&b) if b == closer => {
            *pos += 1;
            match close_container(&mut stack) {
              Some(root) => return Ok(root),
              None => state = ParserState::AfterValue,
            }
          }
          Some(&b',') => {
            *pos += 1;
            state = if closer == b'}' {
              ParserState::ObjectKeyAfterComma
            } else {
              ParserState::ArrayValueAfterComma
            };
          }
          Some(_) => return Err(ParseError::UnexpectedCharacter(char_at(s, *pos))),
          None => return Err(ParseError::UnexpectedEof),
        }
      }
    }
  }
}

/// Pop the finished container. If it was the root, hand it back; otherwise
/// attach it to the parent. The insert increments the child's count and the
/// popped handle's Drop queues the release of the count it was born with.
fn close_container(stack: &mut Vec<ParseFrame>) -> Option<ParseFrame> {
  let child = stack.pop()?;
  if stack.is_empty() {
    return Some(child);
  }
  let data = match &child {
    ParseFrame::Object { obj, .. } => Data::DObject(obj.data_ref),
    ParseFrame::Array { arr } => Data::DArray(arr.data_ref),
  };
  attach_value(stack, data);
  // `child` handle drops here, queueing its decrement for the next gc().
  None
}

/// Store a completed value into the container on top of the stack.
fn attach_value(stack: &mut [ParseFrame], data: Data) {
  match stack.last_mut() {
    Some(ParseFrame::Object { obj, pending_key }) => {
      if let Some(key) = pending_key.take() {
        obj.set_property(&key, data);
      }
    }
    Some(ParseFrame::Array { arr }) => arr.push_property(data),
    None => {}
  }
}

/// Parse one value at `*pos`. Scalars are attached immediately; `{`/`[` push a
/// new frame. Returns the parser state to continue in.
fn parse_value(
  s: &str,
  pos: &mut usize,
  stack: &mut Vec<ParseFrame>,
) -> Result<ParserState, ParseError> {
  match s.as_bytes().get(*pos) {
    None => Err(ParseError::UnexpectedEof),
    Some(&b'"') => {
      *pos += 1;
      let text = parse_string_body(s, pos, true)?;
      attach_value(stack, Data::DString(text));
      Ok(ParserState::AfterValue)
    }
    Some(&b'{') => {
      *pos += 1;
      stack.push(ParseFrame::Object { obj: DataObject::new(), pending_key: None });
      Ok(ParserState::ObjectKeyOrClose)
    }
    Some(&b'[') => {
      *pos += 1;
      stack.push(ParseFrame::Array { arr: DataArray::new() });
      Ok(ParserState::ArrayValueOrClose)
    }
    Some(&b't') => {
      if s[*pos..].starts_with("true") {
        *pos += 4;
        attach_value(stack, Data::DBoolean(true));
        Ok(ParserState::AfterValue)
      } else {
        Err(ParseError::UnexpectedCharacter('t'))
      }
    }
    Some(&b'f') => {
      if s[*pos..].starts_with("false") {
        *pos += 5;
        attach_value(stack, Data::DBoolean(false));
        Ok(ParserState::AfterValue)
      } else {
        Err(ParseError::UnexpectedCharacter('f'))
      }
    }
    Some(&b'n') => {
      if s[*pos..].starts_with("null") {
        *pos += 4;
        attach_value(stack, Data::DNull);
        Ok(ParserState::AfterValue)
      } else {
        Err(ParseError::UnexpectedCharacter('n'))
      }
    }
    Some(&b) if b == b'-' || b.is_ascii_digit() => {
      let num = parse_number(s, pos)?;
      attach_value(stack, num);
      Ok(ParserState::AfterValue)
    }
    Some(_) => Err(ParseError::UnexpectedCharacter(char_at(s, *pos))),
  }
}

/// Parse a number using the strict JSON grammar:
/// `-? (0 | [1-9][0-9]*) ('.' [0-9]+)? ([eE] [+-]? [0-9]+)?`
///
/// A fraction or exponent that doesn't fully match (e.g. `1.` or `1e`) simply
/// ends the number before it, and the surrounding context then rejects the
/// leftover character. Integers that overflow i64 fall back to f64; a value
/// whose magnitude overflows f64 (e.g. `1e999`) is rejected rather than
/// silently becoming infinity.
fn parse_number(s: &str, pos: &mut usize) -> Result<Data, ParseError> {
  let bytes = s.as_bytes();
  let start = *pos;
  let mut i = *pos;

  if bytes.get(i) == Some(&b'-') {
    i += 1;
  }
  match bytes.get(i) {
    Some(&b'0') => i += 1,
    Some(b) if b.is_ascii_digit() => {
      while bytes.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
      }
    }
    _ => return Err(ParseError::InvalidNumber(s[start..i].to_string())),
  }

  let mut is_float = false;
  if bytes.get(i) == Some(&b'.') && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
    is_float = true;
    i += 2;
    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
      i += 1;
    }
  }
  if matches!(bytes.get(i), Some(&b'e') | Some(&b'E')) {
    let mut j = i + 1;
    if matches!(bytes.get(j), Some(&b'+') | Some(&b'-')) {
      j += 1;
    }
    if bytes.get(j).is_some_and(u8::is_ascii_digit) {
      is_float = true;
      i = j + 1;
      while bytes.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
      }
    }
  }

  let text = &s[start..i];
  *pos = i;
  if !is_float {
    if let Ok(v) = text.parse::<i64>() {
      return Ok(Data::DInt(v));
    }
  }
  match text.parse::<f64>() {
    Ok(f) if f.is_finite() => Ok(Data::DFloat(f)),
    _ => Err(ParseError::InvalidNumber(text.to_string())),
  }
}

// --- Low-level scanning helpers ---

/// Consume JSON whitespace only (space, tab, CR, LF) — not the full Unicode
/// whitespace set that `str::trim` uses.
fn skip_whitespace(s: &str, pos: &mut usize) {
  let bytes = s.as_bytes();
  while matches!(bytes.get(*pos), Some(&b' ') | Some(&b'\t') | Some(&b'\n') | Some(&b'\r')) {
    *pos += 1;
  }
}

/// The char starting at byte `pos`, which the scanners keep on a char
/// boundary (they only ever advance past complete characters).
fn char_at(s: &str, pos: usize) -> char {
  s[pos..].chars().next().unwrap_or('\u{fffd}')
}

/// Consume `expected` (an ASCII byte) or report what was found instead.
fn expect_byte(s: &str, pos: &mut usize, expected: u8) -> Result<(), ParseError> {
  match s.as_bytes().get(*pos) {
    Some(&b) if b == expected => {
      *pos += 1;
      Ok(())
    }
    Some(_) => Err(ParseError::UnexpectedCharacter(char_at(s, *pos))),
    None => Err(ParseError::UnexpectedEof),
  }
}

/// Parse exactly four ASCII hex digits at `*pos`. Byte-wise validation keeps
/// this panic-free even when multibyte characters follow a truncated escape.
fn parse_hex4(s: &str, pos: &mut usize) -> Result<u32, ParseError> {
  let bytes = s.as_bytes();
  let start = *pos;
  for offset in 0..4 {
    match bytes.get(start + offset) {
      Some(b) if b.is_ascii_hexdigit() => {}
      Some(_) => {
        // The verified prefix is pure ASCII, so start+offset is a char boundary.
        return Err(ParseError::InvalidUnicodeEscape(format!(
          "\\u{}<-- invalid char '{}'",
          &s[start..start + offset],
          char_at(s, start + offset)
        )));
      }
      None => {
        return Err(ParseError::InvalidUnicodeEscape(format!(
          "\\u{} (unexpected EOF)",
          &s[start..]
        )));
      }
    }
  }
  let code = u32::from_str_radix(&s[start..start + 4], 16)
    .map_err(|_| ParseError::InvalidUnicodeEscape(format!("\\u{} (internal parsing failed)", &s[start..start + 4])))?;
  *pos = start + 4;
  Ok(code)
}

/// Shared unescaping engine for JSON string content.
///
/// With `quoted == true` (the parser), scanning ends at an unescaped `"`
/// (consumed) and hitting end-of-input is an error. With `quoted == false`
/// (the public [`unescape`] helper), an unescaped `"` is ordinary content and
/// scanning ends at end-of-input.
///
/// Handles all standard escapes including `\uXXXX` and UTF-16 surrogate
/// pairs. Rejects unescaped control characters (U+0000..U+001F) per RFC 8259.
fn parse_string_body(s: &str, pos: &mut usize, quoted: bool) -> Result<String, ParseError> {
  let bytes = s.as_bytes();
  let mut out = String::new();
  let mut seg_start = *pos;
  let mut i = *pos;

  loop {
    match bytes.get(i) {
      None => {
        out.push_str(&s[seg_start..]);
        *pos = s.len();
        return if quoted { Err(ParseError::UnexpectedEof) } else { Ok(out) };
      }
      Some(&b'"') if quoted => {
        out.push_str(&s[seg_start..i]);
        *pos = i + 1;
        return Ok(out);
      }
      Some(&b'\\') => {
        out.push_str(&s[seg_start..i]);
        i += 1;
        match bytes.get(i) {
          None => return Err(ParseError::UnexpectedEof),
          Some(&b'"') => {
            out.push('"');
            i += 1;
          }
          Some(&b'\\') => {
            out.push('\\');
            i += 1;
          }
          Some(&b'/') => {
            out.push('/');
            i += 1;
          }
          Some(&b'b') => {
            out.push('\u{08}');
            i += 1;
          }
          Some(&b'f') => {
            out.push('\u{0c}');
            i += 1;
          }
          Some(&b'n') => {
            out.push('\n');
            i += 1;
          }
          Some(&b'r') => {
            out.push('\r');
            i += 1;
          }
          Some(&b't') => {
            out.push('\t');
            i += 1;
          }
          Some(&b'u') => {
            i += 1;
            let code1 = parse_hex4(s, &mut i)?;
            if (0xD800..=0xDBFF).contains(&code1) {
              // High surrogate: a low surrogate escape must follow.
              if bytes.get(i) == Some(&b'\\') {
                if bytes.get(i + 1) == Some(&b'u') {
                  i += 2;
                  let code2 = parse_hex4(s, &mut i)?;
                  if (0xDC00..=0xDFFF).contains(&code2) {
                    let combined = 0x10000 + ((code1 - 0xD800) << 10) + (code2 - 0xDC00);
                    match core::char::from_u32(combined) {
                      Some(c) => out.push(c),
                      None => {
                        return Err(ParseError::InvalidUnicodeEscape(format!(
                          "\\u{:04X}\\u{:04X} (combined to invalid code point {})",
                          code1, code2, combined
                        )))
                      }
                    }
                  } else {
                    return Err(ParseError::InvalidUnicodeEscape(format!(
                      "\\u{:04X} followed by non-low surrogate \\u{:04X}",
                      code1, code2
                    )));
                  }
                } else {
                  return Err(ParseError::InvalidUnicodeEscape(format!(
                    "\\u{:04X} followed by invalid escape sequence",
                    code1
                  )));
                }
              } else {
                return Err(ParseError::InvalidUnicodeEscape(format!(
                  "Lone high surrogate \\u{:04X}",
                  code1
                )));
              }
            } else {
              match core::char::from_u32(code1) {
                Some(c) => out.push(c),
                None => {
                  return Err(ParseError::InvalidUnicodeEscape(format!(
                    "\\u{:04X} (invalid code point)",
                    code1
                  )))
                }
              }
            }
          }
          Some(_) => {
            return Err(ParseError::InvalidEscapeSequence(format!("\\{}", char_at(s, i))));
          }
        }
        seg_start = i;
      }
      Some(&b) if b < 0x20 => {
        return Err(ParseError::UnexpectedCharacter(b as char));
      }
      Some(_) => {
        // Any other byte (including UTF-8 continuation bytes) is plain
        // content; it is copied wholesale when the segment is flushed.
        i += 1;
      }
    }
  }
}

/// Unescapes a string slice that represents the *content* of a JSON string
/// (without the surrounding quotes). Handles standard JSON escapes like \n,
/// \t, \\, \", \uXXXX etc., including UTF-16 surrogate pairs
/// (e.g. \uD83D\uDE00). Unescaped double quotes are treated as ordinary
/// content. Unescaped control characters are rejected.
pub fn unescape(s: &str) -> Result<String, ParseError> {
  let mut pos = 0usize;
  parse_string_body(s, &mut pos, false)
}

// --- Original Escape/Unescape (kept for API compatibility) ---

/// Unescape the string (original version).
///
/// Handles only `\"`, `\n`, `\r`, `\t` and `\\` — not `\uXXXX`, `\b`, `\f`
/// or `\/` — and cannot report errors.
#[deprecated(since = "0.3.17", note = "please use `unescape` instead")]
pub fn unescape_original(s: &str) -> String {
  let s = s.replace("\\\"", "\"");
  let s = s.replace("\\n", "\n");
  let s = s.replace("\\r", "\r");
  let s = s.replace("\\t", "\t");
  s.replace("\\\\", "\\")
}

/// Escape the string (original version).
///
/// Handles only `\`, `"`, newline, carriage return and tab — not `\b`, `\f`
/// or other control characters.
#[deprecated(since = "0.3.17", note = "please use `escape` instead")]
pub fn escape_original(s: &str) -> String {
  let s = s.replace('\\', "\\\\");
  let s = s.replace('"', "\\\"");
  let s = s.replace('\n', "\\n");
  let s = s.replace('\r', "\\r");
  s.replace('\t', "\\t")
}

// --- Tests for the robustness fixes (the original suite lives in json_util_tests.rs) ---

#[cfg(test)]
mod robustness_tests {
  use super::*;

  // NOTE: none of these tests call gc(); tests run in parallel against the
  // shared global heaps, and collecting here could also collect handles that
  // other tests still reason about.

  /// crate::init() has a check-then-set race when called from parallel test
  /// threads (it panics if another thread's set() wins in between). Funnel
  /// this module's tests through one attempt and tolerate losing that race
  /// to json_util_tests.rs, whose tests call crate::init() directly.
  fn test_init() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
      for _ in 0..100 {
        if std::panic::catch_unwind(|| {
          crate::init();
        })
        .is_ok()
        {
          return;
        }
        std::thread::yield_now();
      }
    });
  }

  fn parse_obj(s: &str) -> DataObject {
    object_from_string(s).expect("parse should succeed")
  }

  // --- Numbers ---

  #[test]
  fn parses_negative_integers() {
    test_init();
    let obj = parse_obj(r#"{"n":-5}"#);
    assert_eq!(obj.get_property("n"), Data::DInt(-5));
  }

  #[test]
  fn parses_negative_floats() {
    test_init();
    let obj = parse_obj(r#"{"n":-5.25}"#);
    assert_eq!(obj.get_property("n"), Data::DFloat(-5.25));
  }

  #[test]
  fn parses_signed_exponents() {
    test_init();
    let obj = parse_obj(r#"{"a":1e+5,"b":25E-2,"c":2e3}"#);
    assert_eq!(obj.get_property("a"), Data::DFloat(1e5));
    assert_eq!(obj.get_property("b"), Data::DFloat(0.25));
    assert_eq!(obj.get_property("c"), Data::DFloat(2000.0));
  }

  #[test]
  fn integer_overflow_falls_back_to_float() {
    test_init();
    let obj = parse_obj(r#"{"n":99999999999999999999999}"#);
    assert!(obj.get_property("n").is_float());
  }

  #[test]
  fn rejects_leading_zeros() {
    test_init();
    assert!(object_from_string(r#"{"n":0123}"#).is_err());
  }

  #[test]
  fn rejects_bare_trailing_dot_and_exponent() {
    test_init();
    assert!(object_from_string(r#"{"n":1.}"#).is_err());
    assert!(object_from_string(r#"{"n":1e}"#).is_err());
    assert!(object_from_string(r#"{"n":.5}"#).is_err());
    assert!(object_from_string(r#"{"n":-}"#).is_err());
    assert!(object_from_string(r#"{"n":+1}"#).is_err());
  }

  #[test]
  fn rejects_float_overflow() {
    test_init();
    let err = object_from_string(r#"{"n":1e999}"#).err().unwrap();
    assert!(matches!(err, ParseError::InvalidNumber(_)));
  }

  #[test]
  fn negative_zero_parses() {
    test_init();
    let obj = parse_obj(r#"{"n":-0}"#);
    assert_eq!(obj.get_property("n"), Data::DInt(0));
  }

  // --- Strings ---

  #[test]
  fn rejects_unterminated_string() {
    test_init();
    let err = object_from_string(r#"{"a":"bc"#).err().unwrap();
    assert_eq!(err, ParseError::UnexpectedEof);
  }

  #[test]
  fn rejects_raw_control_char_in_string() {
    test_init();
    assert!(object_from_string("{\"a\":\"b\nc\"}").is_err());
  }

  #[test]
  fn truncated_unicode_escape_before_multibyte_does_not_panic() {
    test_init();
    // The old byte-slicing implementation panicked on inputs like this.
    assert!(object_from_string("{\"a\":\"\\u12\u{20AC}\"}").is_err());
    assert!(unescape("\\u12\u{20AC}").is_err());
  }

  #[test]
  fn rejects_signed_hex_in_unicode_escape() {
    test_init();
    // u32::from_str_radix accepts a leading '+'; the escape parser must not.
    assert!(matches!(
      unescape(r"\u+abc").err().unwrap(),
      ParseError::InvalidUnicodeEscape(_)
    ));
  }

  #[test]
  fn rejects_lone_low_surrogate() {
    assert!(matches!(
      unescape(r"\uDC00").err().unwrap(),
      ParseError::InvalidUnicodeEscape(_)
    ));
  }

  #[test]
  fn parses_surrogate_pairs_in_documents() {
    test_init();
    let obj = parse_obj(r#"{"emoji":"\uD83D\uDE00"}"#);
    assert_eq!(obj.get_property("emoji"), Data::DString("😀".to_string()));
  }

  #[test]
  fn escape_round_trips_through_unescape() {
    let original = "line1\nline2\t\"quoted\" \\ \u{08}\u{0c}\u{01} 😀 €";
    assert_eq!(unescape(&escape(original)).unwrap(), original);
  }

  // --- Structure ---

  #[test]
  fn deep_nesting_does_not_overflow_the_stack() {
    test_init();
    let depth = 10_000;
    let mut json = String::new();
    for _ in 0..depth {
      json.push('[');
    }
    for _ in 0..depth {
      json.push(']');
    }
    let arr = array_from_string(&json).expect("deep parse should succeed");
    assert_eq!(array_to_string(arr.clone()), json);
  }

  #[test]
  fn rejects_trailing_garbage_after_nested_close() {
    test_init();
    let err = array_from_string("[1]]").err().unwrap();
    assert!(matches!(err, ParseError::TrailingCharacters(_)));
  }

  #[test]
  fn rejects_non_json_whitespace() {
    test_init();
    // U+00A0 is whitespace to str::trim but not to JSON.
    assert!(object_from_string("\u{00A0}{}").is_err());
  }

  #[test]
  fn parse_error_after_partial_build_does_not_corrupt_heap() {
    test_init();
    // The old error paths called Heap::decr directly, freeing the container's
    // slot while its handle's queued drop still pointed at it. Here we only
    // verify the error surfaces cleanly and later allocations still work.
    for bad in [
      r#"{"a": {"b": 1}, "c": "#,
      r#"[[1, 2], [3, "#,
      r#"{"a": [1, {"b": 2}"#,
      r#"{"a": 1} trailing"#,
    ] {
      assert!(object_from_string(bad).is_err() || array_from_string(bad).is_err());
    }
    let obj = parse_obj(r#"{"still":"works"}"#);
    assert_eq!(obj.get_property("still"), Data::DString("works".to_string()));
  }

  // --- Serialization ---

  #[test]
  fn serializes_keys_in_sorted_order() {
    test_init();
    let obj = parse_obj(r#"{"b":1,"zz":3,"a":2}"#);
    assert_eq!(object_to_string(obj.clone()), r#"{"a":2,"b":1,"zz":3}"#);
  }

  #[test]
  fn floats_round_trip_as_floats() {
    test_init();
    let mut obj = DataObject::new();
    obj.set_property("f", Data::DFloat(1.0));
    let json = object_to_string(obj.clone());
    assert_eq!(json, r#"{"f":1.0}"#);
    let back = parse_obj(&json);
    assert_eq!(back.get_property("f"), Data::DFloat(1.0));
  }

  #[test]
  fn non_finite_floats_serialize_as_null() {
    test_init();
    let mut obj = DataObject::new();
    obj.set_property("nan", Data::DFloat(f64::NAN));
    obj.set_property("inf", Data::DFloat(f64::INFINITY));
    obj.set_property("ninf", Data::DFloat(f64::NEG_INFINITY));
    assert_eq!(object_to_string(obj.clone()), r#"{"inf":null,"nan":null,"ninf":null}"#);
  }

  #[test]
  fn negative_numbers_round_trip() {
    test_init();
    let json = r#"{"a":-5,"b":-2.5}"#;
    let obj = parse_obj(json);
    assert_eq!(object_to_string(obj.clone()), json);
  }

  #[test]
  fn control_characters_round_trip() {
    test_init();
    let mut obj = DataObject::new();
    obj.set_property("c", Data::DString("\u{01}\u{08}\u{0c}\n\r\t".to_string()));
    let json = object_to_string(obj.clone());
    assert_eq!(json, r#"{"c":"\u0001\b\f\n\r\t"}"#);
    let back = parse_obj(&json);
    assert_eq!(back.get_property("c"), obj.get_property("c"));
  }

  #[test]
  fn forward_slash_is_not_escaped_on_output() {
    test_init();
    let mut obj = DataObject::new();
    obj.set_property("url", Data::DString("http://x/y".to_string()));
    assert_eq!(object_to_string(obj.clone()), r#"{"url":"http://x/y"}"#);
  }

  #[test]
  fn self_referencing_object_serializes_cycle_as_null() {
    test_init();
    let mut obj = DataObject::new();
    obj.set_property("me", Data::DObject(obj.data_ref));
    assert_eq!(object_to_string(obj.clone()), r#"{"me":null}"#);
  }

  #[test]
  fn mutual_cycle_terminates() {
    test_init();
    let mut a = DataObject::new();
    let mut b = DataObject::new();
    a.set_property("b", Data::DObject(b.data_ref));
    b.set_property("a", Data::DObject(a.data_ref));
    assert_eq!(object_to_string(a.clone()), r#"{"b":{"a":null}}"#);
  }

  #[test]
  fn shared_child_is_not_treated_as_cycle() {
    test_init();
    let mut shared = DataObject::new();
    shared.set_property("x", Data::DInt(1));
    let mut obj = DataObject::new();
    obj.set_property("a", Data::DObject(shared.data_ref));
    obj.set_property("b", Data::DObject(shared.data_ref));
    assert_eq!(object_to_string(obj.clone()), r#"{"a":{"x":1},"b":{"x":1}}"#);
  }

  #[test]
  fn dangling_reference_serializes_as_null() {
    test_init();
    let obj = DataObject::new();
    // Fabricate a dangling reference directly in the map, bypassing
    // set_property's refcount bookkeeping. usize::MAX can never be a live
    // heap slot, so this stays deterministic with tests running in parallel
    // against the shared heap.
    {
      let heap = &mut oheap().lock();
      heap.get(obj.data_ref).insert("child".to_string(), Data::DObject(usize::MAX));
    }
    assert_eq!(object_to_string(obj.clone()), r#"{"child":null}"#);
  }

  // --- Error-shape compatibility with the pre-existing suite ---

  #[test]
  fn error_variants_match_original_contract() {
    test_init();
    assert!(matches!(
      object_from_string(r#"{"key": 1, }"#).err().unwrap(),
      ParseError::ExpectedCharacter('"')
    ));
    assert!(matches!(
      array_from_string("[1, 2, ]").err().unwrap(),
      ParseError::ExpectedValue
    ));
    assert!(matches!(
      object_from_string(r#"{"key" "value"}"#).err().unwrap(),
      ParseError::UnexpectedCharacter('"')
    ));
    assert!(matches!(
      object_from_string(r#"{"key": value}"#).err().unwrap(),
      ParseError::UnexpectedCharacter('v')
    ));
    assert!(matches!(
      array_from_string(r#"[1, "two" false]"#).err().unwrap(),
      ParseError::UnexpectedCharacter('f')
    ));
    assert_eq!(object_from_string("").err().unwrap(), ParseError::UnexpectedEof);
    assert_eq!(object_from_string("   ").err().unwrap(), ParseError::UnexpectedEof);
    assert!(matches!(
      object_from_string("[1]").err().unwrap(),
      ParseError::UnexpectedCharacter('[')
    ));
  }

  // --- Legacy helpers ---

  #[test]
  #[allow(deprecated)]
  fn escape_original_escapes_carriage_return() {
    // The old implementation replaced "\r" with itself, leaving a raw CR.
    assert_eq!(escape_original("a\rb"), "a\\rb");
    assert_eq!(unescape_original(&escape_original("a\r\n\t\"\\b")), "a\r\n\t\"\\b");
  }
}
