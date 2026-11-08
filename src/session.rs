//! Exclusive session state and correlated blocking transaction execution.

use crate::client::ClientConfig;
use crate::protocol::{self, Negotiated, Pdu};
use crate::transport::Connection;
use crate::{Error, Result};
use std::time::{Duration, Instant};

/// A transport owner and negotiated S7 transaction state, guarded by the client.
#[derive(Debug)]
pub(crate) struct Session {
    connection: Connection,
    /// Negotiated limits used by operation and packet planners.
    pub(crate) limits: Negotiated,
    reference: u16,
    timeout: Duration,
    limiter: crate::policy::Limiter,
    origin: Instant,
}

impl Session {
    /// Establish all layers before publishing this session to a client.
    pub(crate) fn connect(config: &ClientConfig) -> Result<Self> {
        let connection = Connection::connect(&config.transport)?;
        let mut session = Self {
            connection,
            limits: Negotiated {
                caller_jobs: 1,
                callee_jobs: 1,
                pdu_length: config.requested_pdu,
            },
            reference: 0,
            timeout: config.transport.timeout,
            limiter: crate::policy::Limiter::new(config.rate_limit)?,
            origin: Instant::now(),
        };
        let reply = session
            .transact(|reference| protocol::setup_request(reference, config.requested_pdu))?;
        session.limits = protocol::parse_setup(&reply, config.requested_pdu)?;
        Ok(session)
    }

    /// Whether transport has survived every prior exchange.
    pub(crate) fn is_connected(&self) -> bool {
        self.connection.is_connected()
    }
    /// Gracefully disconnect the owned transport.
    pub(crate) fn disconnect(&mut self) -> Result<()> {
        self.connection.disconnect()
    }

    /// Reserve a reference, encode, exchange, and validate without replaying requests.
    pub(crate) fn transact(&mut self, build: impl FnOnce(u16) -> Result<Pdu>) -> Result<Pdu> {
        self.transact_until(build, self.deadline())
    }

    /// Create a total operation deadline shared by every continuation exchange.
    pub(crate) fn deadline(&self) -> Instant {
        Instant::now() + self.timeout
    }

    /// Execute one exchange within an existing operation deadline.
    pub(crate) fn transact_until(
        &mut self,
        build: impl FnOnce(u16) -> Result<Pdu>,
        deadline: Instant,
    ) -> Result<Pdu> {
        if !self.is_connected() {
            return Err(Error::Disconnected);
        }
        if Instant::now() >= deadline {
            return Err(Error::DeadlineBeforeSend);
        }
        let next = self.reference.wrapping_add(1);
        let request = build(next)?.encode()?;
        if request.len() > usize::from(self.limits.pdu_length) {
            return Err(Error::InvalidInput("request exceeds negotiated PDU"));
        }
        loop {
            if Instant::now() >= deadline {
                return Err(Error::DeadlineBeforeSend);
            }
            let wait = self.limiter.poll(self.origin.elapsed())?;
            if wait.is_zero() {
                break;
            }
            if wait >= deadline.saturating_duration_since(Instant::now()) {
                return Err(Error::DeadlineBeforeSend);
            }
            std::thread::sleep(wait);
        }
        self.reference = next;
        if let Err(error) = self.connection.send_until(&request, deadline) {
            self.connection.abort();
            return Err(error);
        }
        let result = self.receive_matching(deadline);
        if result
            .as_ref()
            .is_err_and(|e| !matches!(e, Error::Plc(_) | Error::Authentication(_)))
        {
            self.connection.abort();
        }
        result
    }

    /// Invalidate a structurally wrong service reply before another job reuses it.
    pub(crate) fn validate<T>(&mut self, result: Result<T>) -> Result<T> {
        if matches!(result, Err(Error::Protocol(_))) {
            self.connection.abort();
        }
        result
    }

    /// Ignore a bounded number of stale frames under the original deadline.
    fn receive_matching(&mut self, deadline: Instant) -> Result<Pdu> {
        for _ in 0..=3 {
            let bytes = self.connection.receive_until(deadline)?;
            if bytes.len() > usize::from(self.limits.pdu_length) {
                return Err(Error::Protocol("response exceeds negotiated PDU"));
            }
            let response = Pdu::decode(&bytes)?;
            match protocol::validate_reference(self.reference, response.reference) {
                Err(Error::StaleReference { .. }) => continue,
                other => other?,
            }
            response.check_response()?;
            return Ok(response);
        }
        Err(Error::Protocol("stale response retry limit reached"))
    }
}
