//! Block catalog models and checked remote/offline metadata parsers.
//!
//! Offline compact headers expose encoded fields and raw timestamps; dates are
//! never synthesized from fixed values as in the Python compatibility helper.

pub(crate) mod transfer;
use crate::{BlockType, Error, Result, codec};
use std::collections::BTreeMap;

/// Counts for the seven supported classic S7 block types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlocksList(BTreeMap<BlockType, u16>);

impl BlocksList {
    /// Return the count for one block type; valid tables always contain all types.
    pub fn count(&self, kind: BlockType) -> u16 {
        self.0.get(&kind).copied().unwrap_or(0)
    }
    /// Iterate over block types and counts in wire-identifier order.
    pub fn iter(&self) -> impl Iterator<Item = (BlockType, u16)> + '_ {
        self.0.iter().map(|(k, v)| (*k, *v))
    }
    /// Decode exactly seven distinct four-byte catalog entries.
    pub fn decode(data: &[u8]) -> Result<Self> {
        if data.len() != 28 {
            return Err(Error::Protocol("block counts require seven entries"));
        }
        let mut entries = BTreeMap::new();
        for item in data.chunks_exact(4) {
            if item[0] != 0x30 {
                return Err(Error::Protocol("invalid block-count indicator"));
            }
            let kind = BlockType::try_from(item[1])
                .map_err(|_| Error::Protocol("unknown block-count type"))?;
            if entries
                .insert(kind, u16::from_be_bytes([item[2], item[3]]))
                .is_some()
            {
                return Err(Error::Protocol("duplicate block-count type"));
            }
        }
        Ok(Self(entries))
    }
}

/// One block-list entry, preserving language and the upstream unknown flag byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockEntry {
    /// PLC block number.
    pub number: u16,
    /// Uninterpreted metadata flag.
    pub flags: u8,
    /// PLC language identifier.
    pub language: u8,
}

/// Decode whole entries; empty lists are valid and partial entries are errors.
pub fn decode_entries(data: &[u8]) -> Result<Vec<BlockEntry>> {
    if !data.len().is_multiple_of(4) {
        return Err(Error::Protocol("partial block-list entry"));
    }
    Ok(data
        .chunks_exact(4)
        .map(|b| BlockEntry {
            number: u16::from_be_bytes([b[0], b[1]]),
            flags: b[2],
            language: b[3],
        })
        .collect())
}

/// Metadata from a canonical 78-byte block-info service payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockInfo {
    /// Catalog block type.
    pub kind: BlockType,
    /// PLC block number.
    pub number: u16,
    /// PLC language identifier.
    pub language: u8,
    /// Block flags.
    pub flags: u8,
    /// MC7/data body size in bytes.
    pub mc7_size: u16,
    /// Load-memory size.
    pub load_size: u32,
    /// Local-data size.
    pub local_data: u16,
    /// SBB section length.
    pub sbb_length: u16,
    /// Block checksum.
    pub checksum: u16,
    /// Encoded version byte.
    pub version: u8,
    /// Raw six-byte code timestamp.
    pub code_timestamp: [u8; 6],
    /// Raw six-byte interface timestamp.
    pub interface_timestamp: [u8; 6],
    /// Latin-1 author field with trailing padding removed.
    pub author: String,
    /// Latin-1 family field with trailing padding removed.
    pub family: String,
    /// Latin-1 header name with trailing padding removed.
    pub header: String,
}

impl BlockInfo {
    /// Parse the upstream block-info layout, rejecting missing and trailing bytes.
    pub fn decode(data: &[u8]) -> Result<Self> {
        if data.len() != 78 {
            return Err(Error::Protocol("block info must contain exactly 78 bytes"));
        }
        Ok(Self {
            kind: BlockType::try_from(data[1])
                .map_err(|_| Error::Protocol("unknown block-info type"))?,
            number: codec::read(data, 12)?,
            language: data[10],
            flags: data[9],
            mc7_size: codec::read(data, 40)?,
            load_size: codec::read(data, 14)?,
            local_data: codec::read(data, 38)?,
            sbb_length: codec::read(data, 34)?,
            checksum: codec::read(data, 68)?,
            version: data[66],
            code_timestamp: data[22..28]
                .try_into()
                .map_err(|_| Error::Protocol("invalid code timestamp"))?,
            interface_timestamp: data[28..34]
                .try_into()
                .map_err(|_| Error::Protocol("invalid interface timestamp"))?,
            author: text(&data[42..50]),
            family: text(&data[50..58]),
            header: text(&data[58..66]),
        })
    }
}

/// Encoded fields from a 36-byte S7 compact block header (version 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactBlockInfo {
    /// Encoded language identifier.
    pub language: u8,
    /// Compact sub-block identifier; distinct from catalog `BlockType` values.
    pub sub_block_type: u8,
    /// Block number.
    pub number: u16,
    /// Encoded load-memory size, at offset 8.
    pub load_size: u32,
    /// MC7/data length, at offset 34.
    pub mc7_size: u16,
    /// Encoded flags.
    pub flags: u8,
    /// Uninterpreted code timestamp bytes.
    pub code_timestamp: [u8; 6],
    /// Uninterpreted interface timestamp bytes.
    pub interface_timestamp: [u8; 6],
}

impl CompactBlockInfo {
    /// Inspect an offline header without inventing timestamps or requiring network I/O.
    pub fn decode(data: &[u8]) -> Result<Self> {
        if data.len() < 36 || data[2] != 1 {
            return Err(Error::InvalidInput(
                "missing version-1 compact block header",
            ));
        }
        Ok(Self {
            language: data[4],
            sub_block_type: data[5],
            number: codec::read(data, 6)?,
            load_size: codec::read(data, 8)?,
            mc7_size: codec::read(data, 34)?,
            flags: data[3],
            code_timestamp: data[16..22]
                .try_into()
                .map_err(|_| Error::InvalidInput("invalid code timestamp"))?,
            interface_timestamp: data[22..28]
                .try_into()
                .map_err(|_| Error::InvalidInput("invalid interface timestamp"))?,
        })
    }
}

/// Decode padded upstream byte text with no UTF-8 assumptions.
fn text(data: &[u8]) -> String {
    data.iter()
        .copied()
        .map(char::from)
        .collect::<String>()
        .trim_end_matches(['\0', ' '])
        .to_owned()
}
