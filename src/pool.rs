//! Bounded read parallelism and result ordering.

pub use crate::client::ClientPool;
use crate::{Address, Error, Result};

/// Assign original item indexes round-robin, leaving result order explicit.
pub(crate) fn assignments(addresses: &[Address], count: usize) -> Vec<Vec<(usize, Address)>> {
    let mut groups = vec![Vec::new(); count];
    for (index, address) in addresses.iter().enumerate() {
        groups[index % count].push((index, *address));
    }
    groups
}

/// Reassemble independent worker outputs while preserving each PLC item error.
pub(crate) fn restore(
    count: usize,
    groups: Vec<Vec<(usize, Result<Vec<u8>>)>>,
) -> Result<Vec<Result<Vec<u8>>>> {
    let mut results: Vec<Option<Result<Vec<u8>>>> = (0..count).map(|_| None).collect();
    for (index, result) in groups.into_iter().flatten() {
        let slot = results
            .get_mut(index)
            .ok_or(Error::Protocol("invalid pool result index"))?;
        if slot.is_some() {
            return Err(Error::Protocol("duplicate pool result index"));
        }
        *slot = Some(result);
    }
    results
        .into_iter()
        .map(|result| result.ok_or(Error::Protocol("missing pool result")))
        .collect()
}
