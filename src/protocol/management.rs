//! Pure PLC control and clock service wire codecs.

use super::{Pdu, PduType, parse_userdata, userdata_request};
use crate::{
    Error, Result,
    codec::datetime::{DateTime, get_dt, set_dt},
};

/// Supported PLC PI service operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// Stop the CPU.
    Stop,
    /// Restart the CPU without a cold reset.
    HotStart,
    /// Cold restart the CPU.
    ColdStart,
    /// Compress PLC memory.
    Compress,
    /// Copy RAM contents to ROM.
    CopyRamToRom,
}

impl Control {
    /// Function code used in request and acknowledgement parameters.
    pub const fn function(self) -> u8 {
        if matches!(self, Self::Stop) {
            0x29
        } else {
            0x28
        }
    }
}

/// Build the upstream PI service parameter vector.
pub fn control_request(reference: u16, operation: Control) -> Pdu {
    let parameters = match operation {
        Control::Stop => b"\x29\0\0\0\0\0\x09P_PROGRAM".to_vec(),
        Control::HotStart => b"\x28\0\0\0\0\0\0\xfd\0\0\x09P_PROGRAM".to_vec(),
        Control::ColdStart => b"\x28\0\0\0\0\0\0\xfd\0\x02C \x09P_PROGRAM".to_vec(),
        Control::Compress => b"\x28\0\0\0\0\0\0\xfd\0\0\x05_GARB".to_vec(),
        Control::CopyRamToRom => b"\x28\0\0\0\0\0\0\xfd\0\x02EP\x05_MODU".to_vec(),
    };
    Pdu::job(reference, parameters, Vec::new())
}

/// Validate control service identity in addition to whole-PDU errors.
pub fn check_control(response: &Pdu, operation: Control) -> Result<()> {
    response.check_response()?;
    if !matches!(response.kind, PduType::Ack | PduType::AckData)
        || response.parameters != [operation.function()]
        || !response.data.is_empty()
    {
        return Err(Error::Protocol("invalid PLC control acknowledgement"));
    }
    Ok(())
}

/// Build a PLC clock read service.
pub fn clock_read_request(reference: u16) -> Result<Pdu> {
    userdata_request(reference, 7, 1, None)
}

/// Build the native ten-byte clock write layout with the upstream 0x19 prefix.
pub fn clock_write_request(reference: u16, value: DateTime) -> Result<Pdu> {
    let mut payload = [0; 10];
    payload[1] = 0x19;
    set_dt(&mut payload, 2, value)?;
    userdata_request(reference, 7, 2, Some(&payload))
}

/// Read native ten-byte or legacy eight-byte PLC clock responses.
pub fn parse_clock(response: &Pdu) -> Result<DateTime> {
    let reply = parse_userdata(response, 7, 1, false)?;
    if reply.more {
        return Err(Error::Protocol("unexpected clock continuation"));
    }
    let payload = &reply.payload;
    let result = match payload.len() {
        10 => get_dt(payload, 2),
        8 => {
            let raw = [
                payload[1],
                payload[2],
                payload[3],
                payload[4],
                payload[5],
                payload[6],
                0,
                payload[7] & 15,
            ];
            get_dt(&raw, 0)
        }
        _ => return Err(Error::Protocol("invalid PLC clock payload length")),
    };
    result.map_err(|_| Error::Protocol("invalid PLC clock timestamp"))
}
