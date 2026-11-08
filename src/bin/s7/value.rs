//! CLI value parsing and stable JSON presentation.

use rs_snap7::{
    Error, Result,
    tags::{DataType, Tag, Value},
};

/// Parse text or a JSON array, retaining strict numeric signedness and widths.
pub(super) fn parse(tag: &Tag, text: &str) -> Result<Value> {
    if tag.count() > 1 {
        let values: serde_json::Value =
            serde_json::from_str(text).map_err(|_| Error::InvalidInput("expected JSON array"))?;
        let values = values
            .as_array()
            .ok_or(Error::InvalidInput("expected JSON array"))?;
        return Ok(Value::Array(
            values
                .iter()
                .map(|value| {
                    let text = match value {
                        serde_json::Value::String(s) => s.clone(),
                        _ => value.to_string(),
                    };
                    scalar(tag.datatype(), &text)
                })
                .collect::<Result<_>>()?,
        ));
    }
    scalar(tag.datatype(), text)
}
/// Parse one scalar; calendar inputs use the library codecs directly instead.
fn scalar(datatype: DataType, text: &str) -> Result<Value> {
    use DataType as T;
    let invalid = || Error::InvalidInput("value does not match tag type");
    Ok(match datatype {
        T::Bool => Value::Bool(match text.to_ascii_lowercase().as_str() {
            "true" | "1" => true,
            "false" | "0" => false,
            _ => return Err(invalid()),
        }),
        T::SInt | T::Int | T::DInt | T::LInt | T::Time | T::LTime => {
            Value::Signed(text.parse().map_err(|_| invalid())?)
        }
        T::Byte
        | T::USInt
        | T::UInt
        | T::UDInt
        | T::ULInt
        | T::Word
        | T::DWord
        | T::LWord
        | T::Tod
        | T::LTod => Value::Unsigned(text.parse().map_err(|_| invalid())?),
        T::Real => Value::Real(text.parse().map_err(|_| invalid())?),
        T::LReal => Value::LReal(text.parse().map_err(|_| invalid())?),
        T::String(_) | T::WString(_) | T::FString(_) => Value::Text(text.to_owned()),
        T::Char | T::WChar => {
            let mut chars = text.chars();
            let value = chars.next().ok_or_else(invalid)?;
            if chars.next().is_some() {
                return Err(invalid());
            }
            Value::Character(value)
        }
        _ => {
            return Err(Error::Unsupported(
                "CLI calendar/S5TIME writes; use the typed library API",
            ));
        }
    })
}
/// Render typed values without locale-dependent number formatting.
pub(super) fn json(value: &Value) -> serde_json::Value {
    use serde_json::json;
    match value {
        Value::Bool(v) => json!(v),
        Value::Signed(v) => json!(v),
        Value::Unsigned(v) => json!(v),
        Value::Real(v) => json!(v),
        Value::LReal(v) => json!(v),
        Value::Character(v) => json!(v.to_string()),
        Value::Text(v) => json!(v),
        Value::Date(v) => json!(format!("{:04}-{:02}-{:02}", v.year(), v.month(), v.day())),
        Value::DateTime(v) => {
            json!({"date":format!("{:04}-{:02}-{:02}",v.date().year(),v.date().month(),v.date().day()),"hour":v.hour(),"minute":v.minute(),"second":v.second(),"nanosecond":v.nanosecond()})
        }
        Value::Array(values) => serde_json::Value::Array(values.iter().map(json).collect()),
    }
}
