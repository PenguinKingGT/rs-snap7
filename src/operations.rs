//! Shared memory-operation planning and write outcome classification.

use crate::{Address, Area, Error, Result, WordLen};

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
