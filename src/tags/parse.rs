//! Strict address dialect parsers, independent of PLC I/O.

use super::{DataType, Tag, type_count};
use crate::{Area, Error, Result};

/// Parse only unsigned ASCII decimal digits; reject signs and empty components.
pub(super) fn number<T: std::str::FromStr>(text: &str) -> Result<T> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::InvalidInput("expected unsigned decimal"));
    }
    text.parse()
        .map_err(|_| Error::InvalidInput("decimal overflow"))
}

/// Parse a byte offset and optional bit suffix without floating-point conversion.
pub(crate) fn offset(text: &str) -> Result<(u32, u8)> {
    match text.split_once('.') {
        Some((byte, bit)) => Ok((number(byte)?, number(bit)?)),
        None => Ok((number(text)?, 0)),
    }
}

/// Resolve classic area prefixes, including German input/output aliases.
fn area(c: u8) -> Result<Area> {
    match c {
        b'M' => Ok(Area::Markers),
        b'I' | b'E' => Ok(Area::Inputs),
        b'Q' | b'A' => Ok(Area::Outputs),
        _ => Err(Error::InvalidInput("unknown tag area")),
    }
}
impl Tag {
    /// Parse PLC4X/STEP7 syntax with an explicit type suffix.
    pub fn parse_plc4x(address: &str) -> Result<Self> {
        if !address.is_ascii() {
            return Err(Error::InvalidInput("non-ASCII tag address"));
        }
        let normalized = address.trim().trim_start_matches('%').to_ascii_uppercase();
        let (address, dtype) = normalized
            .rsplit_once(':')
            .ok_or(Error::InvalidInput("missing tag type"))?;
        let (datatype, count) = type_count(dtype)?;
        let (area, db, position) = if let Some(rest) = address.strip_prefix("DB") {
            let (db, rest) = rest
                .split_once([':', '.'])
                .ok_or(Error::InvalidInput("missing DB offset"))?;
            let position = if rest.starts_with("DB") {
                if !["DBX", "DBB", "DBW", "DBD"]
                    .iter()
                    .any(|p| rest.starts_with(p))
                {
                    return Err(Error::InvalidInput("unknown STEP7 storage prefix"));
                }
                &rest[3..]
            } else {
                rest
            };
            (Area::Db, number(db)?, position)
        } else {
            let first = *address
                .as_bytes()
                .first()
                .ok_or(Error::InvalidInput("missing tag area"))?;
            let rest = &address[1..];
            (
                area(first)?,
                0,
                rest.strip_prefix(['B', 'W', 'D']).unwrap_or(rest),
            )
        };
        let (offset, bit) = offset(position)?;
        Self::new(area, db, offset, bit, datatype, count)
    }

    /// Parse nodeS7 DB or bare area forms. Non-bit scalar suffixes are rejected.
    pub fn parse_nodes7(address: &str) -> Result<Self> {
        if !address.is_ascii() {
            return Err(Error::InvalidInput("non-ASCII tag address"));
        }
        let normalized = address.trim().trim_start_matches('%').to_ascii_uppercase();
        let (area, db, rest) = if let Some(rest) = normalized.strip_prefix("DB") {
            let (db, rest) = rest
                .split_once(',')
                .ok_or(Error::InvalidInput("missing nodeS7 comma"))?;
            (Area::Db, number(db)?, rest)
        } else {
            let first = *normalized
                .as_bytes()
                .first()
                .ok_or(Error::InvalidInput("missing tag area"))?;
            (area(first)?, 0, &normalized[1..])
        };
        let split = rest
            .find(|c: char| c.is_ascii_digit())
            .ok_or(Error::InvalidInput("missing nodeS7 offset"))?;
        let (code, position) = rest.split_at(split);
        let (byte, trailing) = match position.split_once('.') {
            Some((byte, trailing)) => (number(byte)?, Some(trailing)),
            None => (number(position)?, None),
        };
        let datatype = match code {
            "" | "X" => DataType::Bool,
            "B" => DataType::Byte,
            "C" => DataType::Char,
            "W" => DataType::Word,
            "I" => DataType::Int,
            "D" | "DW" => DataType::DWord,
            "DI" => DataType::DInt,
            "R" => DataType::Real,
            "LR" => DataType::LReal,
            "LI" => DataType::LInt,
            "ULI" => DataType::ULInt,
            "LW" => DataType::LWord,
            "SINT" => DataType::SInt,
            "USINT" => DataType::USInt,
            "S" => DataType::String(number(
                trailing.ok_or(Error::InvalidInput("missing STRING capacity"))?,
            )?),
            "WS" => DataType::WString(number(
                trailing.ok_or(Error::InvalidInput("missing WSTRING capacity"))?,
            )?),
            _ => return Err(Error::InvalidInput("unknown nodeS7 type code")),
        };
        let bit = if datatype == DataType::Bool {
            number(trailing.ok_or(Error::InvalidInput("missing bit suffix"))?)?
        } else {
            if trailing.is_some() && !matches!(datatype, DataType::String(_) | DataType::WString(_))
            {
                return Err(Error::InvalidInput("unexpected nodeS7 suffix"));
            }
            0
        };
        Self::new(area, db, byte, bit, datatype, 1)
    }
}
