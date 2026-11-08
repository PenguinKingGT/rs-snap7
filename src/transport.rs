//! Advanced ISO-on-TCP transport, independent of S7 application operations.
//!
//! Each send/receive has a total deadline. Failed exchanges close the socket so
//! incomplete messages cannot become the next operation's response.

use crate::{Error, Result};
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

/// Experimental gateway routing parameters, matching upstream C6/C7 parameters.
#[derive(Debug, Clone, Copy)]
pub struct Routing {
    /// Target network subnet identifier.
    pub subnet: u16,
    /// Final target rack, 0 through 7.
    pub rack: u8,
    /// Final target slot, 0 through 31.
    pub slot: u8,
}

/// Transport settings. Validate them before opening a socket.
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// PLC or gateway TCP endpoint.
    pub endpoint: SocketAddr,
    /// Calling TSAP.
    pub local_tsap: u16,
    /// Called TSAP, including variable-length LOGO!/HMI forms.
    pub remote_tsap: Vec<u8>,
    /// Requested TPDU size exponent, 7 through 13.
    pub tpdu_exponent: u8,
    /// Total deadline per transport operation.
    pub timeout: Duration,
    /// Maximum reassembled payload bytes; bounded to one S7 length space.
    pub max_message_size: usize,
    /// Optional experimental gateway route.
    pub routing: Option<Routing>,
}

impl TransportConfig {
    /// Construct standard PG transport settings for a resolved endpoint.
    pub fn new(endpoint: SocketAddr) -> Self {
        Self {
            endpoint,
            local_tsap: 0x0100,
            remote_tsap: vec![0x01, 0x02],
            tpdu_exponent: 10,
            timeout: Duration::from_secs(5),
            max_message_size: 65535,
            routing: None,
        }
    }

    /// Validate sizes and routing fields without performing network I/O.
    pub fn validate(&self) -> Result<()> {
        let route_bytes = if self.routing.is_some() { 8 } else { 0 };
        if self.remote_tsap.is_empty()
            || self.remote_tsap.len() + 15 + route_bytes > 255
            || !(7..=13).contains(&self.tpdu_exponent)
            || self.timeout.is_zero()
            || Instant::now().checked_add(self.timeout).is_none()
            || self.max_message_size == 0
            || self.max_message_size > 65535
        {
            return Err(Error::InvalidInput("invalid transport settings"));
        }
        if let Some(route) = self.routing
            && (route.rack > 7 || route.slot > 31)
        {
            return Err(Error::InvalidInput("invalid routing rack/slot"));
        }
        Ok(())
    }
}

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

/// Encode the calling/called TSAP and optional route into a COTP CR.
pub(crate) fn connection_request(config: &TransportConfig) -> Result<Vec<u8>> {
    config.validate()?;
    let mut pdu = vec![0, 0xe0, 0, 0, 0, 1, 0, 0xc1, 2];
    pdu.extend_from_slice(&config.local_tsap.to_be_bytes());
    pdu.extend_from_slice(&[0xc2, config.remote_tsap.len() as u8]);
    pdu.extend_from_slice(&config.remote_tsap);
    pdu.extend_from_slice(&[0xc0, 1, config.tpdu_exponent]);
    if let Some(route) = config.routing {
        pdu.extend_from_slice(&[0xc6, 2]);
        pdu.extend_from_slice(&route.subnet.to_be_bytes());
        pdu.extend_from_slice(&[0xc7, 2]);
        pdu.extend_from_slice(
            &(0x0100 | u16::from(route.rack) << 5 | u16::from(route.slot)).to_be_bytes(),
        );
    }
    pdu[0] = (pdu.len() - 1) as u8;
    Ok(pdu)
}

/// Validate the peer reference and parameter TLVs without trusting length fields.
pub(crate) fn parse_confirm(pdu: &[u8], requested: usize) -> Result<(u16, usize)> {
    if pdu.len() < 7
        || usize::from(pdu[0]) + 1 != pdu.len()
        || pdu[1] != 0xd0
        || pdu[2..4] != [0, 1]
        || pdu[6] != 0
    {
        return Err(Error::Protocol("invalid COTP connection confirm"));
    }
    let mut size = requested;
    let mut offset = 7;
    let mut seen_size = false;
    while offset < pdu.len() {
        if offset + 2 > pdu.len() {
            return Err(Error::Protocol("truncated COTP parameter"));
        }
        let code = pdu[offset];
        let length = usize::from(pdu[offset + 1]);
        offset += 2;
        let value = pdu
            .get(offset..offset + length)
            .ok_or(Error::Protocol("truncated COTP parameter value"))?;
        if code == 0xc0 {
            if seen_size {
                return Err(Error::Protocol("duplicate TPDU size"));
            }
            seen_size = true;
            let received = match value {
                [exponent] if (7..=13).contains(exponent) => 1usize << exponent,
                [high, low] => usize::from(u16::from_be_bytes([*high, *low])),
                _ => return Err(Error::Protocol("invalid negotiated TPDU size")),
            };
            if !(128..=8192).contains(&received) {
                return Err(Error::Protocol("invalid negotiated TPDU size"));
            }
            size = requested.min(received);
        }
        offset += length;
    }
    Ok((u16::from_be_bytes([pdu[4], pdu[5]]), size))
}

/// Remaining monotonic deadline, rounded by the OS only at socket operations.
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

/// Shared strict TPKT header validation for blocking and async adapters.
pub(crate) fn frame_payload_len(header: [u8; 4]) -> Result<usize> {
    let size = usize::from(u16::from_be_bytes([header[2], header[3]]));
    if header[0] != 3 || header[1] != 0 || size < 7 {
        return Err(Error::Protocol("invalid TPKT header"));
    }
    Ok(size - 4)
}

/// Shared bounded TPKT encoding for blocking and async adapters.
pub(crate) fn frame_bytes(payload: &[u8]) -> Result<Vec<u8>> {
    if !(3..=65531).contains(&payload.len()) {
        return Err(Error::InvalidInput("invalid TPKT payload size"));
    }
    let mut frame = vec![3, 0];
    frame.extend_from_slice(&((payload.len() + 4) as u16).to_be_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

#[cfg(test)]
mod tests {
    //! Pure negotiation vectors and malformed TLV coverage.
    use super::*;

    /// Connection request bytes match the upstream reference implementation.
    #[test]
    fn request_vector_and_routing() {
        let mut config = TransportConfig::new("127.0.0.1:102".parse().unwrap());
        assert_eq!(
            connection_request(&config).unwrap(),
            [
                17, 0xe0, 0, 0, 0, 1, 0, 0xc1, 2, 1, 0, 0xc2, 2, 1, 2, 0xc0, 1, 10
            ]
        );
        config.routing = Some(Routing {
            subnet: 0x1234,
            rack: 1,
            slot: 3,
        });
        assert_eq!(
            &connection_request(&config).unwrap()[18..],
            &[0xc6, 2, 0x12, 0x34, 0xc7, 2, 1, 0x23]
        );
        config.remote_tsap = b"SIMATIC-ROOT-HMI".to_vec();
        assert!(
            connection_request(&config)
                .unwrap()
                .windows(b"SIMATIC-ROOT-HMI".len())
                .any(|s| s == b"SIMATIC-ROOT-HMI")
        );
        config.routing.as_mut().unwrap().slot = 32;
        assert!(config.validate().is_err());
    }

    /// Never trust CC lengths, references, or negotiated size parameters.
    #[test]
    fn validates_confirm_headers_and_size_tlvs() {
        let valid = [9, 0xd0, 0, 1, 0x12, 0x34, 0, 0xc0, 1, 7];
        assert_eq!(parse_confirm(&valid, 1024).unwrap(), (0x1234, 128));
        for n in 0..valid.len() {
            assert!(parse_confirm(&valid[..n], 1024).is_err());
        }
        for index in [0, 1, 2, 3, 6, 8, 9] {
            let mut invalid = valid;
            invalid[index] = 255;
            assert!(parse_confirm(&invalid, 1024).is_err());
        }
    }
}
