//! Experimental single-master/single-slave S7-200 PPI over 8E1 serial.
//!
//! No multimaster token management or PPI-over-TCP is provided. Read retries
//! are bounded; mutating requests are never retransmitted after possible delivery.

pub mod frame;
use crate::{
    Address, Area, Error, Result, WordLen,
    protocol::{self, Pdu},
};
use std::{
    io::{self, Read, Write},
    time::{Duration, Instant},
};

/// Serial adapter seam. Implementations must honor timeout on every read/write.
pub trait Port: Read + Write {
    /// Set the maximum duration of the next blocking I/O operation.
    fn set_timeout(&mut self, timeout: Duration) -> io::Result<()>;
}
/// Owned OS serial adapter; dropping closes the serial device.
pub struct Serial(Box<dyn serialport::SerialPort>);
impl Serial {
    /// Open a serial device with eight data bits, even parity and one stop bit.
    pub fn open(path: &str, baud: u32, timeout: Duration) -> Result<Self> {
        if baud == 0 || timeout.is_zero() {
            return Err(Error::InvalidInput("invalid serial settings"));
        }
        let port = serialport::new(path, baud)
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::Even)
            .stop_bits(serialport::StopBits::One)
            .flow_control(serialport::FlowControl::None)
            .timeout(timeout)
            .open()
            .map_err(|e| Error::Io(io::Error::other(e)))?;
        Ok(Self(port))
    }
}
impl Read for Serial {
    /// Read a possibly partial serial chunk.
    fn read(&mut self, target: &mut [u8]) -> io::Result<usize> {
        self.0.read(target)
    }
}
impl Write for Serial {
    /// Write a possibly partial serial chunk.
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }
    /// Flush the driver output buffer.
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
impl Port for Serial {
    /// Update the OS driver deadline.
    fn set_timeout(&mut self, timeout: Duration) -> io::Result<()> {
        self.0.set_timeout(timeout).map_err(io::Error::other)
    }
}

/// PPI master addressing and bounded retry settings.
#[derive(Debug, Clone)]
pub struct PpiConfig {
    /// Slave station, 0–126.
    pub station: u8,
    /// Local master station, distinct from the slave.
    pub local_station: u8,
    /// Maximum wait per frame, at most sixty seconds.
    pub timeout: Duration,
    /// Read/setup attempts, 1–8. Writes always have one transmission attempt.
    pub retries: u8,
}
impl Default for PpiConfig {
    /// Upstream S7-200 station and serial timing defaults.
    fn default() -> Self {
        Self {
            station: 2,
            local_station: 0,
            timeout: Duration::from_millis(150),
            retries: 3,
        }
    }
}
impl PpiConfig {
    /// Validate station and timing bounds.
    fn validate(&self) -> Result<()> {
        if self.station > 126
            || self.local_station > 126
            || self.station == self.local_station
            || self.timeout.is_zero()
            || self.timeout > Duration::from_secs(60)
            || !(1..=8).contains(&self.retries)
        {
            return Err(Error::InvalidInput(
                "invalid PPI station, timeout or retries",
            ));
        }
        Ok(())
    }
}
/// Owned transport; mutable access serializes complete exchanges.
pub struct PpiTransport<P: Port> {
    port: P,
    config: PpiConfig,
}
impl<P: Port> PpiTransport<P> {
    /// Accept an already opened serial adapter with explicit timeout behavior.
    pub fn new(port: P, config: PpiConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self { port, config })
    }
    /// Release the port to the caller without a hidden background task.
    pub fn into_inner(self) -> P {
        self.port
    }
    /// Read a complete frame under one absolute frame deadline.
    fn read_frame(&mut self, deadline: Instant) -> Result<frame::Frame> {
        let mut bytes = vec![0];
        self.read_exact(&mut bytes, deadline)?;
        let remaining = match bytes[0] {
            0xe5 => return Ok(frame::Frame::Ack),
            0x10 => 5,
            0xa2 => 13,
            0x68 => {
                let mut prefix = [0; 3];
                self.read_exact(&mut prefix, deadline)?;
                if prefix[0] != prefix[1] || prefix[2] != 0x68 || !(3..=249).contains(&prefix[0]) {
                    return Err(Error::Protocol("invalid PPI repeated frame length"));
                }
                bytes.extend_from_slice(&prefix);
                usize::from(prefix[0]) + 2
            }
            _ => return Err(Error::Protocol("unknown PPI delimiter")),
        };
        let start = bytes.len();
        bytes.resize(start + remaining, 0);
        self.read_exact(&mut bytes[start..], deadline)?;
        frame::decode(&bytes)
    }
    /// Handle partial reads without resetting the total frame deadline.
    fn read_exact(&mut self, mut bytes: &mut [u8], deadline: Instant) -> Result<()> {
        while !bytes.is_empty() {
            self.port.set_timeout(remaining(deadline)?)?;
            match self.port.read(bytes) {
                Ok(0) => return Err(Error::Timeout),
                Ok(n) => bytes = &mut bytes[n..],
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    /// Handle partial writes, preserving the same absolute exchange deadline.
    fn write_frame(&mut self, mut bytes: &[u8], deadline: Instant) -> Result<()> {
        while !bytes.is_empty() {
            self.port.set_timeout(remaining(deadline)?)?;
            match self.port.write(bytes) {
                Ok(0) => return Err(Error::Io(io::ErrorKind::WriteZero.into())),
                Ok(n) => bytes = &bytes[n..],
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    /// Exchange an SD2 request, E5 ACK, SD1 poll and addressed SD2 reply.
    /// replay_safe must be false for writes or other mutating S7 operations.
    pub fn exchange(&mut self, pdu: &[u8], replay_safe: bool) -> Result<Vec<u8>> {
        let request = frame::encode(
            0x68,
            self.config.station,
            self.config.local_station,
            0x6c,
            pdu,
        )?;
        let attempts = if replay_safe { self.config.retries } else { 1 };
        let deadline = Instant::now() + self.config.timeout * (u32::from(attempts) + 2);
        let mut acknowledged = false;
        for attempt in 0..attempts {
            self.write_frame(&request, deadline)?;
            let result = self.read_frame((Instant::now() + self.config.timeout).min(deadline));
            match result {
                Ok(frame::Frame::Ack) => {
                    acknowledged = true;
                    break;
                }
                Ok(_) => return Err(Error::Protocol("expected PPI E5 acknowledgement")),
                Err(error) if attempt + 1 == attempts => return Err(error),
                Err(_) => {}
            }
        }
        if !acknowledged {
            return Err(Error::Timeout);
        }
        let mut control = 0x5c;
        self.poll(control, deadline)?;
        for _ in 0..u16::from(self.config.retries) * 2 {
            match self.read_frame((Instant::now() + self.config.timeout).min(deadline)) {
                Ok(frame::Frame::Ack) => {
                    control = if control == 0x5c { 0x7c } else { 0x5c };
                    self.poll(control, deadline)?;
                }
                Ok(frame::Frame::Data {
                    delimiter: 0x68,
                    destination,
                    source,
                    payload,
                    ..
                }) if destination == self.config.local_station && source == self.config.station => {
                    return Ok(payload);
                }
                Ok(_) => {
                    return Err(Error::Protocol(
                        "unexpected PPI response station or delimiter",
                    ));
                }
                Err(Error::Protocol(_)) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(Error::Timeout)
    }
    /// Send a request-data poll with the current frame-count bit.
    fn poll(&mut self, control: u8, deadline: Instant) -> Result<()> {
        let frame = frame::encode(
            0x10,
            self.config.station,
            self.config.local_station,
            control,
            &[],
        )?;
        self.write_frame(&frame, deadline)
    }
}

/// S7-200 memory identifiers, including V memory mapped to wire DB1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PpiArea {
    /// Sequence-state memory.
    S = 3,
    /// Special memory.
    Sm = 5,
    /// Analog inputs, 16-bit items.
    Ai = 6,
    /// Analog outputs, 16-bit items.
    Aq = 7,
    /// Process inputs.
    I = 0x81,
    /// Process outputs.
    Q = 0x82,
    /// Markers.
    M = 0x83,
    /// V memory (DB1).
    V = 0x84,
    /// S7-200 counters, indexed 16-bit items.
    C = 0x1e,
    /// S7-200 timers, indexed 16-bit items.
    T = 0x1f,
}
impl PpiArea {
    /// Reuse standard width/range validation before substituting the S7-200 area code.
    fn address(self, start: u32, count: u16) -> Result<Address> {
        let (area, db, word) = match self {
            Self::V => (Area::Db, 1, WordLen::Byte),
            Self::Ai | Self::Aq => (Area::Markers, 0, WordLen::Word),
            Self::C => (Area::Counters, 0, WordLen::Counter200),
            Self::T => (Area::Timers, 0, WordLen::Timer200),
            _ => (Area::Markers, 0, WordLen::Byte),
        };
        Address::new(area, db, start, word, count)
    }
}

/// Negotiated single-slave PPI client; each operation borrows it exclusively.
pub struct PpiClient<P: Port> {
    transport: PpiTransport<P>,
    pdu: u16,
    reference: u16,
    connected: bool,
}
impl<P: Port> PpiClient<P> {
    /// Negotiate a PDU no larger than 240 bytes (SD2 supports at most 246).
    pub fn connect(transport: PpiTransport<P>) -> Result<Self> {
        let mut client = Self {
            transport,
            pdu: 240,
            reference: 1,
            connected: true,
        };
        let reply = client.exchange(protocol::setup_request(1, 240)?, true)?;
        client.pdu = protocol::parse_setup(&reply, 240)?.pdu_length;
        Ok(client)
    }
    /// Negotiated application PDU bytes.
    pub fn pdu_length(&self) -> u16 {
        self.pdu
    }
    /// Whether transport failures have invalidated this session.
    pub fn is_connected(&self) -> bool {
        self.connected
    }
    /// Disconnect by consuming the session and returning its transport.
    pub fn disconnect(mut self) -> PpiTransport<P> {
        self.connected = false;
        self.transport
    }
    /// Read byte or 16-bit items; large operations are explicitly rejected.
    pub fn read_area(&mut self, area: PpiArea, start: u32, count: u16) -> Result<Vec<u8>> {
        let address = area.address(start, count)?;
        if address.byte_len() > usize::from(self.pdu) - 18 {
            return Err(Error::InvalidInput("PPI read exceeds negotiated PDU"));
        }
        let mut job = protocol::read_request(self.reference, &[address])?;
        job.parameters[10] = area as u8;
        let reply = self.exchange(job, true)?;
        let result = protocol::parse_read(&reply, &[address]).and_then(|mut items| items.remove(0));
        if matches!(result, Err(Error::Protocol(_))) {
            self.connected = false;
        }
        result
    }
    /// Write complete byte/word items once. Unknown outcomes are never replayed.
    pub fn write_area(&mut self, area: PpiArea, start: u32, data: &[u8]) -> Result<()> {
        let width = if matches!(area, PpiArea::Ai | PpiArea::Aq | PpiArea::C | PpiArea::T) {
            2
        } else {
            1
        };
        if data.is_empty()
            || !data.len().is_multiple_of(width)
            || data.len() > usize::from(self.pdu) - 35
        {
            return Err(Error::InvalidInput("invalid PPI write width or PDU budget"));
        }
        let count = u16::try_from(data.len() / width)
            .map_err(|_| Error::InvalidInput("PPI item count overflow"))?;
        let address = area.address(start, count)?;
        let mut job = protocol::write_request(self.reference, &[(address, data)])?;
        job.parameters[10] = area as u8;
        let result = self
            .exchange(job, false)
            .and_then(|reply| protocol::parse_write(&reply, 1)?.remove(0));
        if matches!(result, Err(Error::Protocol(_))) {
            self.connected = false;
        }
        result.map_err(|error| crate::operations::write_failure(error, 0, data.len()))
    }
    /// Read S7-200 V memory.
    pub fn v_read(&mut self, start: u32, size: u16) -> Result<Vec<u8>> {
        self.read_area(PpiArea::V, start, size)
    }
    /// Write S7-200 V memory.
    pub fn v_write(&mut self, start: u32, data: &[u8]) -> Result<()> {
        self.write_area(PpiArea::V, start, data)
    }
    /// Advance references and invalidate malformed/lost sessions before returning errors.
    fn exchange(&mut self, mut job: Pdu, replay_safe: bool) -> Result<Pdu> {
        if !self.connected {
            return Err(Error::Disconnected);
        }
        job.reference = self.reference;
        self.reference = self.reference.wrapping_add(1);
        let bytes = job.encode()?;
        let result = self
            .transport
            .exchange(&bytes, replay_safe)
            .and_then(|raw| {
                if raw.len() > self.pdu as usize {
                    return Err(Error::Protocol("PPI response exceeds negotiated PDU"));
                }
                let response = Pdu::decode(&raw)?;
                protocol::validate_reference(job.reference, response.reference)?;
                response.check_response()?;
                Ok(response)
            });
        if result.is_err() && !matches!(result, Err(Error::Plc(_) | Error::Item(_))) {
            self.connected = false;
        }
        result
    }
}
/// Convert absolute deadlines to per-driver waits without zero-duration busy loops.
fn remaining(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or(Error::Timeout)
}

impl PpiClient<Serial> {
    /// Open an OS serial device using 8E1, then negotiate the S7-200 session.
    pub fn open(path: &str, baud: u32, config: PpiConfig) -> Result<Self> {
        config.validate()?;
        let port = Serial::open(path, baud, config.timeout)?;
        Self::connect(PpiTransport::new(port, config)?)
    }
}
