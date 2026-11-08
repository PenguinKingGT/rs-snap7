//! Blocking client configuration and public operation entry points.

use crate::protocol::{Pdu, PduType};
use crate::session::Session;
use crate::transport::TransportConfig;
use crate::{Error, Result};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};

/// Validated connection settings, editable before establishing a session.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// TCP/COTP settings, including explicit TSAP and optional routing.
    pub transport: TransportConfig,
    /// Requested S7 PDU bytes; the peer may negotiate a smaller value.
    pub requested_pdu: u16,
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
        })
    }

    /// Validate caller-provided settings before touching the network.
    pub fn validate(&self) -> Result<()> {
        self.transport.validate()?;
        if self.requested_pdu < 128
            || usize::from(self.requested_pdu) > self.transport.max_message_size
        {
            return Err(Error::InvalidInput("invalid requested S7 PDU size"));
        }
        Ok(())
    }
}

/// A blocking S7 client. Clones share one serialized session.
///
/// Each complete logical operation holds the session guard, including multi-step
/// exchanges. Disconnecting any clone disconnects all clones. Dropping the last
/// clone releases the socket. Use independent clients for parallel connections.
#[derive(Debug, Clone)]
pub struct Client {
    pub(crate) inner: Arc<Mutex<Session>>,
    config: ClientConfig,
}

impl Client {
    /// Complete TCP, COTP, and S7 negotiation or return an error without a session.
    pub fn connect(config: ClientConfig) -> Result<Self> {
        config.validate()?;
        let session = Session::connect(&config)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(session)),
            config,
        })
    }

    /// True when this object still owns a ready local session; not a remote probe.
    pub fn is_connected(&self) -> bool {
        self.inner.lock().map(|s| s.is_connected()).unwrap_or(false)
    }

    /// Negotiated application PDU size. A disconnected client returns an error.
    pub fn pdu_length(&self) -> Result<u16> {
        let session = self.session()?;
        if !session.is_connected() {
            return Err(Error::Disconnected);
        }
        Ok(session.limits.pdu_length)
    }

    /// Close the shared transport; repeated disconnects are harmless.
    pub fn disconnect(&self) -> Result<()> {
        self.session()?.disconnect()
    }

    /// Explicitly reconnect and renegotiate using the original validated settings.
    /// No in-flight application operation is replayed.
    pub fn reconnect(&self) -> Result<()> {
        let mut session = self.session()?;
        let _ = session.disconnect();
        *session = Session::connect(&self.config)?;
        Ok(())
    }

    /// Exchange a custom S7 job using the session's next correlation reference.
    /// The supplied reference is replaced. Malformed or unmatched replies close
    /// the session; this advanced operation never automatically retries a job.
    pub fn iso_exchange(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        let job = Pdu::decode(bytes)?;
        if job.kind != PduType::Job && job.kind != PduType::UserData {
            return Err(Error::InvalidInput(
                "raw exchange requires a job or user-data request",
            ));
        }
        self.session()?
            .transact(|reference| Ok(Pdu { reference, ..job }))?
            .encode()
    }

    /// Obtain exclusive operation ownership, reporting poison instead of panicking.
    pub(crate) fn session(&self) -> Result<MutexGuard<'_, Session>> {
        self.inner
            .lock()
            .map_err(|_| Error::Protocol("session lock poisoned"))
    }
}
