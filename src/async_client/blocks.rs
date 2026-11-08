//! Block catalog, upload/download, deletion, and offline inspection entry points.

use super::AsyncClient;
use crate::blocks::download_number;
use crate::{
    BlockType, Result,
    blocks::{
        self, BlockEntry, BlockInfo, BlocksList,
        transfer::{Catalog, Download, Upload},
    },
    protocol::{self, blocks as wire},
};

impl AsyncClient {
    /// Read all seven catalog block counts, validating the complete table.
    pub async fn list_blocks(&self) -> Result<BlocksList> {
        self.observe("list_blocks", async {
            let mut session = self.session().await;
            let data = session
                .transfer(Catalog::new(protocol::userdata_request(0, 3, 1, None)?, 1))
                .await?;
            session.validate(BlocksList::decode(&data))
        })
        .await
    }
    /// Read all entries of one type, including language/flags and fragmented responses.
    pub async fn list_block_entries(&self, kind: BlockType) -> Result<Vec<BlockEntry>> {
        self.observe("list_block_entries", async {
            let mut session = self.session().await;
            let data = session
                .transfer(Catalog::new(
                    protocol::userdata_request(0, 3, 2, Some(&[0x30, kind as u8]))?,
                    2,
                ))
                .await?;
            session.validate(blocks::decode_entries(&data))
        })
        .await
    }
    /// Return at most `limit` block numbers. A zero limit performs no wire request.
    pub async fn list_blocks_of_type(&self, kind: BlockType, limit: usize) -> Result<Vec<u16>> {
        self.observe("list_blocks_of_type", async {
            if limit == 0 {
                return Ok(Vec::new());
            }
            Ok(self
                .list_block_entries(kind)
                .await?
                .into_iter()
                .take(limit)
                .map(|entry| entry.number)
                .collect())
        })
        .await
    }
    /// Read canonical remote metadata and verify its type and block number.
    pub async fn get_block_info(&self, kind: BlockType, number: u16) -> Result<BlockInfo> {
        self.observe("get_block_info", async {
            self.session().await.block_info(kind, number).await
        })
        .await
    }
    /// Upload a DB's data body, omitting its initial 36-byte compact header.
    pub async fn upload(&self, number: u16) -> Result<Vec<u8>> {
        self.observe("upload", async {
            self.session()
                .await
                .transfer(Upload::new(BlockType::Db, number, false))
                .await
        })
        .await
    }
    /// Upload any block type with its complete header/body/footer representation.
    /// The returned Vec's length replaces Python's redundant `(buffer, size)` tuple.
    pub async fn full_upload(&self, kind: BlockType, number: u16) -> Result<Vec<u8>> {
        self.observe("full_upload", async {
            self.session()
                .await
                .transfer(Upload::new(kind, number, true))
                .await
        })
        .await
    }
    /// Download a DB representation using PLC-driven slice jobs. `None` extracts
    /// its number from bytes 6–7; short input requires an explicit number.
    /// Interrupted downloads have an uncertain remote outcome and never replay.
    pub async fn download(&self, data: &[u8], number: Option<u16>) -> Result<()> {
        self.observe("download", async {
            let number = download_number(data, number)?;
            wire::download_request(0, number, data)?;
            let mut session = self.session().await;
            let deadline = session.deadline();
            session.ensure_ready(deadline).await?;
            let machine = Download::new(data, number, session.limits.pdu_length)?;
            session.transfer(machine).await
        })
        .await
    }
    /// Delete a block using the upstream _DELE PI service, without automatic replay.
    pub async fn delete(&self, kind: BlockType, number: u16) -> Result<()> {
        self.observe("delete", async {
            let mut session = self.session().await;
            let job = wire::delete_request(0, kind, number);
            let result = session
                .transact(|reference| {
                    Ok(protocol::Pdu {
                        reference,
                        ..job.clone()
                    })
                })
                .await
                .and_then(|reply| session.validate(wire::check_ack(&reply, 0x28)));
            result.map_err(|error| crate::policy::mutation_failure(&job, error))
        })
        .await
    }
}
