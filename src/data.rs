extern crate alloc;
use crate::dataobject::*;
use crate::dataarray::*;
use crate::databytes::DataBytes;

#[cfg(feature="no_std_support")]
use alloc::string::String;
#[cfg(feature="no_std_support")]
use alloc::borrow::ToOwned;
#[cfg(feature="no_std_support")]
use alloc::string::ToString;

/// Represents an NData value
///
/// DObject, DArray, and DBytes are considered *instances* and the clone() function will return a reference to the *same* instance.
#[derive(Debug, PartialEq)]
pub enum Data {
    /// Represents an existing instance of ndata::dataobject::DataObject, where data_ref is the value of the DataObject's data_ref field.
    ///
    /// ```
    /// # use ndata::Data;
    /// #
    /// let data_ref: usize = 0;
    /// let d = Data::DObject(data_ref);
    /// ```
    DObject(usize),
    /// Represents an existing instance of ndata::dataobject::DataArray, where data_ref is the value of the DataArray's data_ref field.
    ///
    /// ```
    /// # use ndata::Data;
    /// #
    /// let data_ref: usize = 0;
    /// let d = Data::DArray(data_ref);
    /// ```
    DArray(usize),
    /// Represents an existing instance of ndata::dataobject::DataBytes, where data_ref is the value of the DataBytes's data_ref field.
    ///
    /// ```
    /// # use ndata::Data;
    /// #
    /// let data_ref: usize = 0;
    /// let d = Data::DBytes(data_ref);
    /// ```
    DBytes(usize),
    /// Contains a String value
    ///
    /// ```
    /// # use ndata::Data;
    /// #
    /// let d = Data::DString("hello world".to_owned());
    /// ```
    DString(String),
    /// Contains a bool value
    ///
    /// ```
    /// # use ndata::Data;
    /// #
    /// let d = Data::DBoolean(true);
    /// ```
    DBoolean(bool),
    /// Contains an f64 value
    ///
    /// ```
    /// # use ndata::Data;
    /// #
    /// let d = Data::DFloat(99.99);
    /// ```
    DFloat(f64),
    /// Contains an i64 value
    ///
    /// ```
    /// # use ndata::Data;
    /// #
    /// let d = Data::DInt(99);
    /// ```
    DInt(i64),
    /// Contains no value
    ///
    /// ```
    /// # use ndata::Data;
    /// #
    /// let d = Data::DNull;
    /// ```
    DNull,
}

impl Clone for Data{
    /// Returns a copy of the value.
    ///
    /// Since DObject, DArray, and DBytes are *references* to instances, the resulting Data
    /// will point to the *same* instance as the original.
    fn clone(&self) -> Self {
        if let Data::DInt(d) = self { return Data::DInt(*d); }
        if let Data::DFloat(d) = self { return Data::DFloat(*d); }
        if let Data::DBoolean(d) = self { return Data::DBoolean(*d); }
        if let Data::DString(d) = self { return Data::DString(d.to_owned()); }
        if let Data::DObject(d) = self { return Data::DObject(*d); }
        if let Data::DArray(d) = self { return Data::DArray(*d); }
        if let Data::DBytes(d) = self { return Data::DBytes(*d); }
        Data::DNull
    }
}

impl Data {
    /// Returns ```true``` if the value is of type ```DInt``` or ```DFloat```.
    pub fn is_number(&self) -> bool {
        self.is_int() || self.is_float()
    }

    /// Returns ```true``` if the value is of type ```DInt```.
    pub fn is_int(&self) -> bool {
        matches!(self, Data::DInt(_))
    }

    /// Returns ```true``` if the value is of type ```DFloat```.
    pub fn is_float(&self) -> bool {
        matches!(self, Data::DFloat(_))
    }

    /// Returns ```true``` if the value is of type ```DString```.
    pub fn is_string(&self) -> bool {
        matches!(self, Data::DString(_))
    }

    /// Returns ```true``` if the value is of type ```DBoolean```.
    pub fn is_boolean(&self) -> bool {
        matches!(self, Data::DBoolean(_))
    }

    /// Returns ```true``` if the value is of type ```DObject```.
    pub fn is_object(&self) -> bool {
        matches!(self, Data::DObject(_))
    }

    /// Returns ```true``` if the value is of type ```DArray```.
    pub fn is_array(&self) -> bool {
        matches!(self, Data::DArray(_))
    }

    /// Returns ```true``` if the value is of type ```DBytes```.
    pub fn is_bytes(&self) -> bool {
        matches!(self, Data::DBytes(_))
    }

    /// Returns ```true``` if the value is of type ```DNull```.
    pub fn is_null(&self) -> bool { // Changed to take &self
        matches!(self, Data::DNull)
    }

    /// Returns the underlying ```i64``` value, or panics if not ```DInt```.
    pub fn int(&self) -> i64 {
        if let Data::DInt(i) = self { *i } else { panic!("Not an int: {:?}/{}", self, Data::as_string(self.clone())); }
    }

    /// Returns the underlying ```f64``` value, or panics if not ```DFloat```.
    pub fn float(&self) -> f64 {
        if let Data::DFloat(f) = self { *f } else { panic!("Not a float: {:?}/{}", self, Data::as_string(self.clone())); }
    }

    /// Returns the underlying ```bool``` value, or panics if not ```DBoolean```.
    pub fn boolean(&self) -> bool {
        if let Data::DBoolean(b) = self { *b } else { panic!("Not a boolean: {:?}/{}", self, Data::as_string(self.clone())); }
    }

    /// Returns the underlying ```String``` value, or if not converts it to ```DString```.
    pub fn string(&self) -> String {
        if let Data::DString(s) = self { s.to_owned() } else { Data::as_string(self.clone()) }
    }

    /// Returns a new ```DataObject``` representing the underlying object instance,
    /// or panics if not ```DObject```.
    pub fn object(&self) -> DataObject { // Assumes DataObject::get correctly increments ref count.
        if let Data::DObject(i) = self { DataObject::get(*i) } else { panic!("Not an object: {:?}/{}", self, Data::as_string(self.clone())); }
    }

    /// Returns a new ```DataArray``` representing the underlying array instance,
    /// or panics if not ```DArray```.
    pub fn array(&self) -> DataArray { // Assumes DataArray::get correctly increments ref count.
        if let Data::DArray(i) = self { DataArray::get(*i) } else { panic!("Not an array: {:?}/{}", self, Data::as_string(self.clone())); }
    }

    /// Returns a new ```DataBytes``` representing the underlying byte buffer instance,
    /// or panics if not ```DBytes```.
    pub fn bytes(&self) -> DataBytes { // Assumes DataBytes::get correctly increments ref count.
        if let Data::DBytes(i) = self { DataBytes::get(*i) } else { panic!("Not a byte array: {:?}/{}", self, Data::as_string(self.clone())); }
    }

    /// Returns a ```String``` representation of the underlying value.
    pub fn as_string(a:Data) -> String { // This takes ownership of 'a'. Consider taking &Data.
        if a.is_float() { return a.float().to_string(); }
        if a.is_int() { return a.int().to_string(); }
        if a.is_string() { return a.string(); } // .string() already clones if necessary
        if a.is_boolean() { return a.boolean().to_string(); }
        if a.is_object() { return a.object().to_string(); }
        if a.is_array() { return a.array().to_string(); }
        if a.is_bytes() {
            return a.bytes().to_hex_string();
        }
        if a.is_null() { return "null".to_string(); }
        "".to_string()
    }

    // Return true if the two Data structs are equal
    pub fn equals(a:Data, b:Data) -> bool { // This takes ownership of 'a' and 'b'. Consider taking &Data.
        if a.is_float() { if b.is_float() { return a.float() == b.float(); } }
        else if a.is_int() { if b.is_int() { return a.int() == b.int(); } }
        else if a.is_string() { if b.is_string() { return a.string() == b.string(); } }
        else if a.is_boolean() { if b.is_boolean() { return a.boolean() == b.boolean(); } }
        else if let Data::DObject(i) = a { if let Data::DObject(j) = b { return i == j; } }
        else if let Data::DArray(i) = a { if let Data::DArray(j) = b { return i == j; } }
        else if let Data::DBytes(i) = a { if let Data::DBytes(j) = b { return i == j; } }
        else if a.is_null() { return b.is_null(); }
        false
    }

    /// Returns a string representation of the data type.
    pub fn type_name_owned(&self) -> &'static str {
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

/// The default for ```ndata.Data``` is ```DNull```.
impl Default for Data {
    fn default() -> Data {
        Data::DNull
    }
}
