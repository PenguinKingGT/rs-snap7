//! Bounded read parallelism across explicitly independent client sessions.

use crate::{Address, Client, Error, Result};
use std::{sync::Arc, thread};

/// A read pool with one independent blocking connection per worker (maximum 32).
#[derive(Debug)]
pub struct ClientPool {
    clients: Vec<Client>,
}

impl ClientPool {
    /// Validate the worker bound and reject clones sharing the same session.
    pub fn new(clients: Vec<Client>) -> Result<Self> {
        if clients.is_empty() || clients.len() > 32 {
            return Err(Error::InvalidInput("pool requires 1 through 32 sessions"));
        }
        for (index, client) in clients.iter().enumerate() {
            if clients[..index]
                .iter()
                .any(|other| Arc::ptr_eq(&client.inner, &other.inner))
            {
                return Err(Error::InvalidInput("pool sessions must be independent"));
            }
        }
        Ok(Self { clients })
    }

    /// Read up to 20 variables in parallel, preserving input order and item errors.
    /// Transport errors fail the outer operation; independent read jobs may finish.
    pub fn read_multi_vars(&self, addresses: &[Address]) -> Result<Vec<Result<Vec<u8>>>> {
        if addresses.len() > crate::protocol::MAX_ITEMS {
            return Err(Error::InvalidInput("at most 20 variables per batch call"));
        }
        let groups = assignments(addresses, self.clients.len());
        let outcomes = thread::scope(|scope| {
            let workers: Vec<_> = groups
                .into_iter()
                .zip(&self.clients)
                .filter(|(items, _)| !items.is_empty())
                .map(|(items, client)| {
                    scope.spawn(move || {
                        let addresses: Vec<_> = items.iter().map(|(_, a)| *a).collect();
                        client.read_multi_vars(&addresses).map(|results| {
                            items
                                .into_iter()
                                .map(|(i, _)| i)
                                .zip(results)
                                .collect::<Vec<_>>()
                        })
                    })
                })
                .collect();
            workers
                .into_iter()
                .map(|worker| {
                    worker
                        .join()
                        .map_err(|_| Error::Protocol("read worker panicked"))?
                })
                .collect::<Result<Vec<_>>>()
        })?;
        restore(addresses.len(), outcomes)
    }
}

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
