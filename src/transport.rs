//! Advanced ISO-on-TCP transport, independent of S7 application operations.
//!
//! Each send/receive has a total deadline. Failed exchanges close the socket so
//! incomplete messages cannot become the next operation's response.

use crate::{Error, Result};
use std::net::SocketAddr;
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

#[cfg(any(feature = "blocking", feature = "server", feature = "partner"))]
pub mod blocking;
mod native;
pub(crate) use native::AsyncConnection;
pub use native::Connection;
