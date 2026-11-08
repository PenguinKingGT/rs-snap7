//! Pure S7 PDU framing, negotiation, and memory-service wire operations.
//!
//! Wire layouts follow the pinned python-snap7 implementation. No function in
//! this module opens sockets or changes session state.

use crate::{Address, Error, Result, WordLen};

/// Maximum variables in one classic S7 read/write job.
pub const MAX_ITEMS: usize = 20;

/// S7 message header format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PduType {
    /// Client job, with a ten-byte header.
    Job = 1,
    /// Acknowledgement, with a twelve-byte header.
    Ack = 2,
    /// Acknowledgement with data, with a twelve-byte header.
    AckData = 3,
    /// User-data service, with a ten-byte header.
    UserData = 7,
}

/// Owned S7 PDU; parameter and data sections are preserved for service decoders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pdu {
    /// Message format.
    pub kind: PduType,
    /// Request/response correlation reference.
    pub reference: u16,
    /// Whole acknowledgement error word, zero for success.
    pub error: u16,
    /// Uninterpreted service parameter bytes.
    pub parameters: Vec<u8>,
    /// Uninterpreted service data bytes.
    pub data: Vec<u8>,
}

impl Pdu {
    /// Construct a job with a caller-owned session reference.
    pub fn job(reference: u16, parameters: Vec<u8>, data: Vec<u8>) -> Self {
        Self {
            kind: PduType::Job,
            reference,
            error: 0,
            parameters,
            data,
        }
    }

    /// Encode exactly one bounded PDU, validating all 16-bit length fields.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let header = if matches!(self.kind, PduType::Ack | PduType::AckData) {
            12
        } else {
            10
        };
        if self.parameters.len() + self.data.len() > 65535 - header
            || (header == 10 && self.error != 0)
        {
            return Err(Error::InvalidInput("invalid PDU length or header error"));
        }
        let mut output = vec![0x32, self.kind as u8, 0, 0];
        output.extend_from_slice(&self.reference.to_be_bytes());
        output.extend_from_slice(&(self.parameters.len() as u16).to_be_bytes());
        output.extend_from_slice(&(self.data.len() as u16).to_be_bytes());
        if header == 12 {
            output.extend_from_slice(&self.error.to_be_bytes());
        }
        output.extend_from_slice(&self.parameters);
        output.extend_from_slice(&self.data);
        Ok(output)
    }

    /// Decode exact section lengths, rejecting truncation, trailing bytes, and reserved fields.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 10 || bytes.len() > 65535 || bytes[0] != 0x32 || bytes[2..4] != [0, 0] {
            return Err(Error::Protocol("invalid S7 header"));
        }
        let kind = match bytes[1] {
            1 => PduType::Job,
            2 => PduType::Ack,
            3 => PduType::AckData,
            7 => PduType::UserData,
            _ => return Err(Error::Protocol("unknown PDU type")),
        };
        let header = if matches!(kind, PduType::Ack | PduType::AckData) {
            12
        } else {
            10
        };
        let parameters = usize::from(u16::from_be_bytes([bytes[6], bytes[7]]));
        let data = usize::from(u16::from_be_bytes([bytes[8], bytes[9]]));
        if bytes.len() != header + parameters + data {
            return Err(Error::Protocol("S7 section lengths do not match PDU"));
        }
        Ok(Self {
            kind,
            reference: u16::from_be_bytes([bytes[4], bytes[5]]),
            error: if header == 12 {
                u16::from_be_bytes([bytes[10], bytes[11]])
            } else {
                0
            },
            parameters: bytes[header..header + parameters].to_vec(),
            data: bytes[header + parameters..].to_vec(),
        })
    }

    /// Validate response format and the entire error word, including low-byte-only errors.
    pub fn check_response(&self) -> Result<()> {
        if self.kind == PduType::Job {
            return Err(Error::Protocol("received a job instead of a response"));
        }
        if self.error != 0 {
            return Err(Error::Plc(self.error));
        }
        Ok(())
    }
}

/// Compare references using modulo-65536 ordering, including sequence rollover.
pub fn validate_reference(expected: u16, received: u16) -> Result<()> {
    if expected == received {
        return Ok(());
    }
    if received.wrapping_sub(expected) as i16 > 0 {
        Err(Error::LostReference { expected, received })
    } else {
        Err(Error::StaleReference { expected, received })
    }
}

/// Requested/negotiated S7 application limits, distinct from COTP TPDU size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Negotiated {
    /// Maximum outstanding caller jobs.
    pub caller_jobs: u16,
    /// Maximum outstanding callee jobs.
    pub callee_jobs: u16,
    /// Maximum S7 PDU bytes, including S7 headers.
    pub pdu_length: u16,
}

/// Build a standard setup-communication job with one outstanding request.
pub fn setup_request(reference: u16, requested: u16) -> Result<Pdu> {
    if requested < 128 {
        return Err(Error::InvalidInput("requested PDU below minimum"));
    }
    let mut parameters = vec![0xf0, 0, 0, 1, 0, 1];
    parameters.extend_from_slice(&requested.to_be_bytes());
    Ok(Pdu::job(reference, parameters, Vec::new()))
}

/// Validate a matching setup response and peer limits without silently guessing defaults.
pub fn parse_setup(response: &Pdu, requested: u16) -> Result<Negotiated> {
    response.check_response()?;
    let p = &response.parameters;
    if response.kind != PduType::AckData
        || p.len() != 8
        || p[..2] != [0xf0, 0]
        || !response.data.is_empty()
    {
        return Err(Error::Protocol("invalid setup response"));
    }
    let limits = Negotiated {
        caller_jobs: u16::from_be_bytes([p[2], p[3]]),
        callee_jobs: u16::from_be_bytes([p[4], p[5]]),
        pdu_length: u16::from_be_bytes([p[6], p[7]]),
    };
    if limits.caller_jobs == 0
        || limits.callee_jobs == 0
        || !(128..=requested).contains(&limits.pdu_length)
    {
        return Err(Error::Protocol("invalid negotiated S7 limits"));
    }
    Ok(limits)
}

/// Construct a multi-variable read job; session planning enforces negotiated budgets.
pub fn read_request(reference: u16, addresses: &[Address]) -> Result<Pdu> {
    validate_count(addresses.len())?;
    let mut params = vec![4, addresses.len() as u8];
    for address in addresses {
        params.extend_from_slice(&address.encode());
    }
    Ok(Pdu::job(reference, params, Vec::new()))
}

/// Construct a write job, validating payload widths and inter-item alignment.
pub fn write_request(reference: u16, items: &[(Address, &[u8])]) -> Result<Pdu> {
    validate_count(items.len())?;
    let mut params = vec![5, items.len() as u8];
    let mut data = Vec::new();
    for (index, (address, value)) in items.iter().enumerate() {
        if value.len() != address.byte_len() {
            return Err(Error::InvalidInput("write payload width mismatch"));
        }
        if address.word_len() == WordLen::Bit && value.iter().any(|b| *b > 1) {
            return Err(Error::InvalidInput("bit payload must contain zero or one"));
        }
        params.extend_from_slice(&address.encode());
        let transport = match address.word_len() {
            WordLen::Bit => 3,
            WordLen::Int | WordLen::DInt => 5,
            WordLen::Real => 7,
            WordLen::Char
            | WordLen::Counter
            | WordLen::Timer
            | WordLen::Counter200
            | WordLen::Timer200 => 9,
            _ => 4,
        };
        let length = value
            .len()
            .checked_mul(if transport == 4 || transport == 5 {
                8
            } else {
                1
            })
            .and_then(|v| u16::try_from(v).ok())
            .ok_or(Error::InvalidInput("write item exceeds length field"))?;
        data.extend_from_slice(&[0, transport]);
        data.extend_from_slice(&length.to_be_bytes());
        data.extend_from_slice(value);
        if index + 1 != items.len() && value.len() % 2 == 1 {
            data.push(0);
        }
    }
    Ok(Pdu::job(reference, params, data))
}

/// Parse read outcomes independently so one PLC item error does not erase other results.
pub fn parse_read(response: &Pdu, addresses: &[Address]) -> Result<Vec<Result<Vec<u8>>>> {
    check_area(response, 4, addresses.len())?;
    let mut offset = 0;
    let mut outcomes = Vec::with_capacity(addresses.len());
    for (index, address) in addresses.iter().enumerate() {
        let head = response
            .data
            .get(offset..offset + 4)
            .ok_or(Error::Protocol("truncated read item header"))?;
        let code = head[0];
        let transport = head[1];
        let length = usize::from(u16::from_be_bytes([head[2], head[3]]));
        offset += 4;
        let bytes = match transport {
            0 if code != 0xff && length == 0 => 0,
            3 if address.word_len() == WordLen::Bit => length,
            4 | 5 if length % 8 == 0 => length / 8,
            7 | 9 => length,
            _ => return Err(Error::Protocol("invalid read transport or length units")),
        };
        let payload = response
            .data
            .get(offset..offset + bytes)
            .ok_or(Error::Protocol("truncated read payload"))?;
        if code == 0xff {
            if payload.len() != address.byte_len() {
                return Err(Error::Protocol("read item size mismatch"));
            }
            outcomes.push(Ok(payload.to_vec()));
        } else {
            if bytes != 0 {
                return Err(Error::Protocol("failed read item contains payload"));
            }
            outcomes.push(Err(Error::Item(code)));
        }
        offset += bytes;
        if index + 1 < addresses.len() && bytes % 2 == 1 {
            if response.data.get(offset) != Some(&0) {
                return Err(Error::Protocol("missing read alignment byte"));
            }
            offset += 1;
        }
    }
    if offset != response.data.len() {
        return Err(Error::Protocol("trailing read data"));
    }
    Ok(outcomes)
}

/// Parse one acknowledgement code per write item, preserving mixed outcomes.
pub fn parse_write(response: &Pdu, count: usize) -> Result<Vec<Result<()>>> {
    check_area(response, 5, count)?;
    if response.data.len() != count {
        return Err(Error::Protocol("write acknowledgement count mismatch"));
    }
    Ok(response
        .data
        .iter()
        .map(|code| {
            if *code == 0xff {
                Ok(())
            } else {
                Err(Error::Item(*code))
            }
        })
        .collect())
}

/// Reject empty/oversized packet-level variable lists.
fn validate_count(count: usize) -> Result<()> {
    if !(1..=MAX_ITEMS).contains(&count) {
        return Err(Error::InvalidInput("job requires 1 through 20 variables"));
    }
    Ok(())
}

/// Match the complete service parameters, not just the first return code.
fn check_area(response: &Pdu, function: u8, count: usize) -> Result<()> {
    validate_count(count)?;
    response.check_response()?;
    if !matches!(response.kind, PduType::Ack | PduType::AckData)
        || response.parameters != [function, count as u8]
    {
        return Err(Error::Protocol(
            "response service or variable count mismatch",
        ));
    }
    Ok(())
}

/// Build an initial USERDATA request with an optional octet-string payload.
pub fn userdata_request(
    reference: u16,
    group: u8,
    subfunction: u8,
    payload: Option<&[u8]>,
) -> Result<Pdu> {
    if !(1..=15).contains(&group) {
        return Err(Error::InvalidInput("invalid USERDATA group"));
    }
    let mut data = if payload.is_some() {
        vec![0xff, 9]
    } else {
        vec![0x0a, 0]
    };
    let payload = payload.unwrap_or_default();
    let length = u16::try_from(payload.len())
        .map_err(|_| Error::InvalidInput("USERDATA payload too large"))?;
    data.extend_from_slice(&length.to_be_bytes());
    data.extend_from_slice(payload);
    Ok(Pdu {
        kind: PduType::UserData,
        reference,
        error: 0,
        parameters: vec![0, 1, 0x12, 4, 0x11, 0x40 | group, subfunction, 0],
        data,
    })
}

/// A validated user-data fragment, retaining continuation metadata.
#[derive(Debug, Clone)]
pub struct UserData {
    /// Reference supplied in a continuation request.
    pub sequence: u8,
    /// Data-unit identifier supplied by the peer.
    pub unit_reference: u8,
    /// Whether another fragment must be requested.
    pub more: bool,
    /// Octet-string service payload without the data header.
    pub payload: Vec<u8>,
}

/// Validate service identity, parameter errors, payload lengths, and optional null ACK.
pub fn parse_userdata(
    response: &Pdu,
    group: u8,
    subfunction: u8,
    null_ack: bool,
) -> Result<UserData> {
    response.check_response()?;
    let p = &response.parameters;
    if response.kind != PduType::UserData
        || p.len() != 12
        || p[..5] != [0, 1, 0x12, 8, 0x12]
        || p[5] != 0x80 | group
        || p[6] != subfunction
        || p[9] > 1
    {
        return Err(Error::Protocol("invalid USERDATA response parameters"));
    }
    let code = u16::from_be_bytes([p[10], p[11]]);
    if code != 0 {
        return Err(if group == 5 {
            Error::Authentication(code)
        } else {
            Error::Plc(code)
        });
    }
    let d = &response.data;
    if d.len() < 4 {
        return Err(Error::Protocol("missing USERDATA data header"));
    }
    let size = usize::from(u16::from_be_bytes([d[2], d[3]]));
    if d.len() != 4 + size {
        return Err(Error::Protocol("invalid USERDATA data length"));
    }
    let is_null = d.as_slice() == [0x0a, 0, 0, 0];
    if !(d[0] == 0xff && d[1] == 9 || null_ack && is_null) {
        return Err(Error::Item(d[0]));
    }
    Ok(UserData {
        sequence: p[7],
        unit_reference: p[8],
        more: p[9] == 1,
        payload: d[4..].to_vec(),
    })
}

/// Encode a one-to-eight byte ASCII S7 session password using chained XOR.
/// This is protocol obfuscation, not encryption; callers must protect the network.
pub fn encode_password(password: &str) -> Result<[u8; 8]> {
    if !password.is_ascii() || !(1..=8).contains(&password.len()) {
        return Err(Error::InvalidInput(
            "password requires one through eight ASCII characters",
        ));
    }
    let mut encoded = [0; 8];
    for index in 0..8 {
        let byte = password.as_bytes().get(index).copied().unwrap_or(b' ');
        encoded[index] = byte ^ 0x55 ^ if index >= 2 { encoded[index - 2] } else { 0 };
    }
    Ok(encoded)
}
