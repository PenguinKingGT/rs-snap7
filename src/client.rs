//! Blocking client configuration and public operation entry points.

mod batch;
mod diagnostics;
mod force;
mod management;
mod security;
mod typed;

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
    /// Per-session wire request limiter, including negotiation and continuation jobs.
    pub rate_limit: crate::policy::RateLimit,
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
        })
    }

    /// Validate caller-provided settings before touching the network.
    pub fn validate(&self) -> Result<()> {
        self.transport.validate()?;
        self.rate_limit.validate()?;
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

impl Client {
    /// Read raw elements from an area. Timer/counter offsets and counts use elements;
    /// other areas use bytes. Large transfers are split using negotiated PDU limits.
    pub fn read_area(
        &self,
        area: crate::Area,
        db: u16,
        start: u32,
        count: usize,
    ) -> Result<Vec<u8>> {
        self.session()?.read_area(area, db, start, count)
    }

    /// Write raw area data. Timer/counter payloads require complete 16-bit elements.
    /// Communication failures report uncertain writes; writes are never replayed.
    pub fn write_area(&self, area: crate::Area, db: u16, start: u32, data: &[u8]) -> Result<()> {
        self.session()?.write_area(area, db, start, data)
    }

    /// Read bytes from a data block.
    pub fn db_read(&self, db: u16, start: u32, size: usize) -> Result<Vec<u8>> {
        self.read_area(crate::Area::Db, db, start, size)
    }

    /// Write bytes to a data block; acknowledged/uncertain progress is retained in errors.
    pub fn db_write(&self, db: u16, start: u32, data: &[u8]) -> Result<()> {
        self.write_area(crate::Area::Db, db, start, data)
    }

    /// Read an entire DB. Supply its size for PLCs with unreliable block metadata;
    /// `None` queries MC7 size, falling back to 65536 only for a reported zero size.
    pub fn db_get(&self, db: u16, size: Option<usize>) -> Result<Vec<u8>> {
        let mut session = self.session()?;
        let size = match size {
            Some(size) => size,
            None => session.db_size(db)?,
        };
        session.read_area(crate::Area::Db, db, 0, size)
    }

    /// Fill a whole DB with one byte; `None` determines its size from block metadata.
    pub fn db_fill(&self, db: u16, filler: u8, size: Option<usize>) -> Result<()> {
        let mut session = self.session()?;
        let size = match size {
            Some(size) => size,
            None => session.db_size(db)?,
        };
        if size > 0x20_0000 {
            return Err(Error::InvalidInput("DB exceeds address space"));
        }
        session.write_area(crate::Area::Db, db, 0, &vec![filler; size])
    }
}

impl Client {
    /// Read outputs; counts use the area's natural element units.
    pub fn ab_read(&self, start: u32, count: usize) -> Result<Vec<u8>> {
        self.read_area(crate::Area::Outputs, 0, start, count)
    }
    /// Write outputs; payload bytes must contain complete elements.
    pub fn ab_write(&self, start: u32, data: &[u8]) -> Result<()> {
        self.write_area(crate::Area::Outputs, 0, start, data)
    }
}

impl Client {
    /// Read inputs; counts use the area's natural element units.
    pub fn eb_read(&self, start: u32, count: usize) -> Result<Vec<u8>> {
        self.read_area(crate::Area::Inputs, 0, start, count)
    }
    /// Write inputs; payload bytes must contain complete elements.
    pub fn eb_write(&self, start: u32, data: &[u8]) -> Result<()> {
        self.write_area(crate::Area::Inputs, 0, start, data)
    }
}

impl Client {
    /// Read markers; counts use the area's natural element units.
    pub fn mb_read(&self, start: u32, count: usize) -> Result<Vec<u8>> {
        self.read_area(crate::Area::Markers, 0, start, count)
    }
    /// Write markers; payload bytes must contain complete elements.
    pub fn mb_write(&self, start: u32, data: &[u8]) -> Result<()> {
        self.write_area(crate::Area::Markers, 0, start, data)
    }
}

impl Client {
    /// Read timer elements; counts use the area's natural element units.
    pub fn tm_read(&self, start: u32, count: usize) -> Result<Vec<u8>> {
        self.read_area(crate::Area::Timers, 0, start, count)
    }
    /// Write timer elements; payload bytes must contain complete elements.
    pub fn tm_write(&self, start: u32, data: &[u8]) -> Result<()> {
        self.write_area(crate::Area::Timers, 0, start, data)
    }
}

impl Client {
    /// Read counter elements; counts use the area's natural element units.
    pub fn ct_read(&self, start: u32, count: usize) -> Result<Vec<u8>> {
        self.read_area(crate::Area::Counters, 0, start, count)
    }
    /// Write counter elements; payload bytes must contain complete elements.
    pub fn ct_write(&self, start: u32, data: &[u8]) -> Result<()> {
        self.write_area(crate::Area::Counters, 0, start, data)
    }
}
