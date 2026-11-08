//! Explicit synchronous transport for server, Partner, and advanced blocking callers.
use super::{TransportConfig, connection_request, frame_bytes, frame_payload_len, parse_confirm};
use crate::{Error, Result};
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpStream},
    time::{Duration, Instant},
};

/// A negotiated COTP Class 0 connection. S7 negotiation belongs to the client.
#[derive(Debug)]
pub struct Connection {
    stream: Option<TcpStream>,
    timeout: Duration,
    max_message: usize,
    tpdu_size: usize,
    peer_reference: u16,
}

impl Connection {
    /// Connect TCP and complete COTP negotiation, closing on any failure.
    pub fn connect(config: &TransportConfig) -> Result<Self> {
        config.validate()?;
        let deadline = Instant::now() + config.timeout;
        let mut stream = TcpStream::connect_timeout(&config.endpoint, remaining(deadline)?)?;
        stream.set_nodelay(true)?;
        write_frame(&mut stream, &connection_request(config)?, deadline)?;
        let confirm = read_frame(&mut stream, deadline)?;
        let (peer_reference, tpdu_size) = parse_confirm(&confirm, 1usize << config.tpdu_exponent)?;
        Ok(Self {
            stream: Some(stream),
            timeout: config.timeout,
            max_message: config.max_message_size,
            tpdu_size,
            peer_reference,
        })
    }

    /// Whether this object still owns a usable local socket (not a remote probe).
    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }
    /// Negotiated maximum COTP TPDU size, excluding the TPKT header.
    pub fn tpdu_size(&self) -> usize {
        self.tpdu_size
    }

    /// Send one message, fragmenting at the negotiated TPDU limit.
    pub fn send(&mut self, data: &[u8]) -> Result<()> {
        if data.len() > self.max_message {
            return Err(Error::InvalidInput("message exceeds transport limit"));
        }
        let deadline = Instant::now() + self.timeout;
        let result = self.send_until(data, deadline);
        self.close_on_error(result)
    }

    /// Receive one complete message; reassembly and fragment count are bounded.
    pub fn receive(&mut self) -> Result<Vec<u8>> {
        let deadline = Instant::now() + self.timeout;
        let result = self.receive_until(deadline);
        self.close_on_error(result)
    }

    /// Send and receive under a single total deadline.
    pub fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>> {
        if request.len() > self.max_message {
            return Err(Error::InvalidInput("message exceeds transport limit"));
        }
        let deadline = Instant::now() + self.timeout;
        let result = self
            .send_until(request, deadline)
            .and_then(|()| self.receive_until(deadline));
        self.close_on_error(result)
    }

    /// Close a session whose protocol stream is no longer safe to reuse.
    pub(crate) fn abort(&mut self) {
        if let Some(stream) = self.stream.take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }

    /// Best-effort COTP disconnect followed by unconditional socket closure.
    pub fn disconnect(&mut self) -> Result<()> {
        let Some(mut stream) = self.stream.take() else {
            return Ok(());
        };
        let peer = self.peer_reference.to_be_bytes();
        let request = [6, 0x80, peer[0], peer[1], 0, 1, 0];
        let result = write_frame(&mut stream, &request, Instant::now() + self.timeout);
        let _ = stream.shutdown(Shutdown::Both);
        result
    }

    /// Serialize fragments; the low seven TPDU number bits remain zero in Class 0.
    pub(crate) fn send_until(&mut self, data: &[u8], deadline: Instant) -> Result<()> {
        let stream = self.stream.as_mut().ok_or(Error::Disconnected)?;
        let width = self.tpdu_size - 3;
        if data.is_empty() {
            return write_frame(stream, &[2, 0xf0, 0x80], deadline);
        }
        let chunks = data.chunks(width);
        let count = chunks.len();
        for (i, chunk) in chunks.enumerate() {
            let mut payload = vec![2, 0xf0, if i + 1 == count { 0x80 } else { 0 }];
            payload.extend_from_slice(chunk);
            write_frame(stream, &payload, deadline)?;
        }
        Ok(())
    }

    /// Assemble at most 1024 fragments and at most the configured message limit.
    pub(crate) fn receive_until(&mut self, deadline: Instant) -> Result<Vec<u8>> {
        let stream = self.stream.as_mut().ok_or(Error::Disconnected)?;
        let mut message = Vec::new();
        for _ in 0..1024 {
            let frame = read_frame(stream, deadline)?;
            if frame.len() < 3
                || frame[0] != 2
                || frame[1] != 0xf0
                || frame[2] & 0x7f != 0
                || frame.len() > self.tpdu_size
            {
                return Err(Error::Protocol("invalid COTP data fragment"));
            }
            if message.len() + frame.len() - 3 > self.max_message {
                return Err(Error::Protocol("reassembled message exceeds limit"));
            }
            message.extend_from_slice(&frame[3..]);
            if frame[2] & 0x80 != 0 {
                return Ok(message);
            }
            if frame.len() == 3 {
                return Err(Error::Protocol("empty continuation fragment"));
            }
        }
        Err(Error::Protocol("too many COTP fragments"))
    }

    /// An incomplete stream must not survive into a later transaction.
    fn close_on_error<T>(&mut self, result: Result<T>) -> Result<T> {
        if result.is_err() {
            self.abort();
        }
        result
    }
}

impl Drop for Connection {
    /// Release the descriptor without waiting for a network peer during drop.
    fn drop(&mut self) {
        self.abort();
    }
}

fn remaining(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or(Error::Timeout)
}

/// Read a complete frame with a single deadline across arbitrarily partial reads.
pub(crate) fn read_frame(stream: &mut TcpStream, deadline: Instant) -> Result<Vec<u8>> {
    let mut header = [0; 4];
    read_exact(stream, &mut header, deadline)?;
    let mut payload = vec![0; frame_payload_len(header)?];
    read_exact(stream, &mut payload, deadline)?;
    Ok(payload)
}

/// Read exactly the required bytes while bounding partial-read peers.
fn read_exact(stream: &mut TcpStream, mut target: &mut [u8], deadline: Instant) -> Result<()> {
    while !target.is_empty() {
        stream.set_read_timeout(Some(remaining(deadline)?))?;
        match stream.read(target) {
            Ok(0) => return Err(Error::Io(std::io::ErrorKind::UnexpectedEof.into())),
            Ok(n) => {
                target = &mut target[n..];
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// Write a TPKT frame without permitting a truncated 16-bit length.
pub(crate) fn write_frame(stream: &mut TcpStream, payload: &[u8], deadline: Instant) -> Result<()> {
    let frame = frame_bytes(payload)?;
    let mut remaining_bytes = frame.as_slice();
    while !remaining_bytes.is_empty() {
        stream.set_write_timeout(Some(remaining(deadline)?))?;
        match stream.write(remaining_bytes) {
            Ok(0) => return Err(Error::Io(std::io::ErrorKind::WriteZero.into())),
            Ok(n) => remaining_bytes = &remaining_bytes[n..],
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// Accept COTP Class 0 for optional server/partner adapters under one deadline.
/// Validate TLV bounds and echo the caller's reference in the confirmation.
#[cfg(any(feature = "server", feature = "partner"))]
pub(crate) fn accept_connection(
    mut stream: TcpStream,
    timeout: Duration,
    max_message: usize,
    allowed_tsap: Option<&[u8]>,
) -> Result<Connection> {
    if timeout.is_zero()
        || Instant::now().checked_add(timeout).is_none()
        || !(1..=65535).contains(&max_message)
    {
        return Err(Error::InvalidInput("invalid passive transport limits"));
    }
    stream.set_nonblocking(false)?;
    stream.set_nodelay(true)?;
    let deadline = Instant::now() + timeout;
    let request = read_frame(&mut stream, deadline)?;
    if request.len() < 7
        || request[0] as usize + 1 != request.len()
        || request[1] != 0xe0
        || request[2..4] != [0, 0]
        || request[6] != 0
    {
        return Err(Error::Protocol("invalid COTP connection request"));
    }
    let peer_reference = u16::from_be_bytes([request[4], request[5]]);
    if peer_reference == 0 {
        return Err(Error::Protocol("zero COTP source reference"));
    }
    let mut tpdu_size: usize = 128;
    let mut called = None;
    let mut seen = Vec::new();
    let mut offset = 7;
    while offset < request.len() {
        if offset + 2 > request.len() {
            return Err(Error::Protocol("truncated COTP parameter"));
        }
        let kind = request[offset];
        let length = usize::from(request[offset + 1]);
        offset += 2;
        if length == 0 || offset + length > request.len() || seen.contains(&kind) {
            return Err(Error::Protocol(
                "invalid COTP parameter length or duplicate",
            ));
        }
        seen.push(kind);
        let value = &request[offset..offset + length];
        match kind {
            0xc0 => {
                if length != 1 || !(7..=13).contains(&value[0]) {
                    return Err(Error::Protocol("invalid COTP TPDU exponent"));
                }
                tpdu_size = 1 << value[0].min(10);
            }
            0xc2 => called = Some(value),
            _ => {}
        }
        offset += length;
    }
    if allowed_tsap.is_some() && called != allowed_tsap {
        return Err(Error::Protocol("called TSAP mismatch"));
    }
    let mut confirm = vec![9, 0xd0];
    confirm.extend_from_slice(&peer_reference.to_be_bytes());
    confirm.extend_from_slice(&[0, 1, 0, 0xc0, 1, tpdu_size.trailing_zeros() as u8]);
    write_frame(&mut stream, &confirm, deadline)?;
    Ok(Connection {
        stream: Some(stream),
        timeout,
        max_message,
        tpdu_size,
        peer_reference,
    })
}

#[cfg(feature = "partner")]
impl Connection {
    /// Duplicate the OS socket for one receiver and one serialized sender.
    /// The Partner dispatcher is the sole reader; shutdown affects both handles.
    pub(crate) fn reader_clone(&self) -> Result<Self> {
        Ok(Self {
            stream: Some(
                self.stream
                    .as_ref()
                    .ok_or(Error::Disconnected)?
                    .try_clone()?,
            ),
            timeout: self.timeout,
            max_message: self.max_message,
            tpdu_size: self.tpdu_size,
            peer_reference: self.peer_reference,
        })
    }
}
