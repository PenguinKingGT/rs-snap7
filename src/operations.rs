//! Shared memory-operation planning and blocking execution.

use crate::{Address, Area, Error, Result, WordLen, protocol, session::Session};

/// Split a complete validated range into jobs that fit both PDU and wire fields.
pub(crate) fn chunks(
    area: Area,
    db: u16,
    start: u32,
    count: usize,
    word: WordLen,
    pdu: u16,
    writing: bool,
) -> Result<Vec<Address>> {
    let width = word.size();
    let step = if word.is_indexed() || word == WordLen::Bit {
        1
    } else {
        width
    };
    let end = u64::from(start)
        .checked_add(
            u64::try_from(count)
                .map_err(|_| Error::InvalidInput("count overflow"))?
                .checked_mul(step as u64)
                .ok_or(Error::InvalidInput("range overflow"))?,
        )
        .ok_or(Error::InvalidInput("range overflow"))?;
    let limit = if word.is_indexed() || word == WordLen::Bit {
        0x1000000
    } else {
        0x200000
    };
    if end > limit {
        return Err(Error::InvalidInput("transfer exceeds address space"));
    }
    // Validate area/DB compatibility even for an empty operation.
    Address::new(area, db, start, word, 1)?;
    let overhead = if writing { 28 } else { 18 };
    let budget = usize::from(pdu)
        .checked_sub(overhead)
        .ok_or(Error::InvalidInput("PDU too small"))?;
    let wire_limit =
        if matches!(word, WordLen::Bit | WordLen::Real | WordLen::Char) || word.is_indexed() {
            65535
        } else {
            8191
        };
    let per_job = (budget.min(wire_limit) / width).min(65535);
    if per_job == 0 {
        return Err(Error::InvalidInput("PDU cannot fit an element"));
    }
    let mut result = Vec::new();
    let mut done = 0;
    while done < count {
        let n = (count - done).min(per_job);
        result.push(Address::new(
            area,
            db,
            start + (done * step) as u32,
            word,
            n as u16,
        )?);
        done += n;
    }
    Ok(result)
}

impl Session {
    /// Execute a prevalidated raw read, keeping one exclusive session for all chunks.
    pub(crate) fn read_area(
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
        self.ensure_ready(deadline)?;
        if original_pdu != self.limits.pdu_length {
            plan = chunks(area, db, start, count, word, self.limits.pdu_length, false)?;
        }
        let mut pending: std::collections::VecDeque<_> = plan.into();
        let mut data = Vec::with_capacity(count * word.size());
        while let Some(address) = pending.pop_front() {
            let before = self.limits.pdu_length;
            let response = self.transact_until(
                |reference| protocol::read_request(reference, &[address]),
                deadline,
            );
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
    pub(crate) fn write_area(
        &mut self,
        area: Area,
        db: u16,
        start: u32,
        data: &[u8],
    ) -> Result<()> {
        self.ensure_ready(self.deadline())?;
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
    pub(crate) fn db_size(&mut self, db: u16) -> Result<usize> {
        let mut payload = vec![0x30, 0x41];
        payload.extend_from_slice(format!("{db:05}").as_bytes());
        payload.push(b'A');
        let response =
            self.transact(|reference| protocol::userdata_request(reference, 3, 3, Some(&payload)))?;
        let info = self.validate(protocol::parse_userdata(&response, 3, 3, false))?;
        if info.more
            || info.payload.len() != 78
            || u16::from_be_bytes([info.payload[12], info.payload[13]]) != db
            || info.payload[1] != 0x41
        {
            return self.validate(Err(Error::Protocol("invalid DB block-info response")));
        }
        let size = usize::from(u16::from_be_bytes([info.payload[40], info.payload[41]]));
        Ok(if size == 0 { 65536 } else { size })
    }
}

/// Preserve definite partial completion and ambiguous, unacknowledged exchanges separately.
pub(crate) fn write_failure(source: Error, confirmed: usize, uncertain: usize) -> Error {
    match source {
        Error::Io(_)
        | Error::Timeout
        | Error::Protocol(_)
        | Error::LostReference { .. }
        | Error::StaleReference { .. } => Error::AmbiguousWrite {
            confirmed,
            uncertain,
            source: Box::new(source),
        },
        _ if confirmed > 0 => Error::PartialWrite {
            confirmed,
            source: Box::new(source),
        },
        _ => source,
    }
}
