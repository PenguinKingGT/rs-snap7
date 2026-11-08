//! Validated Siemens tag data types and their storage widths.

use crate::{Error, Result};
use std::{fmt, str::FromStr};

/// A Siemens storage type. String capacity is part of the type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    /// Boolean bit.
    Bool,
    /// Byte.
    Byte,
    /// Signed byte.
    SInt,
    /// Unsigned byte.
    USInt,
    /// Latin-1 character.
    Char,
    /// Signed word.
    Int,
    /// Unsigned word.
    UInt,
    /// Bit word.
    Word,
    /// BMP character.
    WChar,
    /// Calendar date.
    Date,
    /// Read-only BCD duration in milliseconds.
    S5Time,
    /// Signed double word.
    DInt,
    /// Unsigned double word.
    UDInt,
    /// Bit double word.
    DWord,
    /// IEEE binary32.
    Real,
    /// Signed milliseconds.
    Time,
    /// Milliseconds since midnight.
    Tod,
    /// Signed long word.
    LInt,
    /// Unsigned long word.
    ULInt,
    /// Bit long word.
    LWord,
    /// IEEE binary64.
    LReal,
    /// Signed nanoseconds.
    LTime,
    /// Nanoseconds since midnight.
    LTod,
    /// Long date and time.
    Ldt,
    /// BCD date and time.
    Dt,
    /// Structured date and time.
    Dtl,
    /// Latin-1 STRING with at most 254 characters.
    String(u8),
    /// UTF-16BE WSTRING with at most 16382 code units.
    WString(u16),
    /// Space-padded Latin-1 string without a header.
    FString(u16),
}
impl DataType {
    /// Return the checked storage width, including string headers.
    pub fn size(self) -> Result<usize> {
        Ok(match self {
            Self::Bool => 1,
            Self::Byte => 1,
            Self::SInt => 1,
            Self::USInt => 1,
            Self::Char => 1,
            Self::Int => 2,
            Self::UInt => 2,
            Self::Word => 2,
            Self::WChar => 2,
            Self::Date => 2,
            Self::S5Time => 2,
            Self::DInt => 4,
            Self::UDInt => 4,
            Self::DWord => 4,
            Self::Real => 4,
            Self::Time => 4,
            Self::Tod => 4,
            Self::LInt => 8,
            Self::ULInt => 8,
            Self::LWord => 8,
            Self::LReal => 8,
            Self::LTime => 8,
            Self::LTod => 8,
            Self::Ldt => 8,
            Self::Dt => 8,
            Self::Dtl => 12,
            Self::String(n) if n <= 254 => 2 + usize::from(n),
            Self::WString(n) if n <= 16382 => 4 + 2 * usize::from(n),
            Self::FString(n) if n > 0 => usize::from(n),
            _ => return Err(Error::InvalidInput("invalid string capacity")),
        })
    }
}
impl fmt::Display for DataType {
    /// Render the canonical Siemens type name.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool => f.write_str("BOOL"),
            Self::Byte => f.write_str("BYTE"),
            Self::SInt => f.write_str("SINT"),
            Self::USInt => f.write_str("USINT"),
            Self::Char => f.write_str("CHAR"),
            Self::Int => f.write_str("INT"),
            Self::UInt => f.write_str("UINT"),
            Self::Word => f.write_str("WORD"),
            Self::WChar => f.write_str("WCHAR"),
            Self::Date => f.write_str("DATE"),
            Self::S5Time => f.write_str("S5TIME"),
            Self::DInt => f.write_str("DINT"),
            Self::UDInt => f.write_str("UDINT"),
            Self::DWord => f.write_str("DWORD"),
            Self::Real => f.write_str("REAL"),
            Self::Time => f.write_str("TIME"),
            Self::Tod => f.write_str("TOD"),
            Self::LInt => f.write_str("LINT"),
            Self::ULInt => f.write_str("ULINT"),
            Self::LWord => f.write_str("LWORD"),
            Self::LReal => f.write_str("LREAL"),
            Self::LTime => f.write_str("LTIME"),
            Self::LTod => f.write_str("LTOD"),
            Self::Ldt => f.write_str("LDT"),
            Self::Dt => f.write_str("DT"),
            Self::Dtl => f.write_str("DTL"),
            Self::String(n) => write!(f, "STRING[{n}]"),
            Self::WString(n) => write!(f, "WSTRING[{n}]"),
            Self::FString(n) => write!(f, "FSTRING[{n}]"),
        }
    }
}
impl FromStr for DataType {
    type Err = Error;
    /// Parse a scalar type; array counts belong to Tag.
    fn from_str(text: &str) -> Result<Self> {
        let text = text.trim().to_ascii_uppercase();
        let value = match text.as_str() {
            "BOOL" => Self::Bool,
            "BYTE" => Self::Byte,
            "SINT" => Self::SInt,
            "USINT" => Self::USInt,
            "CHAR" => Self::Char,
            "INT" => Self::Int,
            "UINT" => Self::UInt,
            "WORD" => Self::Word,
            "WCHAR" => Self::WChar,
            "DATE" => Self::Date,
            "S5TIME" => Self::S5Time,
            "DINT" => Self::DInt,
            "UDINT" => Self::UDInt,
            "DWORD" => Self::DWord,
            "REAL" => Self::Real,
            "TIME" => Self::Time,
            "TOD" => Self::Tod,
            "LINT" => Self::LInt,
            "ULINT" => Self::ULInt,
            "LWORD" => Self::LWord,
            "LREAL" => Self::LReal,
            "LTIME" => Self::LTime,
            "LTOD" => Self::LTod,
            "LDT" => Self::Ldt,
            "DT" => Self::Dt,
            "DTL" => Self::Dtl,
            _ => {
                let (base, n) = super::parse_brackets(&text)?;
                let n = u16::try_from(n)
                    .map_err(|_| Error::InvalidInput("string capacity overflow"))?;
                match base {
                    "STRING" => Self::String(
                        u8::try_from(n)
                            .map_err(|_| Error::InvalidInput("STRING capacity overflow"))?,
                    ),
                    "WSTRING" => Self::WString(n),
                    "FSTRING" => Self::FString(n),
                    _ => return Err(Error::InvalidInput("unknown tag type")),
                }
            }
        };
        value.size()?;
        Ok(value)
    }
}
