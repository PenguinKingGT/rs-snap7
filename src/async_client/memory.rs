//! Async memory execution over shared chunk and protocol planners.

use super::{AsyncClient, session::Session};
use crate::{
    Area, Error, Result,
    operations::{chunks, write_failure},
    protocol,
};

impl Session {
    /// Execute a prevalidated raw read, keeping one exclusive session for all chunks.
    pub(super) async fn read_area(
        &mut self,
        area: Area,
        db: u16,
        start: u32,
        count: usize,
    ) -> Result<Vec<u8>> {
        let deadline = self.deadline();
        let word = area.word_len();
        let mut plan = chunks(area, db, start, count, word, self.limits.pdu_length, false)?;
        let original_pdu = self.limits.pdu_length;
        self.ensure_ready(deadline).await?;
        if original_pdu != self.limits.pdu_length {
            plan = chunks(area, db, start, count, word, self.limits.pdu_length, false)?;
        }
        let mut pending: std::collections::VecDeque<_> = plan.into();
        let mut data = Vec::with_capacity(count * word.size());
        while let Some(address) = pending.pop_front() {
            let before = self.limits.pdu_length;
            let response = self
                .transact_until(
                    |reference| protocol::read_request(reference, &[address]),
                    deadline,
                )
                .await;
            if matches!(response, Err(Error::InvalidInput(_))) && before != self.limits.pdu_length {
                let smaller = chunks(
                    area,
                    db,
                    address.start(),
                    usize::from(address.count()),
                    word,
                    self.limits.pdu_length,
                    false,
                )?;
                for piece in smaller.into_iter().rev() {
                    pending.push_front(piece);
                }
                continue;
            }
            let mut outcomes = self.validate(protocol::parse_read(&response?, &[address]))?;
            data.extend(outcomes.remove(0)?);
        }
        Ok(data)
    }

    /// Execute writes without replaying any mutating exchange on communication loss.
    pub(super) async fn write_area(
        &mut self,
        area: Area,
        db: u16,
        start: u32,
        data: &[u8],
    ) -> Result<()> {
        self.ensure_ready(self.deadline()).await?;
        let word = area.word_len();
        if !data.len().is_multiple_of(word.size()) {
            return Err(Error::InvalidInput("write must contain whole elements"));
        }
        let plan = chunks(
            area,
            db,
            start,
            data.len() / word.size(),
            word,
            self.limits.pdu_length,
            true,
        )?;
        let mut confirmed = 0;
        for address in plan {
            let size = address.byte_len();
            let result = self
                .transact(|reference| {
                    protocol::write_request(
                        reference,
                        &[(address, &data[confirmed..confirmed + size])],
                    )
                })
                .await
                .and_then(|response| self.validate(protocol::parse_write(&response, 1)))
                .and_then(|mut outcomes| outcomes.remove(0));
            if let Err(source) = result {
                return Err(write_failure(source, confirmed, size));
            }
            confirmed += size;
        }
        Ok(())
    }

    /// Obtain a DB's MC7 size from the upstream 78-byte block-info layout.
    pub(super) async fn db_size(&mut self, db: u16) -> Result<usize> {
        let size = usize::from(self.block_info(crate::BlockType::Db, db).await?.mc7_size);
        Ok(if size == 0 { 65536 } else { size })
    }
}

impl AsyncClient {
    /// Read raw elements from an area. Timer/counter offsets and counts use elements;
    /// other areas use bytes. Large transfers are split using negotiated PDU limits.
    pub async fn read_area(
        &self,
        area: crate::Area,
        db: u16,
        start: u32,
        count: usize,
    ) -> Result<Vec<u8>> {
        self.observe("read_area", async {
            self.session().await.read_area(area, db, start, count).await
        })
        .await
    }

    /// Write raw area data. Timer/counter payloads require complete 16-bit elements.
    /// Communication failures report uncertain writes; writes are never replayed.
    pub async fn write_area(
        &self,
        area: crate::Area,
        db: u16,
        start: u32,
        data: &[u8],
    ) -> Result<()> {
        self.observe("write_area", async {
            self.session().await.write_area(area, db, start, data).await
        })
        .await
    }

    /// Read bytes from a data block.
    pub async fn db_read(&self, db: u16, start: u32, size: usize) -> Result<Vec<u8>> {
        self.observe("db_read", async {
            self.read_area(crate::Area::Db, db, start, size).await
        })
        .await
    }

    /// Write bytes to a data block; acknowledged/uncertain progress is retained in errors.
    pub async fn db_write(&self, db: u16, start: u32, data: &[u8]) -> Result<()> {
        self.observe("db_write", async {
            self.write_area(crate::Area::Db, db, start, data).await
        })
        .await
    }

    /// Read an entire DB. Supply its size for PLCs with unreliable block metadata;
    /// `None` queries MC7 size, falling back to 65536 only for a reported zero size.
    pub async fn db_get(&self, db: u16, size: Option<usize>) -> Result<Vec<u8>> {
        self.observe("db_get", async {
            let mut session = self.session().await;
            let size = match size {
                Some(size) => size,
                None => session.db_size(db).await?,
            };
            session.read_area(crate::Area::Db, db, 0, size).await
        })
        .await
    }

    /// Fill a whole DB with one byte; `None` determines its size from block metadata.
    pub async fn db_fill(&self, db: u16, filler: u8, size: Option<usize>) -> Result<()> {
        self.observe("db_fill", async {
            let mut session = self.session().await;
            let size = match size {
                Some(size) => size,
                None => session.db_size(db).await?,
            };
            if size > 0x20_0000 {
                return Err(Error::InvalidInput("DB exceeds address space"));
            }
            session
                .write_area(crate::Area::Db, db, 0, &vec![filler; size])
                .await
        })
        .await
    }
}

impl AsyncClient {
    /// Read outputs; counts use the area's natural element units.
    pub async fn ab_read(&self, start: u32, count: usize) -> Result<Vec<u8>> {
        self.observe("ab_read", async {
            self.read_area(crate::Area::Outputs, 0, start, count).await
        })
        .await
    }
    /// Write outputs; payload bytes must contain complete elements.
    pub async fn ab_write(&self, start: u32, data: &[u8]) -> Result<()> {
        self.observe("ab_write", async {
            self.write_area(crate::Area::Outputs, 0, start, data).await
        })
        .await
    }
}

impl AsyncClient {
    /// Read inputs; counts use the area's natural element units.
    pub async fn eb_read(&self, start: u32, count: usize) -> Result<Vec<u8>> {
        self.observe("eb_read", async {
            self.read_area(crate::Area::Inputs, 0, start, count).await
        })
        .await
    }
    /// Write inputs; payload bytes must contain complete elements.
    pub async fn eb_write(&self, start: u32, data: &[u8]) -> Result<()> {
        self.observe("eb_write", async {
            self.write_area(crate::Area::Inputs, 0, start, data).await
        })
        .await
    }
}

impl AsyncClient {
    /// Read markers; counts use the area's natural element units.
    pub async fn mb_read(&self, start: u32, count: usize) -> Result<Vec<u8>> {
        self.observe("mb_read", async {
            self.read_area(crate::Area::Markers, 0, start, count).await
        })
        .await
    }
    /// Write markers; payload bytes must contain complete elements.
    pub async fn mb_write(&self, start: u32, data: &[u8]) -> Result<()> {
        self.observe("mb_write", async {
            self.write_area(crate::Area::Markers, 0, start, data).await
        })
        .await
    }
}

impl AsyncClient {
    /// Read timer elements; counts use the area's natural element units.
    pub async fn tm_read(&self, start: u32, count: usize) -> Result<Vec<u8>> {
        self.observe("tm_read", async {
            self.read_area(crate::Area::Timers, 0, start, count).await
        })
        .await
    }
    /// Write timer elements; payload bytes must contain complete elements.
    pub async fn tm_write(&self, start: u32, data: &[u8]) -> Result<()> {
        self.observe("tm_write", async {
            self.write_area(crate::Area::Timers, 0, start, data).await
        })
        .await
    }
}

impl AsyncClient {
    /// Read counter elements; counts use the area's natural element units.
    pub async fn ct_read(&self, start: u32, count: usize) -> Result<Vec<u8>> {
        self.observe("ct_read", async {
            self.read_area(crate::Area::Counters, 0, start, count).await
        })
        .await
    }
    /// Write counter elements; payload bytes must contain complete elements.
    pub async fn ct_write(&self, start: u32, data: &[u8]) -> Result<()> {
        self.observe("ct_write", async {
            self.write_area(crate::Area::Counters, 0, start, data).await
        })
        .await
    }
}
