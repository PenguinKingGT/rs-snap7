//! Block catalog, upload/download, deletion, and offline inspection entry points.

use super::Client;
use crate::{
    BlockType, Error, Result,
    blocks::{
        self, BlockEntry, BlockInfo, BlocksList,
        transfer::{Catalog, Download, Upload},
    },
    protocol::{self, blocks as wire},
};

impl Client {
    /// Read all seven catalog block counts, validating the complete table.
    pub fn list_blocks(&self) -> Result<BlocksList> {
        self.observe("list_blocks", || {
            let mut session = self.session()?;
            let data =
                session.transfer(Catalog::new(protocol::userdata_request(0, 3, 1, None)?, 1))?;
            session.validate(BlocksList::decode(&data))
        })
    }
    /// Read all entries of one type, including language/flags and fragmented responses.
    pub fn list_block_entries(&self, kind: BlockType) -> Result<Vec<BlockEntry>> {
        self.observe("list_block_entries", || {
            let mut session = self.session()?;
            let data = session.transfer(Catalog::new(
                protocol::userdata_request(0, 3, 2, Some(&[0x30, kind as u8]))?,
                2,
            ))?;
            session.validate(blocks::decode_entries(&data))
        })
    }
    /// Return at most `limit` block numbers. A zero limit performs no wire request.
    pub fn list_blocks_of_type(&self, kind: BlockType, limit: usize) -> Result<Vec<u16>> {
        self.observe("list_blocks_of_type", || {
            if limit == 0 {
                return Ok(Vec::new());
            }
            Ok(self
                .list_block_entries(kind)?
                .into_iter()
                .take(limit)
                .map(|entry| entry.number)
                .collect())
        })
    }
    /// Read canonical remote metadata and verify its type and block number.
    pub fn get_block_info(&self, kind: BlockType, number: u16) -> Result<BlockInfo> {
        self.observe("get_block_info", || {
            self.session()?.block_info(kind, number)
        })
    }
    /// Upload a DB's data body, omitting its initial 36-byte compact header.
    pub fn upload(&self, number: u16) -> Result<Vec<u8>> {
        self.observe("upload", || {
            self.session()?
                .transfer(Upload::new(BlockType::Db, number, false))
        })
    }
    /// Upload any block type with its complete header/body/footer representation.
    /// The returned Vec's length replaces Python's redundant `(buffer, size)` tuple.
    pub fn full_upload(&self, kind: BlockType, number: u16) -> Result<Vec<u8>> {
        self.observe("full_upload", || {
            self.session()?.transfer(Upload::new(kind, number, true))
        })
    }
    /// Download a DB representation using PLC-driven slice jobs. `None` extracts
    /// its number from bytes 6–7; short input requires an explicit number.
    /// Interrupted downloads have an uncertain remote outcome and never replay.
    pub fn download(&self, data: &[u8], number: Option<u16>) -> Result<()> {
        self.observe("download", || {
            let number = download_number(data, number)?;
            wire::download_request(0, number, data)?;
            let mut session = self.session()?;
            let deadline = session.deadline();
            session.ensure_ready(deadline)?;
            let machine = Download::new(data, number, session.limits.pdu_length)?;
            session.transfer(machine)
        })
    }
    /// Delete a block using the upstream _DELE PI service, without automatic replay.
    pub fn delete(&self, kind: BlockType, number: u16) -> Result<()> {
        self.observe("delete", || {
            let mut session = self.session()?;
            let job = wire::delete_request(0, kind, number);
            let result = session
                .transact(|reference| {
                    Ok(protocol::Pdu {
                        reference,
                        ..job.clone()
                    })
                })
                .and_then(|reply| session.validate(wire::check_ack(&reply, 0x28)));
            result.map_err(|error| crate::policy::mutation_failure(&job, error))
        })
    }
}

/// Never silently choose DB1 when an offline header is too short to identify a block.
pub(crate) fn download_number(data: &[u8], number: Option<u16>) -> Result<u16> {
    match number {
        Some(number) => Ok(number),
        None if data.len() >= 8 => Ok(u16::from_be_bytes([data[6], data[7]])),
        None => Err(Error::InvalidInput(
            "download requires a block number or complete number field",
        )),
    }
}
