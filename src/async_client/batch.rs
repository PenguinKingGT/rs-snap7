//! Batched variable reads and writes with per-item result preservation.

use super::AsyncClient;
use crate::{Address, Error, Result, operations::write_failure, planner, protocol};

impl AsyncClient {
    /// Read up to 20 variables, splitting large items and packets as required.
    /// Results follow input order; PLC item failures do not discard other items.
    /// A transport or malformed-message error fails the outer operation.
    pub async fn read_multi_vars(&self, addresses: &[Address]) -> Result<Vec<Result<Vec<u8>>>> {
        self.observe("read_multi_vars", async {
            let mut session = self.session().await;
            let deadline = session.deadline();
            session.ensure_ready(deadline).await?;
            'planning: loop {
                let plan = session.read_plan(addresses)?;
                let mut results: Vec<Result<Vec<u8>>> =
                    (0..plan.block_count()).map(|_| Ok(Vec::new())).collect();
                for pieces in &plan.packets {
                    let packet: Vec<_> = pieces.iter().map(|piece| piece.address).collect();
                    let before = session.limits.pdu_length;
                    let response = session
                        .transact(|reference| protocol::read_request(reference, &packet))
                        .await;
                    if matches!(response, Err(Error::InvalidInput(_)))
                        && before != session.limits.pdu_length
                    {
                        continue 'planning;
                    }
                    let response = response?;
                    let outcomes = session.validate(protocol::parse_read(&response, &packet))?;
                    for (piece, outcome) in pieces.iter().zip(outcomes) {
                        match (&mut results[piece.index], outcome) {
                            (Ok(data), Ok(fragment)) => data.extend(fragment),
                            (slot @ Ok(_), Err(error)) => *slot = Err(error),
                            _ => {}
                        }
                    }
                }
                let fallback = plan.fallback_items(&results);
                let mut results = plan.extract(results)?;
                for (index, address) in fallback {
                    match session
                        .read_area(
                            address.area(),
                            address.db_number(),
                            address.start(),
                            usize::from(address.count()),
                        )
                        .await
                    {
                        Ok(data) => results[index] = Ok(data),
                        Err(error @ Error::Item(_)) => results[index] = Err(error),
                        Err(error) => return Err(error),
                    }
                }
                return Ok(results);
            }
        })
        .await
    }

    /// Write up to 20 variables. Every payload is checked before the first exchange.
    /// Results preserve PLC item errors. An outer communication error reports total
    /// acknowledged bytes and the uncertain packet size; positions may be noncontiguous.
    pub async fn write_multi_vars(&self, items: &[(Address, &[u8])]) -> Result<Vec<Result<()>>> {
        self.observe("write_multi_vars", async {
            for (address, bytes) in items {
                if bytes.len() != address.byte_len() {
                    return Err(Error::InvalidInput("write payload width mismatch"));
                }
                if address.word_len() == crate::WordLen::Bit && bytes.iter().any(|b| *b > 1) {
                    return Err(Error::InvalidInput("invalid bit payload"));
                }
            }
            let mut session = self.session().await;
            let deadline = session.deadline();
            session.ensure_ready(deadline).await?;
            let addresses: Vec<_> = items.iter().map(|(address, _)| *address).collect();
            let plan = planner::batches(&addresses, session.limits.pdu_length, true)?;
            let mut results: Vec<Result<()>> = items.iter().map(|_| Ok(())).collect();
            let mut confirmed = 0;
            for pieces in plan {
                let packet: Vec<_> = pieces
                    .iter()
                    .map(|piece| {
                        (
                            piece.address,
                            &items[piece.index].1
                                [piece.offset..piece.offset + piece.address.byte_len()],
                        )
                    })
                    .collect();
                let size = pieces.iter().map(|p| p.address.byte_len()).sum();
                let outcomes = session
                    .transact(|reference| protocol::write_request(reference, &packet))
                    .await
                    .and_then(|response| {
                        session.validate(protocol::parse_write(&response, pieces.len()))
                    })
                    .map_err(|error| write_failure(error, confirmed, size))?;
                for (piece, outcome) in pieces.into_iter().zip(outcomes) {
                    match outcome {
                        Ok(()) => confirmed += piece.address.byte_len(),
                        Err(error) if results[piece.index].is_ok() => {
                            results[piece.index] = Err(error)
                        }
                        Err(_) => {}
                    }
                }
            }
            Ok(results)
        })
        .await
    }
}
