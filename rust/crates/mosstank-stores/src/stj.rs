//! Reading and writing the stores' JSON documents as System.Text.Json reads
//! and writes the C#'s classes: properties in declaration order, matched
//! ignoring case on the way in (`PropertyNameCaseInsensitive`), a missing
//! property keeping its default, a later duplicate replacing an earlier
//! one, enums as their numbers, and a value of the wrong JSON kind failing
//! the whole document.

use openac_plugin_compat::json::{JsonNumber, JsonValue, system_text_json};
use openac_plugin_compat::number::{self, NumberStyles};

/// Why a document did not read (the C#'s `JsonException`).
pub type JsonError = String;

/// `new JsonSerializerOptions { WriteIndented = true }`.
pub const INDENTED: system_text_json::Options = system_text_json::Options::INDENTED;

/// A document as `JsonSerializer.Serialize` writes it, indented.
#[must_use]
pub fn write(document: &JsonValue) -> String {
    system_text_json::to_string(document, &INDENTED)
        .expect("store documents hold only finite numbers")
}

fn cannot_convert(path: &str, target: &str) -> JsonError {
    format!("The JSON value could not be converted to {target}. Path: {path}")
}

/// The members of one JSON object, read as a C# class's properties.
#[derive(Debug, Clone, Copy)]
pub struct Object<'a> {
    members: &'a [(String, JsonValue)],
    path: &'a str,
}

impl<'a> Object<'a> {
    /// The object `value` is; `None` for JSON `null`.
    ///
    /// # Errors
    /// Any other kind of value.
    pub fn of(
        value: &'a JsonValue,
        path: &'a str,
        target: &str,
    ) -> Result<Option<Self>, JsonError> {
        match value {
            JsonValue::Null => Ok(None),
            JsonValue::Object(members) => Ok(Some(Self { members, path })),
            _ => Err(cannot_convert(path, target)),
        }
    }

    /// The property's value (the last one, ignoring case), `None` when the
    /// document does not have it.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&'a JsonValue> {
        self.members
            .iter()
            .rev()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value)
    }

    fn path_of(&self, name: &str) -> String {
        format!("{}.{name}", self.path)
    }

    /// A `bool` property.
    ///
    /// # Errors
    /// Not `true` or `false`.
    pub fn bool(&self, name: &str, default: bool) -> Result<bool, JsonError> {
        match self.get(name) {
            None => Ok(default),
            Some(JsonValue::Bool(value)) => Ok(*value),
            Some(_) => Err(cannot_convert(&self.path_of(name), "System.Boolean")),
        }
    }

    /// A `bool?` property.
    ///
    /// # Errors
    /// Not `true`, `false` or `null`.
    pub fn optional_bool(&self, name: &str) -> Result<Option<bool>, JsonError> {
        match self.get(name) {
            None | Some(JsonValue::Null) => Ok(None),
            Some(JsonValue::Bool(value)) => Ok(Some(*value)),
            Some(_) => Err(cannot_convert(
                &self.path_of(name),
                "System.Nullable`1[System.Boolean]",
            )),
        }
    }

    /// An `int` (or an enum stored as its number) property.
    ///
    /// # Errors
    /// Not a whole number that fits.
    pub fn int(&self, name: &str, default: i32) -> Result<i32, JsonError> {
        match self.get(name) {
            None => Ok(default),
            Some(value) => int_value(value, &self.path_of(name)),
        }
    }

    /// A `uint` property.
    ///
    /// # Errors
    /// Not a whole number that fits.
    pub fn uint(&self, name: &str, default: u32) -> Result<u32, JsonError> {
        match self.get(name) {
            None => Ok(default),
            Some(JsonValue::Number(number)) => integer_text(number)
                .and_then(|text| number::parse_u32(&text, NumberStyles::INTEGER))
                .ok_or_else(|| cannot_convert(&self.path_of(name), "System.UInt32")),
            Some(_) => Err(cannot_convert(&self.path_of(name), "System.UInt32")),
        }
    }

    /// A `double` property.
    ///
    /// # Errors
    /// Not a number.
    pub fn double(&self, name: &str, default: f64) -> Result<f64, JsonError> {
        match self.get(name) {
            None => Ok(default),
            Some(value) => double_value(value, &self.path_of(name)),
        }
    }

    /// A `float` property.
    ///
    /// # Errors
    /// Not a number a `float` holds.
    pub fn float(&self, name: &str, default: f32) -> Result<f32, JsonError> {
        match self.get(name) {
            None => Ok(default),
            Some(JsonValue::Number(number)) => {
                let text = number_text(number);
                number::parse_f32(&text, NumberStyles::FLOAT)
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| cannot_convert(&self.path_of(name), "System.Single"))
            }
            Some(_) => Err(cannot_convert(&self.path_of(name), "System.Single")),
        }
    }

    /// A `string` property (`null` is `None`).
    ///
    /// # Errors
    /// Not a string or `null`.
    pub fn string(&self, name: &str, default: &str) -> Result<Option<String>, JsonError> {
        match self.get(name) {
            None => Ok(Some(default.to_owned())),
            Some(value) => string_value(value, &self.path_of(name)),
        }
    }

    /// A `string[]` property (`null` is `None`, a `null` item is `None`).
    ///
    /// # Errors
    /// Not an array of strings or `null`.
    pub fn strings(&self, name: &str) -> Result<Option<Vec<Option<String>>>, JsonError> {
        let path = self.path_of(name);
        match self.get(name) {
            None => Ok(Some(Vec::new())),
            Some(JsonValue::Null) => Ok(None),
            Some(JsonValue::Array(items)) => items
                .iter()
                .map(|item| string_value(item, &path))
                .collect::<Result<Vec<_>, _>>()
                .map(Some),
            Some(_) => Err(cannot_convert(&path, "System.String[]")),
        }
    }

    /// An array property of objects (`null` is `None`, a `null` item is
    /// `None`); `missing` when the document does not have it.
    ///
    /// # Errors
    /// Not an array, or an item that is not an object or `null`.
    pub fn objects(
        &self,
        name: &str,
        missing: Option<Vec<Option<Object<'a>>>>,
        target: &str,
    ) -> Result<Option<Vec<Option<Object<'a>>>>, JsonError> {
        let path = self.path;
        match self.get(name) {
            None => Ok(missing),
            Some(JsonValue::Null) => Ok(None),
            Some(JsonValue::Array(items)) => items
                .iter()
                .map(|item| Object::of(item, path, target))
                .collect::<Result<Vec<_>, _>>()
                .map(Some),
            Some(_) => Err(cannot_convert(&self.path_of(name), target)),
        }
    }

    /// A `Dictionary<string, T>` property's entries (`null` is `None`),
    /// each value as JSON.
    ///
    /// # Errors
    /// Not an object or `null`.
    pub fn map(
        &self,
        name: &str,
        target: &str,
    ) -> Result<Option<Vec<(&'a str, &'a JsonValue)>>, JsonError> {
        match self.get(name) {
            None => Ok(Some(Vec::new())),
            Some(JsonValue::Null) => Ok(None),
            Some(JsonValue::Object(members)) => Ok(Some(
                members
                    .iter()
                    .map(|(key, value)| (key.as_str(), value))
                    .collect(),
            )),
            Some(_) => Err(cannot_convert(&self.path_of(name), target)),
        }
    }
}

fn number_text(number: &JsonNumber) -> String {
    match number {
        JsonNumber::I64(value) => value.to_string(),
        JsonNumber::U64(value) => value.to_string(),
        JsonNumber::F64(value) => number::to_string_f64(*value),
        JsonNumber::F32(value) => number::to_string_f32(*value),
        JsonNumber::Text(text) => text.clone(),
    }
}

/// The text of a number written without a fraction or exponent, as
/// `Utf8JsonReader.TryGetInt32` takes it.
fn integer_text(number: &JsonNumber) -> Option<String> {
    let text = number_text(number);
    (!text.contains(['.', 'e', 'E'])).then_some(text)
}

/// A JSON value as an `int`.
///
/// # Errors
/// Not a whole number that fits.
pub fn int_value(value: &JsonValue, path: &str) -> Result<i32, JsonError> {
    match value {
        JsonValue::Number(number) => integer_text(number)
            .and_then(|text| number::parse_i32(&text, NumberStyles::INTEGER))
            .ok_or_else(|| cannot_convert(path, "System.Int32")),
        _ => Err(cannot_convert(path, "System.Int32")),
    }
}

/// A JSON value as a `double`.
///
/// # Errors
/// Not a finite number.
pub fn double_value(value: &JsonValue, path: &str) -> Result<f64, JsonError> {
    match value {
        JsonValue::Number(number) => number::parse_f64(&number_text(number), NumberStyles::FLOAT)
            .filter(|value| value.is_finite())
            .ok_or_else(|| cannot_convert(path, "System.Double")),
        _ => Err(cannot_convert(path, "System.Double")),
    }
}

/// A JSON value as a `string` (`null` is `None`).
///
/// # Errors
/// Not a string or `null`.
pub fn string_value(value: &JsonValue, path: &str) -> Result<Option<String>, JsonError> {
    match value {
        JsonValue::Null => Ok(None),
        JsonValue::String(text) => Ok(Some(text.clone())),
        _ => Err(cannot_convert(path, "System.String")),
    }
}

/// A JSON value as a `bool`.
///
/// # Errors
/// Not `true` or `false`.
pub fn bool_value(value: &JsonValue, path: &str) -> Result<bool, JsonError> {
    match value {
        JsonValue::Bool(value) => Ok(*value),
        _ => Err(cannot_convert(path, "System.Boolean")),
    }
}

/// `"text"` or `null`.
#[must_use]
pub fn string_or_null(text: Option<&str>) -> JsonValue {
    text.map_or(JsonValue::Null, |text| JsonValue::String(text.to_owned()))
}

/// A string.
#[must_use]
pub fn string(text: &str) -> JsonValue {
    JsonValue::String(text.to_owned())
}

/// An `int`.
#[must_use]
pub fn int(value: i32) -> JsonValue {
    JsonValue::Number(JsonNumber::I64(i64::from(value)))
}

/// A `uint`.
#[must_use]
pub fn uint(value: u32) -> JsonValue {
    JsonValue::Number(JsonNumber::U64(u64::from(value)))
}

/// A `double`.
#[must_use]
pub fn double(value: f64) -> JsonValue {
    JsonValue::Number(JsonNumber::F64(value))
}

/// A `float`.
#[must_use]
pub fn float(value: f32) -> JsonValue {
    JsonValue::Number(JsonNumber::F32(value))
}

/// A `string[]`.
#[must_use]
pub fn strings<S: AsRef<str>>(items: &[S]) -> JsonValue {
    JsonValue::Array(items.iter().map(|item| string(item.as_ref())).collect())
}
