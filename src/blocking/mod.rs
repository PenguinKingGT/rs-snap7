//! Optional synchronous adapters over the native asynchronous S7 execution core.
//!
//! Calls share one continuously running Tokio runtime and must be made outside
//! an entered Tokio runtime. Nested use returns BlockingInAsyncContext before I/O.
//! Client clones share the same session. Final drop releases session tasks/sockets;
//! the bounded runtime and optional background-hook dispatcher live until process exit.

pub(crate) mod hooks;
mod methods;
mod runtime;

use crate::{AsyncClient, ClientConfig, Result};
use std::sync::Arc;

/// Synchronous S7 client; every network operation delegates to the async core.
#[derive(Debug, Clone)]
pub struct Client {
    pub(crate) inner: AsyncClient,
    delivery: Option<Arc<hooks::Delivery>>,
}
impl Client {
    /// Validate and connect on the shared runtime without creating a per-client thread.
    pub fn connect(mut config: ClientConfig) -> Result<Self> {
        runtime::check_context()?;
        config.validate()?;
        let delivery = if config.operation_hook.enabled() || config.connection_hook.enabled() {
            let delivery = hooks::Delivery::new()?;
            config.operation_hook = config.operation_hook.deferred(delivery.clone());
            config.connection_hook = config.connection_hook.deferred(delivery.clone());
            Some(delivery)
        } else {
            None
        };
        let inner = runtime::run(AsyncClient::connect(config))?;
        Ok(Self { inner, delivery })
    }
    /// Inspect local readiness; nested runtime use returns an error rather than panicking.
    pub fn is_connected(&self) -> Result<bool> {
        runtime::run(async { Ok(self.inner.is_connected().await) })
    }
    /// Snapshot core operation accounting and deferred background-event losses.
    pub fn operation_stats(&self) -> crate::observability::OperationStats {
        let mut stats = self.inner.operation_stats();
        stats.background_hook_events_dropped = self
            .delivery
            .as_ref()
            .map_or(0, |delivery| delivery.dropped());
        stats
    }
}

/// Synchronous pool over the same bounded concurrent async read planner.
#[derive(Debug)]
pub struct ClientPool {
    inner: crate::AsyncClientPool,
    _clients: Vec<Client>,
}
impl ClientPool {
    /// Require independent sessions; core validation rejects clones of one connection.
    pub fn new(clients: Vec<Client>) -> Result<Self> {
        let inner = crate::AsyncClientPool::new(
            clients.iter().map(|client| client.inner.clone()).collect(),
        )?;
        Ok(Self {
            inner,
            _clients: clients,
        })
    }
    /// Run independent reads concurrently, preserving input order and per-item errors.
    pub fn read_multi_vars(&self, addresses: &[crate::Address]) -> Result<Vec<Result<Vec<u8>>>> {
        runtime::run(self.inner.read_multi_vars(addresses))
    }
}
