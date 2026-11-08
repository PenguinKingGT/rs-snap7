//! Async transaction state with cancellation-safe socket ownership.

use super::transport::AsyncConnection;
use crate::{
    ClientConfig, Error, Result,
    protocol::{self, Negotiated, Pdu},
};
use std::time::Duration;
use tokio::time::Instant;

/// Exclusive session state shared by async client clones.
#[derive(Debug)]
pub(super) struct Session {
    connection: Option<AsyncConnection>,
    pub(super) limits: Negotiated,
    reference: u16,
    timeout: Duration,
}

impl Session {
    /// Complete all negotiations before exposing a usable async client.
    pub(super) async fn connect(config: &ClientConfig) -> Result<Self> {
        let deadline = Instant::now() + config.transport.timeout;
        let connection = AsyncConnection::connect(&config.transport, deadline).await?;
        let mut session = Self {
            connection: Some(connection),
            limits: Negotiated {
                caller_jobs: 1,
                callee_jobs: 1,
                pdu_length: config.requested_pdu,
            },
            reference: 0,
            timeout: config.transport.timeout,
        };
        let reply = session
            .transact_until(
                |reference| protocol::setup_request(reference, config.requested_pdu),
                deadline,
            )
            .await?;
        session.limits = protocol::parse_setup(&reply, config.requested_pdu)?;
        Ok(session)
    }
    /// Whether no prior I/O failure or cancelled exchange invalidated the socket.
    pub(super) fn is_connected(&self) -> bool {
        self.connection.is_some()
    }
    /// Total deadline shared by all steps of a logical operation.
    pub(super) fn deadline(&self) -> Instant {
        Instant::now() + self.timeout
    }
    /// Drop socket ownership before awaiting, so cancellation cannot leave a live session.
    pub(super) async fn disconnect(&mut self) -> Result<()> {
        let deadline = self.deadline();
        if let Some(connection) = self.connection.take() {
            connection.disconnect(deadline).await?;
        }
        Ok(())
    }
    /// Execute one job with a new deadline.
    pub(super) async fn transact(&mut self, build: impl FnOnce(u16) -> Result<Pdu>) -> Result<Pdu> {
        self.transact_until(build, self.deadline()).await
    }
    /// Own the connection locally across awaits. If this future is dropped after
    /// transmission starts, the descriptor closes and the session remains disconnected.
    pub(super) async fn transact_until(
        &mut self,
        build: impl FnOnce(u16) -> Result<Pdu>,
        deadline: Instant,
    ) -> Result<Pdu> {
        if !self.is_connected() {
            return Err(Error::Disconnected);
        }
        if Instant::now() >= deadline {
            return Err(Error::Timeout);
        }
        let next = self.reference.wrapping_add(1);
        let bytes = build(next)?.encode()?;
        if bytes.len() > usize::from(self.limits.pdu_length) {
            return Err(Error::InvalidInput("request exceeds negotiated PDU"));
        }
        self.reference = next;
        let mut connection = self.connection.take().ok_or(Error::Disconnected)?;
        let result = async {
            connection.send(&bytes, deadline).await?;
            for _ in 0..=3 {
                let bytes = connection.receive(deadline).await?;
                if bytes.len() > usize::from(self.limits.pdu_length) {
                    return Err(Error::Protocol("response exceeds negotiated PDU"));
                }
                let reply = Pdu::decode(&bytes)?;
                match protocol::validate_reference(next, reply.reference) {
                    Err(Error::StaleReference { .. }) => continue,
                    other => other?,
                }
                reply.check_response()?;
                return Ok(reply);
            }
            Err(Error::Protocol("stale response retry limit reached"))
        }
        .await;
        if result.is_ok() || matches!(result, Err(Error::Plc(_) | Error::Authentication(_))) {
            self.connection = Some(connection);
        }
        result
    }
    /// A malformed service reply also invalidates its completed protocol stream.
    pub(super) fn validate<T>(&mut self, result: Result<T>) -> Result<T> {
        if matches!(result, Err(Error::Protocol(_))) {
            self.connection = None;
        }
        result
    }
}
