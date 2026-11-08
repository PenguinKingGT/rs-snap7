//! Shared validated S7 client configuration, independent of execution.

use crate::{Error, Result, transport::TransportConfig};
use std::net::SocketAddr;

/// Validated connection settings, editable before establishing a session.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// TCP/COTP settings, including explicit TSAP and optional routing.
    pub transport: TransportConfig,
    /// Requested S7 PDU bytes; the peer may negotiate a smaller value.
    pub requested_pdu: u16,
    /// Per-session wire request limiter, including negotiation and continuation jobs.
    pub rate_limit: crate::policy::RateLimit,
    /// Redacted logical-operation event sink, invoked outside session locks.
    pub operation_hook: crate::observability::OperationHook,
    /// Optional bounded recovery and heartbeat settings.
    pub recovery: crate::policy::Recovery,
    /// Loss/recovery transitions dispatched outside network locks.
    pub connection_hook: crate::policy::ConnectionHook,
}

impl ClientConfig {
    /// Create PG settings for a rack (0–7) and slot (0–31).
    pub fn new(endpoint: SocketAddr, rack: u8, slot: u8) -> Result<Self> {
        if rack > 7 || slot > 31 {
            return Err(Error::InvalidInput("invalid PLC rack/slot"));
        }
        let mut transport = TransportConfig::new(endpoint);
        transport.remote_tsap = (0x0100 | u16::from(rack) << 5 | u16::from(slot))
            .to_be_bytes()
            .to_vec();
        Ok(Self {
            transport,
            requested_pdu: 480,
            rate_limit: Default::default(),
            operation_hook: Default::default(),
            recovery: Default::default(),
            connection_hook: Default::default(),
        })
    }

    /// Validate caller-provided settings before touching the network.
    pub fn validate(&self) -> Result<()> {
        self.transport.validate()?;
        self.rate_limit.validate()?;
        self.recovery.validate()?;
        if self.requested_pdu < 128
            || usize::from(self.requested_pdu) > self.transport.max_message_size
        {
            return Err(Error::InvalidInput("invalid requested S7 PDU size"));
        }
        Ok(())
    }
}
