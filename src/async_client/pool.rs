//! Bounded native async parallel reads across independent sessions.

use super::AsyncClient;
use crate::{
    Address, Error, Result,
    pool::{assignments, restore},
};
use std::sync::Arc;
use tokio::task::JoinSet;

/// Async read pool with at most 32 independent S7 sessions.
#[derive(Debug)]
pub struct AsyncClientPool {
    clients: Vec<AsyncClient>,
}

impl AsyncClientPool {
    /// Validate the worker count and reject clones of a single shared connection.
    pub fn new(clients: Vec<AsyncClient>) -> Result<Self> {
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

    /// Read up to 20 variables concurrently. Cancelling the pool future aborts
    /// active tasks, invalidating sockets whose exchanges were already transmitted.
    pub async fn read_multi_vars(&self, addresses: &[Address]) -> Result<Vec<Result<Vec<u8>>>> {
        if addresses.len() > crate::protocol::MAX_ITEMS {
            return Err(Error::InvalidInput("at most 20 variables per batch call"));
        }
        let mut jobs = JoinSet::new();
        for (items, client) in assignments(addresses, self.clients.len())
            .into_iter()
            .zip(&self.clients)
        {
            if items.is_empty() {
                continue;
            }
            let client = client.clone();
            let job = async move {
                let addresses: Vec<_> = items.iter().map(|(_, a)| *a).collect();
                client.read_multi_vars(&addresses).await.map(|results| {
                    items
                        .into_iter()
                        .map(|(i, _)| i)
                        .zip(results)
                        .collect::<Vec<_>>()
                })
            };
            #[cfg(feature = "blocking")]
            let job = crate::blocking::hooks::inherit(job);
            jobs.spawn(job);
        }
        let mut outcomes = Vec::new();
        while let Some(result) = jobs.join_next().await {
            outcomes.push(result.map_err(|_| Error::Protocol("async read worker failed"))??);
        }
        restore(addresses.len(), outcomes)
    }
}
