//! Native asynchronous S7 client, enabled with the `async` Cargo feature.
//!
//! Requires a Tokio runtime with I/O and time enabled. Clones serialize complete
//! operations on one session. Cancelling an in-flight exchange closes its socket.

mod batch;
mod diagnostics;
mod force;
mod management;
mod memory;
mod security;
mod session;
mod transport;
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
}

impl AsyncClient {
    /// Connect and negotiate without using blocking socket calls.
    pub async fn connect(config: ClientConfig) -> Result<Self> {
        config.validate()?;
        let session = Session::connect(&config).await?;
        Ok(Self {
            inner: Arc::new(Mutex::new(session)),
            config,
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
        self.session().await.disconnect().await
    }
    /// Explicitly reconnect with fresh negotiation and no automatic operation replay.
    pub async fn reconnect(&self) -> Result<()> {
        let mut session = self.session().await;
        let _ = session.disconnect().await;
        *session = Session::connect(&self.config).await?;
        Ok(())
    }
    /// Exchange a custom S7 request using a fresh session-owned reference.
    pub async fn iso_exchange(&self, bytes: &[u8]) -> Result<Vec<u8>> {
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
    }
    /// Acquire one async operation guard without blocking an executor thread.
    async fn session(&self) -> MutexGuard<'_, Session> {
        self.inner.lock().await
    }
}
