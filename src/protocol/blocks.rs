//! Pure block-service request builders and response validators.

use super::{Pdu, PduType, userdata_request};
use crate::{BlockType, Error, Result, blocks::CompactBlockInfo, codec};

/// Format a nine-byte S7 block file address.
pub fn block_address(kind: BlockType, number: u16, filesystem: u8) -> Vec<u8> {
    let mut address = vec![b'_', b'0', kind as u8];
    address.extend_from_slice(format!("{number:05}").as_bytes());
    address.push(filesystem);
    address
}

/// Build a block-info USERDATA query for a typed block identifier.
pub fn info_request(reference: u16, kind: BlockType, number: u16) -> Result<Pdu> {
    let mut payload = vec![0x30, kind as u8];
    payload.extend_from_slice(format!("{number:05}").as_bytes());
    payload.push(b'A');
    userdata_request(reference, 3, 3, Some(&payload))
}

/// Build the initial upload job with the nine-byte block filename.
pub fn start_upload(reference: u16, kind: BlockType, number: u16) -> Pdu {
    let mut params = vec![0x1d, 0, 0, 0, 0, 0, 0, 0, 9];
    params.extend(block_address(kind, number, b'A'));
    Pdu::job(reference, params, vec![])
}

/// Build UPLOAD or END_UPLOAD for a previously allocated one-byte upload ID.
pub fn upload_job(reference: u16, id: u8, ending: bool) -> Pdu {
    Pdu::job(
        reference,
        vec![if ending { 0x1f } else { 0x1e }, 0, 0, 0, 0, 0, 0, id],
        vec![],
    )
}

/// Validate an upload start, retaining both upload ID and advertised total size.
pub fn parse_upload_start(reply: &Pdu) -> Result<(u8, usize)> {
    reply.check_response()?;
    let p = &reply.parameters;
    if !matches!(reply.kind, PduType::Ack | PduType::AckData)
        || p.len() != 16
        || p[0] != 0x1d
        || !reply.data.is_empty()
    {
        return Err(Error::Protocol("invalid start-upload response"));
    }
    let bytes = &p[11..16];
    if !bytes.iter().all(u8::is_ascii_digit) {
        return Err(Error::Protocol("invalid upload length"));
    }
    let size = bytes.iter().fold(0, |n, b| n * 10 + usize::from(b - b'0'));
    Ok((p[7], size))
}

/// Validate one fragment and its length/marker; return payload and final-fragment flag.
pub fn parse_upload_fragment(reply: &Pdu) -> Result<(&[u8], bool)> {
    reply.check_response()?;
    if !matches!(reply.kind, PduType::Ack | PduType::AckData)
        || reply.parameters.len() != 2
        || reply.parameters[0] != 0x1e
        || reply.parameters[1] > 1
        || reply.data.len() < 4
    {
        return Err(Error::Protocol("invalid upload fragment"));
    }
    let length = usize::from(codec::read::<u16>(&reply.data, 0)?);
    if codec::read::<u16>(&reply.data, 2)? != 0xfb || length != reply.data.len() - 4 {
        return Err(Error::Protocol("invalid upload data length/marker"));
    }
    Ok((&reply.data[4..], reply.parameters[1] == 0))
}

/// Require a canonical one-function acknowledgement and no trailing service data.
pub fn check_ack(reply: &Pdu, function: u8) -> Result<()> {
    reply.check_response()?;
    if !matches!(reply.kind, PduType::Ack | PduType::AckData)
        || reply.parameters != [function]
        || !reply.data.is_empty()
    {
        return Err(Error::Protocol("invalid block service acknowledgement"));
    }
    Ok(())
}

/// Build a DB download offer with six-digit load/MC7 sizes.
pub fn download_request(reference: u16, number: u16, data: &[u8]) -> Result<Pdu> {
    if data.len() > 999999 {
        return Err(Error::InvalidInput("download exceeds six-digit size field"));
    }
    let mc7 = if data.len() >= 36 && data[2] == 1 && data[4] == 5 {
        usize::from(CompactBlockInfo::decode(data)?.mc7_size)
    } else {
        data.len()
            .saturating_sub(if data.len() >= 36 { 36 } else { 0 })
    };
    let mut params = vec![0x1a, 0, 1, 0, 0, 0, 0, 0, 9];
    params.extend(block_address(BlockType::Db, number, b'P'));
    params.push(13);
    params.extend_from_slice(format!("1{:06}{mc7:06}", data.len()).as_bytes());
    Ok(Pdu::job(reference, params, vec![]))
}

/// Validate an unsolicited PLC job before replying with its own reference number.
pub fn check_download_job(job: &Pdu, function: u8, number: u16) -> Result<()> {
    if job.kind != PduType::Job
        || job.error != 0
        || job.parameters.len() != 18
        || !job.data.is_empty()
        || job.parameters[0] != function
        || job.parameters[8] != 9
        || job.parameters[9..] != block_address(BlockType::Db, number, b'P')
    {
        return Err(Error::Protocol("invalid PLC download job"));
    }
    Ok(())
}

/// Reply to a DOWNLOAD_BLOCK job with a bounded fragment and the echoed PLC reference.
pub fn download_fragment(reference: u16, last: bool, data: &[u8]) -> Result<Pdu> {
    let length = u16::try_from(data.len())
        .map_err(|_| Error::InvalidInput("download fragment too large"))?;
    let mut bytes = length.to_be_bytes().to_vec();
    bytes.extend_from_slice(&[0, 0xfb]);
    bytes.extend_from_slice(data);
    Ok(Pdu {
        kind: PduType::AckData,
        reference,
        error: 0,
        parameters: vec![0x1b, u8::from(!last)],
        data: bytes,
    })
}

/// Reply to the PLC's DOWNLOAD_ENDED job, preserving its reference.
pub fn download_end(reference: u16) -> Pdu {
    Pdu {
        kind: PduType::AckData,
        reference,
        error: 0,
        parameters: vec![0x1c],
        data: vec![],
    }
}

/// Build the upstream _DELE PI service request.
pub fn delete_request(reference: u16, kind: BlockType, number: u16) -> Pdu {
    let block = format!("{:02X}{number:05}P", kind as u8);
    let mut params = vec![0x28, 0, 0, 0, 0, 0, 8, 5, 0];
    params.extend_from_slice(block.as_bytes());
    params.extend_from_slice(b"_DELE");
    Pdu::job(reference, params, vec![])
}
