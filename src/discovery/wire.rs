//! Pure Ethernet/DCP Identify codecs, including optional VLAN tags.

use super::Device;
use crate::{Error, Result};
use std::net::Ipv4Addr;

/// PROFINET DCP Identify-All multicast destination.
pub const MULTICAST: [u8; 6] = [1, 0x0e, 0xcf, 0, 0, 0];

/// Build a padded Identify-All request, or a directed Identify request.
pub fn identify_request(source: [u8; 6], target: Option<[u8; 6]>, xid: u32) -> Result<Vec<u8>> {
    if source[0] & 1 != 0
        || source == [0; 6]
        || target.is_some_and(|mac| mac[0] & 1 != 0 || mac == [0; 6])
    {
        return Err(Error::InvalidInput(
            "DCP requires unicast nonzero MAC addresses",
        ));
    }
    let mut frame = target.unwrap_or(MULTICAST).to_vec();
    frame.extend_from_slice(&source);
    frame.extend_from_slice(&[0x88, 0x92, 0xfe, 0xfe, 5, 0]);
    frame.extend_from_slice(&xid.to_be_bytes());
    frame.extend_from_slice(&[0, 0x80, 0, 4, 0xff, 0xff, 0, 0]);
    frame.resize(60, 0);
    Ok(frame)
}
/// Decode a matching Identify response; unrelated traffic returns None.
/// Unknown options are skipped by their checked, even-padded block length.
pub fn parse_response(frame: &[u8], local: [u8; 6], xid: u32) -> Result<Option<Device>> {
    if frame.len() < 14 {
        return Ok(None);
    }
    if frame[..6] != local && frame[..6] != [0xff; 6] {
        return Ok(None);
    }
    let mut ether = 12;
    for _ in 0..2 {
        if !matches!(&frame[ether..ether + 2], [0x81, 0] | [0x88, 0xa8]) {
            break;
        }
        ether += 4;
        if frame.len() < ether + 2 {
            return Err(Error::Protocol("truncated VLAN header"));
        }
    }
    if frame[ether..ether + 2] != [0x88, 0x92] {
        return Ok(None);
    }
    let p = &frame[ether + 2..];
    if p.len() < 12 {
        return Err(Error::Protocol("truncated DCP header"));
    }
    if p[..2] != [0xfe, 0xff] || p[2] != 5 || p[4..8] != xid.to_be_bytes() {
        return Ok(None);
    }
    if p[3] != 1 {
        return Err(Error::Protocol("DCP Identify response refused"));
    }
    let size = usize::from(u16::from_be_bytes([p[10], p[11]]));
    if size > 1500 || p.len() < 12 + size {
        return Err(Error::Protocol("invalid DCP data length"));
    }
    let mut device = Device {
        mac: frame[6..12]
            .try_into()
            .map_err(|_| Error::Protocol("invalid source MAC"))?,
        name: String::new(),
        family: String::new(),
        ip: None,
        netmask: None,
        gateway: None,
        vendor_id: None,
        device_id: None,
    };
    if device.mac[0] & 1 != 0 || device.mac == [0; 6] {
        return Err(Error::Protocol("invalid DCP source MAC"));
    }
    let mut cursor = 12;
    let end = 12 + size;
    let mut seen = Vec::new();
    while cursor < end {
        let header = p
            .get(cursor..cursor + 4)
            .filter(|_| cursor + 4 <= end)
            .ok_or(Error::Protocol("truncated DCP block header"))?;
        let kind = (header[0], header[1]);
        if seen.contains(&kind) {
            return Err(Error::Protocol("duplicate DCP option"));
        }
        seen.push(kind);
        let size = usize::from(u16::from_be_bytes([header[2], header[3]]));
        cursor += 4;
        let body = p
            .get(cursor..cursor + size)
            .filter(|_| size >= 2 && cursor + size <= end)
            .ok_or(Error::Protocol("invalid DCP block length"))?;
        let content = &body[2..]; // Every Identify response block begins with BlockInfo.
        match kind {
            (1, 2) if content.len() == 12 => {
                device.ip = Some(ip(&content[..4])?);
                device.netmask = Some(ip(&content[4..8])?);
                device.gateway = Some(ip(&content[8..12])?);
            }
            (1, 2) => return Err(Error::Protocol("invalid DCP IP block")),
            (2, 1) => device.family = text(content)?,
            (2, 2) => {
                if content.len() > 240 {
                    return Err(Error::Protocol("DCP station name too long"));
                }
                device.name = text(content)?;
            }
            (2, 3) if content.len() == 4 => {
                device.vendor_id = Some(u16::from_be_bytes([content[0], content[1]]));
                device.device_id = Some(u16::from_be_bytes([content[2], content[3]]));
            }
            (2, 3) => return Err(Error::Protocol("invalid DCP device ID block")),
            _ => {}
        }
        cursor += size;
        if size % 2 == 1 {
            if cursor >= end {
                return Err(Error::Protocol("missing DCP block padding"));
            }
            cursor += 1;
        }
    }
    Ok(Some(device))
}
/// Decode a four-byte network address without unchecked conversions.
fn ip(bytes: &[u8]) -> Result<Ipv4Addr> {
    let octets: [u8; 4] = bytes
        .try_into()
        .map_err(|_| Error::Protocol("invalid DCP IP field"))?;
    Ok(Ipv4Addr::from(octets))
}
/// Preserve protocol text without silently replacing invalid UTF-8.
fn text(bytes: &[u8]) -> Result<String> {
    Ok(std::str::from_utf8(bytes)
        .map_err(|_| Error::Protocol("invalid DCP text"))?
        .trim_end_matches('\0')
        .to_owned())
}
