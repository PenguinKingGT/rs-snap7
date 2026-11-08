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
