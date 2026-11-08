//! Typed classic addresses, checked value conversion, and optional metadata imports.
//!
//! Parsing symbolic metadata does not enable S7CommPlus or optimized DB access.

mod datatype;
#[cfg(feature = "tag-import")]
pub mod import;
mod parse;
mod value;

pub use datatype::DataType;
pub(crate) use parse::offset as parse_offset;
pub use value::Value;

use crate::{Area, Error, Result};
use std::{fmt, str::FromStr};

/// Validated classic address with optional symbolic metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    area: Area,
    db: u16,
    offset: u32,
    bit: u8,
    datatype: DataType,
    count: usize,
    name: String,
    symbolic: Option<Symbolic>,
}

/// Metadata for an optimized address; classic clients explicitly reject it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbolic {
    /// Access area followed by local identifier components.
    pub path: Vec<u32>,
    /// Layout CRC, zero when no CRC was supplied.
    pub crc: u32,
}

impl Tag {
    /// Validate a classic address, array count, bit index and complete byte range.
    /// BOOL arrays occupy one byte per element, with the same bit index in each.
    pub fn new(
        area: Area,
        db: u16,
        offset: u32,
        bit: u8,
        datatype: DataType,
        count: usize,
    ) -> Result<Self> {
        let size = datatype
            .size()?
            .checked_mul(count)
            .ok_or(Error::InvalidInput("tag size overflow"))?;
        if !matches!(
            area,
            Area::Db | Area::Inputs | Area::Outputs | Area::Markers
        ) || (area != Area::Db && db != 0)
            || bit > 7
            || (datatype != DataType::Bool && bit != 0)
            || count == 0
            || u64::from(offset) + size as u64 > 0x200000
        {
            return Err(Error::InvalidInput("invalid tag address or count"));
        }
        Ok(Self {
            area,
            db,
            offset,
            bit,
            datatype,
            count,
            name: String::new(),
            symbolic: None,
        })
    }

    /// Autodetect colon/comma dialects; strict mode rejects bare area shortcuts.
    pub fn parse(address: &str, strict: bool) -> Result<Self> {
        if address.contains(',') {
            Self::parse_nodes7(address)
        } else if address.contains(':') {
            Self::parse_plc4x(address)
        } else if strict {
            Err(Error::InvalidInput("ambiguous tag dialect"))
        } else {
            Self::parse_nodes7(address)
        }
    }

    /// Attach a display name without changing the address.
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Preserve a nonempty symbolic path without providing symbolic network access.
    pub fn with_symbolic(mut self, path: Vec<u32>, crc: u32) -> Result<Self> {
        if path.is_empty() || path.len() > 64 {
            return Err(Error::InvalidInput("invalid symbolic path length"));
        }
        self.symbolic = Some(Symbolic { path, crc });
        Ok(self)
    }

    /// Parse a hexadecimal access-area/LID path, retaining it as metadata only.
    pub fn from_access_string(
        address: &str,
        datatype: DataType,
        count: usize,
        crc: u32,
    ) -> Result<Self> {
        let path: Vec<u32> = address
            .trim()
            .split('.')
            .map(|p| {
                u32::from_str_radix(p, 16).map_err(|_| Error::InvalidInput("invalid symbolic path"))
            })
            .collect::<Result<_>>()?;
        let first = *path
            .first()
            .ok_or(Error::InvalidInput("empty symbolic path"))?;
        let (area, db) = match first {
            0x8a0e0000..=0x8a0effff => (Area::Db, (first - 0x8a0e0000) as u16),
            80 => (Area::Inputs, 0),
            81 => (Area::Outputs, 0),
            82 => (Area::Markers, 0),
            _ => return Err(Error::Unsupported("unknown symbolic access area")),
        };
        Self::new(area, db, 0, 0, datatype, count)?.with_symbolic(path, crc)
    }

    /// Memory area.
    pub fn area(&self) -> Area {
        self.area
    }
    /// DB number; zero in other areas.
    pub fn db_number(&self) -> u16 {
        self.db
    }
    /// Byte offset in the selected area.
    pub fn offset(&self) -> u32 {
        self.offset
    }
    /// Bit index for BOOL values.
    pub fn bit(&self) -> u8 {
        self.bit
    }
    /// Element data type.
    pub fn datatype(&self) -> DataType {
        self.datatype
    }
    /// Number of elements.
    pub fn count(&self) -> usize {
        self.count
    }
    /// Optional display name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Optimized addressing metadata, when supplied.
    pub fn symbolic(&self) -> Option<&Symbolic> {
        self.symbolic.as_ref()
    }
    /// Total checked storage width.
    pub fn size(&self) -> usize {
        self.datatype.size().expect("validated type") * self.count
    }
    /// Reject symbolic addressing before any classic request is transmitted.
    pub(crate) fn classic(&self) -> Result<()> {
        if self.symbolic.is_some() {
            return Err(Error::Unsupported("symbolic PLC access"));
        }
        Ok(())
    }
    /// Merge encoded BOOL bits into existing storage, preserving all neighbors.
    pub(crate) fn merge_bits(&self, encoded: &mut [u8], existing: &[u8]) {
        let mask = 1u8 << self.bit;
        for (value, old) in encoded.iter_mut().zip(existing) {
            *value = (*old & !mask) | (*value & mask);
        }
    }
}
impl FromStr for Tag {
    type Err = Error;
    /// Parse strict, unambiguous tag syntax.
    fn from_str(address: &str) -> Result<Self> {
        Self::parse(address, true)
    }
}
impl fmt::Display for Tag {
    /// Render a lossless canonical PLC4X address, including array counts.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.area {
            Area::Db => write!(f, "DB{}:", self.db)?,
            Area::Inputs => f.write_str("I")?,
            Area::Outputs => f.write_str("Q")?,
            Area::Markers => f.write_str("M")?,
            _ => unreachable!("validated tag area"),
        }
        write!(f, "{}", self.offset)?;
        if self.datatype == DataType::Bool {
            write!(f, ".{}", self.bit)?;
        }
        write!(f, ":{}", self.datatype)?;
        if self.count > 1 {
            write!(f, "[{}]", self.count)?;
        }
        Ok(())
    }
}

/// Split a terminal decimal bracket suffix without accepting extra syntax.
fn parse_brackets(text: &str) -> Result<(&str, usize)> {
    let (base, suffix) = text
        .split_once('[')
        .ok_or(Error::InvalidInput("missing type size"))?;
    let n = suffix
        .strip_suffix(']')
        .ok_or(Error::InvalidInput("unclosed type size"))?;
    Ok((base, parse::number(n)?))
}

/// Parse a scalar type or a fixed-width array type.
pub(crate) fn type_count(text: &str) -> Result<(DataType, usize)> {
    if let Ok(datatype) = text.parse() {
        return Ok((datatype, 1));
    }
    let (base, count) = parse_brackets(text)?;
    if count == 0 {
        return Err(Error::InvalidInput("empty tag array"));
    }
    Ok((base.parse()?, count))
}

/// Typed browse metadata supplied by an external symbolic browser.
#[derive(Debug, Clone)]
pub struct BrowseVariable {
    /// Variable name; empty names are ignored.
    pub name: String,
    /// Classic memory address and type.
    pub tag: Tag,
    /// Optional LID; metadata only, with classic I/O rejected.
    pub lid: Option<u32>,
    /// Symbol-layout CRC.
    pub symbol_crc: u32,
}

/// Convert browse results, retaining symbolic metadata and rejecting duplicates.
pub fn from_browse(
    variables: &[BrowseVariable],
) -> Result<std::collections::BTreeMap<String, Tag>> {
    let mut tags = std::collections::BTreeMap::new();
    for variable in variables {
        if variable.name.is_empty() {
            continue;
        }
        let mut tag = variable.tag.clone().named(&variable.name);
        if let Some(lid) = variable.lid {
            tag = tag.with_symbolic(vec![lid], variable.symbol_crc)?;
        }
        if tags.insert(variable.name.clone(), tag).is_some() {
            return Err(Error::InvalidInput("duplicate browse variable"));
        }
    }
    Ok(tags)
}
