//! PBC BSEND data and acknowledgement layouts from the pinned Python baseline.

use crate::{
    Error, Result, codec,
    protocol::{Pdu, PduType},
};

/// Build one PBC data push; payload fragmentation is deliberately not invented.
pub fn data(reference: u16, r_id: u32, payload: &[u8], pdu_limit: usize) -> Result<Pdu> {
    if payload.len() > u16::MAX as usize - 10 || payload.len() + 36 > pdu_limit {
        return Err(Error::InvalidInput(
            "Partner payload exceeds negotiated PDU",
        ));
    }
    let mut data = vec![0xff, 9];
    data.extend_from_slice(&((payload.len() + 10) as u16).to_be_bytes());
    data.extend_from_slice(&[0x12, 6, 0x13, 0]);
    data.extend_from_slice(&r_id.to_be_bytes());
    data.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    data.extend_from_slice(payload);
    Ok(Pdu {
        kind: PduType::UserData,
        reference,
        error: 0,
        parameters: vec![0, 1, 0x12, 8, 0x12, 0x46, 1, 0, 0, 0, 0, 0],
        data,
    })
}
/// Validate an entire PBC envelope; return R-ID and application bytes.
pub fn parse_data(pdu: &Pdu) -> Result<(u32, Vec<u8>)> {
    if pdu.kind != PduType::UserData
        || pdu.parameters != [0, 1, 0x12, 8, 0x12, 0x46, 1, 0, 0, 0, 0, 0]
        || pdu.data.len() < 14
        || pdu.data[..2] != [0xff, 9]
        || codec::read::<u16>(&pdu.data, 2)? as usize != pdu.data.len() - 4
        || pdu.data[4..8] != [0x12, 6, 0x13, 0]
        || codec::read::<u16>(&pdu.data, 12)? as usize != pdu.data.len() - 14
    {
        return Err(Error::Protocol("invalid Partner data envelope"));
    }
    Ok((codec::read(&pdu.data, 8)?, pdu.data[14..].to_vec()))
}
/// Echo the received push reference, independently of our own send sequence.
pub fn ack(reference: u16) -> Pdu {
    Pdu {
        kind: PduType::UserData,
        reference,
        error: 0,
        parameters: vec![0, 1, 0x12, 8, 0x12, 0x86, 1, 0, 0, 0, 0, 0],
        data: vec![0x0a, 0, 0, 0],
    }
}
/// Require a canonical matching acknowledgement, preserving peer error codes.
pub fn check_ack(pdu: &Pdu, reference: u16) -> Result<()> {
    crate::protocol::validate_reference(reference, pdu.reference)?;
    let p = &pdu.parameters;
    if pdu.kind != PduType::UserData
        || p.len() != 12
        || p[..7] != [0, 1, 0x12, 8, 0x12, 0x86, 1]
        || p[8..10] != [0, 0]
        || pdu.data != [0x0a, 0, 0, 0]
    {
        return Err(Error::Protocol("invalid Partner acknowledgement"));
    }
    let code = codec::read::<u16>(p, 10)?;
    if code != 0 {
        return Err(Error::Plc(code));
    }
    Ok(())
}
