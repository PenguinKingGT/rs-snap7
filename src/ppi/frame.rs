//! Strict SD1/SD2/SD3/SC framing and additive checksums.

use crate::{Error, Result};

/// Decoded PPI frame; SC has no addressed fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// Single-byte E5 acknowledgement.
    Ack,
    /// Addressed SD1/SD2/SD3 frame.
    Data {
        /// Start delimiter: 10, 68 or A2 hexadecimal.
        delimiter: u8,
        /// Destination station, 0–126.
        destination: u8,
        /// Source station, 0–126.
        source: u8,
        /// Link control byte.
        control: u8,
        /// Empty for SD1, at most 246 bytes for SD2, exactly eight for SD3.
        payload: Vec<u8>,
    },
}
/// Construct a checksummed frame. All lengths/stations are validated before encoding.
pub fn encode(
    delimiter: u8,
    destination: u8,
    source: u8,
    control: u8,
    payload: &[u8],
) -> Result<Vec<u8>> {
    if destination > 126 || source > 126 {
        return Err(Error::InvalidInput("PPI station must be 0 through 126"));
    }
    let mut bytes = match delimiter {
        0x10 if payload.is_empty() => vec![0x10],
        0x68 if payload.len() <= 246 => vec![
            0x68,
            (payload.len() + 3) as u8,
            (payload.len() + 3) as u8,
            0x68,
        ],
        0xa2 if payload.len() == 8 => vec![0xa2],
        _ => {
            return Err(Error::InvalidInput(
                "invalid PPI delimiter or payload length",
            ));
        }
    };
    let mut body = vec![destination, source, control];
    body.extend_from_slice(payload);
    let checksum = body.iter().fold(0u8, |n, b| n.wrapping_add(*b));
    bytes.extend(body);
    bytes.extend_from_slice(&[checksum, 0x16]);
    Ok(bytes)
}
/// Decode exactly one frame, rejecting inconsistent lengths, stations and checksums.
pub fn decode(bytes: &[u8]) -> Result<Frame> {
    if bytes == [0xe5] {
        return Ok(Frame::Ack);
    }
    let (delimiter, body) = match bytes.first() {
        Some(0x10) if bytes.len() == 6 => (0x10, &bytes[1..4]),
        Some(0xa2) if bytes.len() == 14 => (0xa2, &bytes[1..12]),
        Some(0x68)
            if bytes.len() >= 9
                && bytes[1] == bytes[2]
                && bytes[3] == 0x68
                && (3..=249).contains(&bytes[1])
                && bytes.len() == usize::from(bytes[1]) + 6 =>
        {
            (0x68, &bytes[4..bytes.len() - 2])
        }
        _ => return Err(Error::Protocol("invalid PPI frame length or delimiter")),
    };
    if bytes[bytes.len() - 1] != 0x16
        || bytes[bytes.len() - 2] != body.iter().fold(0u8, |n, b| n.wrapping_add(*b))
        || body[0] > 126
        || body[1] > 126
    {
        return Err(Error::Protocol(
            "invalid PPI checksum, end delimiter or station",
        ));
    }
    Ok(Frame::Data {
        delimiter,
        destination: body[0],
        source: body[1],
        control: body[2],
        payload: body[3..].to_vec(),
    })
}
