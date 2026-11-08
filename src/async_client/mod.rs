//! Native asynchronous S7 client, enabled with the `async` Cargo feature.
//!
//! Requires a Tokio runtime with I/O and time enabled. Clones serialize complete
//! operations on one session. Cancelling an in-flight exchange closes its socket.

mod batch;
mod diagnostics;
mod force;
mod management;
mod memory;
mod pool;
mod security;
mod session;
mod transport;
pub use pool::AsyncClientPool;
mod typed;

use self::session::Session;
use crate::{
    ClientConfig, Error, Result,
    protocol::{Pdu, PduType},
};
use std::sync::Arc;
use tokio::sync::{Mutex, MutexGuard};

/// Native async client; clones share one session and independent clients run in parallel.
#[derive(Debug, Clone)]
pub struct AsyncClient {
    inner: Arc<Mutex<Session>>,
    config: ClientConfig,
    observer: Arc<crate::observability::Observer>,
}

impl AsyncClient {
    /// Connect and negotiate without using blocking socket calls.
    pub async fn connect(config: ClientConfig) -> Result<Self> {
        config.validate()?;
        let observer = Arc::new(crate::observability::Observer::new(
            config.transport.endpoint,
            config.operation_hook.clone(),
        ));
        let session = observer
            .run_async("connect", Session::connect(&config))
            .await?;
        Ok(Self {
            inner: Arc::new(Mutex::new(session)),
            config,
            observer,
        })
    }
    /// Report local readiness; this does not probe the remote PLC.
    pub async fn is_connected(&self) -> bool {
        self.session().await.is_connected()
    }
    /// Return negotiated PDU bytes or a disconnected-session error.
    pub async fn pdu_length(&self) -> Result<u16> {
        let session = self.session().await;
        if !session.is_connected() {
            return Err(Error::Disconnected);
        }
        Ok(session.limits.pdu_length)
    }
    /// Disconnect the shared socket; repeated calls are harmless.
    pub async fn disconnect(&self) -> Result<()> {
        self.observer
            .run_async("disconnect", async {
                self.observer
                    .run_async("disconnect", async {
                        self.session().await.disconnect().await
                    })
                    .await
            })
            .await
    }
    /// Explicitly reconnect with fresh negotiation and no automatic operation replay.
    pub async fn reconnect(&self) -> Result<()> {
        self.observer
            .run_async("reconnect", async {
                self.observer
                    .run_async("reconnect", async {
                        let mut session = self.session().await;
                        let _ = session.disconnect().await;
                        *session = Session::connect(&self.config).await?;
                        Ok(())
                    })
                    .await
            })
            .await
    }
    /// Exchange a custom S7 request using a fresh session-owned reference.
    pub async fn iso_exchange(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        self.observer
            .run_async("iso_exchange", async {
                self.observer
                    .run_async("iso_exchange", async {
                        let job = Pdu::decode(bytes)?;
                        if !matches!(job.kind, PduType::Job | PduType::UserData) {
                            return Err(Error::InvalidInput(
                                "raw exchange requires a job or user-data request",
                            ));
                        }
                        self.session()
                            .await
                            .transact(|reference| Ok(Pdu { reference, ..job }))
                            .await?
                            .encode()
                    })
                    .await
            })
            .await
    }
    /// Acquire one async operation guard without blocking an executor thread.
    async fn session(&self) -> MutexGuard<'_, Session> {
        self.inner.lock().await
    }
}

impl AsyncClient {
    /// Snapshot logical operation counts, last timing/error, and callback failures.
    pub fn operation_stats(&self) -> crate::observability::OperationStats {
        self.observer.stats()
    }
}
