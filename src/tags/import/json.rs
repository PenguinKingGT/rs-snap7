//! Recursive JSON duplicate detection before metadata can choose a PLC address.

use serde::{
    Deserialize, Deserializer,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Map, Number, Value};
use std::fmt;

/// Parsed JSON value whose objects cannot contain duplicate keys at any depth.
pub(super) struct Checked(pub Value);
impl<'de> Deserialize<'de> for Checked {
    /// Delegate tokenization/recursion limits to serde_json while checking map keys.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(CheckedVisitor).map(Self)
    }
}
/// JSON syntax visitor preserving scalar types without unchecked coercions.
struct CheckedVisitor;
impl<'de> Visitor<'de> for CheckedVisitor {
    type Value = Value;
    /// Describe the accepted JSON syntax to the deserializer.
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON without duplicate object keys")
    }
    /// Preserve JSON booleans.
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    /// Preserve signed integral numbers.
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    /// Preserve unsigned integral numbers.
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    /// Reject nonfinite or overflowing JSON numbers.
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("nonfinite JSON number"))
    }
    /// Preserve borrowed string tokens.
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }
    /// Preserve owned/escaped strings.
    fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }
    /// Preserve null.
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    /// Check every nested array element.
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(Checked(value)) = sequence.next_element()? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    /// Reject duplicates before replacing any earlier value or field.
    fn visit_map<A: MapAccess<'de>>(self, mut object: A) -> Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some((key, Checked(value))) = object.next_entry::<String, Checked>()? {
            if values.insert(key, value).is_some() {
                return Err(de::Error::custom("duplicate JSON object key"));
            }
        }
        Ok(Value::Object(values))
    }
}
