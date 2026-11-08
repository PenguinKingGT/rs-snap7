//! Packet budgeting shared by batch operation executors.

use crate::{Address, Error, Result, operations::chunks, protocol::MAX_ITEMS};

/// One original variable or a bounded fragment of it.
#[derive(Debug)]
pub(crate) struct Piece {
    /// Original caller item index, preserving result order.
    pub(crate) index: usize,
    /// Offset into the original item's byte payload.
    pub(crate) offset: usize,
    /// Validated fragment address.
    pub(crate) address: Address,
}

/// Pack fragments under both request and response PDU budgets.
pub(crate) fn batches(addresses: &[Address], pdu: u16, writing: bool) -> Result<Vec<Vec<Piece>>> {
    if addresses.len() > MAX_ITEMS {
        return Err(Error::InvalidInput("at most 20 variables per batch call"));
    }
    let mut batches = Vec::new();
    let mut current = Vec::new();
    let (mut request_size, mut response_size) = (12, 14);
    let mut previous = 0usize;
    for (index, address) in addresses.iter().enumerate() {
        let mut offset = 0;
        for fragment in chunks(
            address.area(),
            address.db_number(),
            address.start(),
            usize::from(address.count()),
            address.word_len(),
            pdu,
            writing,
        )? {
            let size = fragment.byte_len();
            let padding = previous % 2;
            let request_cost = 12 + if writing { 4 + size + padding } else { 0 };
            let response_cost = if writing { 1 } else { 4 + size + padding };
            if current.len() == MAX_ITEMS
                || request_size + request_cost > usize::from(pdu)
                || response_size + response_cost > usize::from(pdu)
            {
                if current.is_empty() {
                    return Err(Error::InvalidInput("variable cannot fit negotiated PDU"));
                }
                batches.push(std::mem::take(&mut current));
                request_size = 12;
                response_size = 14;
                previous = 0;
            }
            request_size += 12 + if writing { 4 + size + previous % 2 } else { 0 };
            response_size += if writing { 1 } else { 4 + size + previous % 2 };
            current.push(Piece {
                index,
                offset,
                address: fragment,
            });
            offset += size;
            previous = size;
        }
    }
    if !current.is_empty() {
        batches.push(current);
    }
    Ok(batches)
}

#[cfg(test)]
mod tests {
    //! Packet budgets are verified independently for odd and even item widths.
    use super::*;

    /// All jobs fit both directions; addresses retain their original byte offsets.
    #[test]
    fn split_packet_budgets_and_mapping() {
        let items = [
            Address::db(1, 0, 301).unwrap(),
            Address::db(1, 1000, 3).unwrap(),
        ];
        for writing in [false, true] {
            let plans = batches(&items, 128, writing).unwrap();
            let mut bytes = [0; 2];
            for packet in plans {
                let mut addresses = Vec::new();
                let mut values = Vec::new();
                for piece in packet {
                    assert_eq!(piece.offset, bytes[piece.index]);
                    bytes[piece.index] += piece.address.byte_len();
                    values.push(vec![0; piece.address.byte_len()]);
                    addresses.push(piece.address);
                }
                let size = if writing {
                    let borrowed: Vec<_> = addresses
                        .iter()
                        .copied()
                        .zip(values.iter().map(Vec::as_slice))
                        .collect();
                    crate::protocol::write_request(1, &borrowed)
                        .unwrap()
                        .encode()
                        .unwrap()
                        .len()
                } else {
                    crate::protocol::read_request(1, &addresses)
                        .unwrap()
                        .encode()
                        .unwrap()
                        .len()
                };
                assert!(size <= 128);
            }
            assert_eq!(bytes, [301, 3]);
        }
        let counter =
            Address::new(crate::Area::Counters, 0, 2, crate::WordLen::Counter, 100).unwrap();
        let plans = batches(&[counter], 128, false).unwrap();
        assert_eq!(plans[1][0].address.start(), 57);
        assert_eq!(plans[1][0].offset, 110);
    }
}

/// A merged byte read with mappings back to caller variables.
#[derive(Debug)]
struct ReadBlock {
    address: Address,
    mappings: Vec<(usize, usize, usize)>,
}

/// Reusable read plan cached by original layout and negotiated PDU length.
#[derive(Debug)]
pub(crate) struct ReadPlan {
    inputs: Vec<Address>,
    pdu: u16,
    blocks: Vec<ReadBlock>,
    pub(crate) packets: Vec<Vec<Piece>>,
}

impl ReadPlan {
    /// Sort and merge adjacent/overlapping byte reads without reading unrequested gaps.
    pub(crate) fn new(inputs: &[Address], pdu: u16) -> Result<Self> {
        if inputs.len() > MAX_ITEMS {
            return Err(Error::InvalidInput("at most 20 variables per batch call"));
        }
        let mut sorted: Vec<_> = inputs.iter().copied().enumerate().collect();
        sorted.sort_by_key(|(_, a)| (a.area() as u8, a.db_number(), a.word_len() as u8, a.start()));
        let mut blocks: Vec<ReadBlock> = Vec::new();
        for (index, address) in sorted {
            if let Some(previous) = blocks.last_mut()
                && address.word_len() == crate::WordLen::Byte
                && previous.address.word_len() == crate::WordLen::Byte
                && address.area() == previous.address.area()
                && address.db_number() == previous.address.db_number()
            {
                let start = previous.address.start();
                let previous_end = start + u32::from(previous.address.count());
                let end = (address.start() + u32::from(address.count())).max(previous_end);
                if address.start() <= previous_end && end - start <= 65535 {
                    previous.mappings.push((
                        index,
                        (address.start() - start) as usize,
                        address.byte_len(),
                    ));
                    previous.address = Address::new(
                        address.area(),
                        address.db_number(),
                        start,
                        crate::WordLen::Byte,
                        (end - start) as u16,
                    )?;
                    continue;
                }
            }
            blocks.push(ReadBlock {
                address,
                mappings: vec![(index, 0, address.byte_len())],
            });
        }
        let addresses: Vec<_> = blocks.iter().map(|b| b.address).collect();
        let packets = batches(&addresses, pdu, false)?;
        Ok(Self {
            inputs: inputs.to_vec(),
            pdu,
            blocks,
            packets,
        })
    }

    /// Whether a cached plan is valid for this layout and current negotiated limits.
    pub(crate) fn matches(&self, inputs: &[Address], pdu: u16) -> bool {
        self.inputs == inputs && self.pdu == pdu
    }
    /// Number of merged response buffers to assemble before extraction.
    pub(crate) fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Retry individual items after a merged PLC rejection to preserve independent
    /// item errors when a valid region happens to border an inaccessible region.
    pub(crate) fn fallback_items(&self, outcomes: &[Result<Vec<u8>>]) -> Vec<(usize, Address)> {
        self.blocks
            .iter()
            .zip(outcomes)
            .filter(|(block, result)| {
                block.mappings.len() > 1 && matches!(result, Err(Error::Item(_)))
            })
            .flat_map(|(block, _)| {
                block
                    .mappings
                    .iter()
                    .map(|(index, _, _)| (*index, self.inputs[*index]))
            })
            .collect()
    }

    /// Restore caller order and subranges; merged PLC errors apply to every covered item.
    pub(crate) fn extract(&self, blocks: Vec<Result<Vec<u8>>>) -> Result<Vec<Result<Vec<u8>>>> {
        if blocks.len() != self.blocks.len() {
            return Err(Error::Protocol("read-plan result count mismatch"));
        }
        let mut result: Vec<Result<Vec<u8>>> = self.inputs.iter().map(|_| Ok(Vec::new())).collect();
        for (block, outcome) in self.blocks.iter().zip(blocks) {
            match outcome {
                Ok(bytes) => {
                    if bytes.len() != block.address.byte_len() {
                        return Err(Error::Protocol("read-plan payload size mismatch"));
                    }
                    for &(index, offset, length) in &block.mappings {
                        result[index] = Ok(bytes[offset..offset + length].to_vec());
                    }
                }
                Err(Error::Item(code)) => {
                    for &(index, _, _) in &block.mappings {
                        result[index] = Err(Error::Item(code));
                    }
                }
                Err(error) => return Err(error),
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod read_plan_tests {
    //! Sort/merge/extraction and cache-key compatibility tests.
    use super::*;

    /// Overlap, adjacency, DB separation, and duplicate reads preserve caller ordering.
    #[test]
    fn merging_and_mapping_preserve_order_and_errors() {
        let inputs = [
            Address::db(1, 2, 3).unwrap(),
            Address::db(1, 0, 3).unwrap(),
            Address::db(2, 0, 1).unwrap(),
            Address::db(1, 0, 3).unwrap(),
        ];
        let plan = ReadPlan::new(&inputs, 128).unwrap();
        assert_eq!(plan.block_count(), 2);
        assert!(plan.matches(&inputs, 128));
        assert!(!plan.matches(&inputs, 240));
        let results = plan
            .extract(vec![Ok(vec![0, 1, 2, 3, 4]), Err(Error::Item(5))])
            .unwrap();
        assert_eq!(results[0].as_ref().unwrap(), &[2, 3, 4]);
        assert_eq!(results[1].as_ref().unwrap(), &[0, 1, 2]);
        assert!(matches!(results[2], Err(Error::Item(5))));
        assert_eq!(results[3].as_ref().unwrap(), &[0, 1, 2]);
    }

    /// Gaps and indexed areas are not widened into unsafe speculative reads.
    #[test]
    fn gaps_and_typed_items_remain_separate() {
        let inputs = [
            Address::db(1, 0, 1).unwrap(),
            Address::db(1, 2, 1).unwrap(),
            Address::new(crate::Area::Counters, 0, 0, crate::WordLen::Counter, 2).unwrap(),
        ];
        let plan = ReadPlan::new(&inputs, 128).unwrap();
        assert_eq!(plan.block_count(), 3);
        assert!(plan.extract(vec![]).is_err());
        assert!(
            plan.extract(vec![Ok(vec![]), Ok(vec![0]), Ok(vec![0; 4])])
                .is_err()
        );
    }
}
