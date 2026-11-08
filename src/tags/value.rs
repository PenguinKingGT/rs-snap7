//! Dynamic tag values; numeric widths and temporal units remain explicit.

use super::{DataType, Tag};
use crate::{
    Error, Result,
    codec::{
        self,
        datetime::{self, Date, DateTime},
    },
};

/// A decoded tag value. TIME uses milliseconds; LTIME uses nanoseconds.
/// TOD/LTOD use the corresponding unsigned units. S5TIME is read-only milliseconds.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Boolean bit.
    Bool(bool),
    /// Signed integer or duration.
    Signed(i64),
    /// Unsigned integer, time of day, or S5TIME milliseconds.
    Unsigned(u64),
    /// IEEE binary32 without widening or rounding.
    Real(f32),
    /// IEEE binary64.
    LReal(f64),
    /// Latin-1 or BMP character.
    Character(char),
    /// Latin-1 or UTF-16 text according to the tag type.
    Text(String),
    /// Calendar date.
    Date(Date),
    /// Calendar date and time, retaining supported subsecond precision.
    DateTime(DateTime),
    /// Fixed element count; nested arrays are not supported.
    Array(Vec<Value>),
}

impl Tag {
    /// Decode exactly this tag's storage width; reject truncated or extra bytes.
    pub fn decode(&self, data: &[u8]) -> Result<Value> {
        if data.len() != self.size() {
            return Err(Error::InvalidInput("tag payload width mismatch"));
        }
        if self.count == 1 {
            return self.decode_scalar(data);
        }
        Ok(Value::Array(
            data.chunks_exact(self.datatype.size()?)
                .map(|part| self.decode_scalar(part))
                .collect::<Result<_>>()?,
        ))
    }

    /// Validate and encode all elements before any network write.
    /// BOOL output contains only its target bit; clients preserve neighboring bits.
    pub fn encode(&self, value: &Value) -> Result<Vec<u8>> {
        let mut data = vec![0; self.size()];
        if self.count == 1 {
            self.encode_scalar(&mut data, value)?;
        } else {
            let Value::Array(values) = value else {
                return Err(Error::InvalidInput("expected tag array"));
            };
            if values.len() != self.count {
                return Err(Error::InvalidInput("tag array count mismatch"));
            }
            for (part, value) in data.chunks_exact_mut(self.datatype.size()?).zip(values) {
                self.encode_scalar(part, value)?;
            }
        }
        Ok(data)
    }

    /// Decode a single element through checked core codecs.
    fn decode_scalar(&self, data: &[u8]) -> Result<Value> {
        use DataType as T;
        Ok(match self.datatype {
            T::SInt => Value::Signed(i64::from(codec::read::<i8>(data, 0)?)),
            T::Int => Value::Signed(i64::from(codec::read::<i16>(data, 0)?)),
            T::DInt => Value::Signed(i64::from(codec::read::<i32>(data, 0)?)),
            T::LInt => Value::Signed(codec::read::<i64>(data, 0)?),
            T::Byte => Value::Unsigned(u64::from(codec::read::<u8>(data, 0)?)),
            T::USInt => Value::Unsigned(u64::from(codec::read::<u8>(data, 0)?)),
            T::UInt => Value::Unsigned(u64::from(codec::read::<u16>(data, 0)?)),
            T::Word => Value::Unsigned(u64::from(codec::read::<u16>(data, 0)?)),
            T::UDInt => Value::Unsigned(u64::from(codec::read::<u32>(data, 0)?)),
            T::DWord => Value::Unsigned(u64::from(codec::read::<u32>(data, 0)?)),
            T::ULInt => Value::Unsigned(codec::read::<u64>(data, 0)?),
            T::LWord => Value::Unsigned(codec::read::<u64>(data, 0)?),
            T::Bool => Value::Bool(codec::get_bool(data, 0, self.bit)?),
            T::Char => Value::Character(codec::get_char(data, 0)?),
            T::WChar => Value::Character(codec::get_wchar(data, 0)?),
            T::Date => Value::Date(datetime::get_date(data, 0)?),
            T::Dt => Value::DateTime(datetime::get_dt(data, 0)?),
            T::Dtl => Value::DateTime(datetime::get_dtl(data, 0)?),
            T::Ldt => Value::DateTime(datetime::get_ldt(data, 0)?),
            T::Real => Value::Real(codec::read(data, 0)?),
            T::LReal => Value::LReal(codec::read(data, 0)?),
            T::Time => Value::Signed(i64::from(datetime::get_time(data, 0)?)),
            T::LTime => Value::Signed(datetime::get_ltime(data, 0)?),
            T::Tod => Value::Unsigned(u64::from(datetime::get_tod(data, 0)?)),
            T::LTod => Value::Unsigned(datetime::get_ltod(data, 0)?),
            T::S5Time => Value::Unsigned(u64::from(datetime::get_s5time(data, 0)?)),
            T::String(n) => {
                if data[0] > n {
                    return Err(Error::Protocol("STRING header exceeds tag capacity"));
                }
                Value::Text(codec::get_string(data, 0)?)
            }
            T::WString(n) => {
                if codec::read::<u16>(data, 0)? > n {
                    return Err(Error::Protocol("WSTRING header exceeds tag capacity"));
                }
                Value::Text(codec::get_wstring(data, 0)?)
            }
            T::FString(n) => Value::Text(codec::get_fstring(data, 0, usize::from(n), true)?),
        })
    }

    /// Encode one element with explicit variant and numeric range checks.
    fn encode_scalar(&self, data: &mut [u8], value: &Value) -> Result<()> {
        use DataType as T;
        match (self.datatype, value) {
            (T::SInt, Value::Signed(v)) => codec::write(
                data,
                0,
                i8::try_from(*v).map_err(|_| Error::InvalidInput("signed tag value overflow"))?,
            ),
            (T::Int, Value::Signed(v)) => codec::write(
                data,
                0,
                i16::try_from(*v).map_err(|_| Error::InvalidInput("signed tag value overflow"))?,
            ),
            (T::DInt, Value::Signed(v)) => codec::write(
                data,
                0,
                i32::try_from(*v).map_err(|_| Error::InvalidInput("signed tag value overflow"))?,
            ),
            (T::LInt, Value::Signed(v)) => codec::write(data, 0, *v),
            (T::Byte, Value::Unsigned(v)) => codec::write(
                data,
                0,
                u8::try_from(*v).map_err(|_| Error::InvalidInput("unsigned tag value overflow"))?,
            ),
            (T::USInt, Value::Unsigned(v)) => codec::write(
                data,
                0,
                u8::try_from(*v).map_err(|_| Error::InvalidInput("unsigned tag value overflow"))?,
            ),
            (T::UInt, Value::Unsigned(v)) => codec::write(
                data,
                0,
                u16::try_from(*v)
                    .map_err(|_| Error::InvalidInput("unsigned tag value overflow"))?,
            ),
            (T::Word, Value::Unsigned(v)) => codec::write(
                data,
                0,
                u16::try_from(*v)
                    .map_err(|_| Error::InvalidInput("unsigned tag value overflow"))?,
            ),
            (T::UDInt, Value::Unsigned(v)) => codec::write(
                data,
                0,
                u32::try_from(*v)
                    .map_err(|_| Error::InvalidInput("unsigned tag value overflow"))?,
            ),
            (T::DWord, Value::Unsigned(v)) => codec::write(
                data,
                0,
                u32::try_from(*v)
                    .map_err(|_| Error::InvalidInput("unsigned tag value overflow"))?,
            ),
            (T::ULInt, Value::Unsigned(v)) => codec::write(data, 0, *v),
            (T::LWord, Value::Unsigned(v)) => codec::write(data, 0, *v),
            (T::Bool, Value::Bool(v)) => codec::set_bool(data, 0, self.bit, *v),
            (T::Char, Value::Character(v)) => codec::set_char(data, 0, *v),
            (T::WChar, Value::Character(v)) => codec::set_wchar(data, 0, *v),
            (T::Date, Value::Date(v)) => datetime::set_date(data, 0, *v),
            (T::Dt, Value::DateTime(v)) => datetime::set_dt(data, 0, *v),
            (T::Dtl, Value::DateTime(v)) => datetime::set_dtl(data, 0, *v),
            (T::Ldt, Value::DateTime(v)) => datetime::set_ldt(data, 0, *v),
            (T::Real, Value::Real(v)) => codec::write(data, 0, *v),
            (T::LReal, Value::LReal(v)) => codec::write(data, 0, *v),
            (T::Time, Value::Signed(v)) => datetime::set_time(
                data,
                0,
                i32::try_from(*v).map_err(|_| Error::InvalidInput("TIME overflow"))?,
            ),
            (T::LTime, Value::Signed(v)) => datetime::set_ltime(data, 0, *v),
            (T::Tod, Value::Unsigned(v)) => datetime::set_tod(
                data,
                0,
                u32::try_from(*v).map_err(|_| Error::InvalidInput("TOD overflow"))?,
            ),
            (T::LTod, Value::Unsigned(v)) => datetime::set_ltod(data, 0, *v),
            (T::S5Time, _) => Err(Error::Unsupported("S5TIME writes")),
            (T::String(n), Value::Text(v)) => codec::set_string(data, 0, v, n),
            (T::WString(n), Value::Text(v)) => codec::set_wstring(data, 0, v, n),
            (T::FString(n), Value::Text(v)) => codec::set_fstring(data, 0, v, usize::from(n)),
            _ => Err(Error::InvalidInput("tag value variant mismatch")),
        }
    }
}
