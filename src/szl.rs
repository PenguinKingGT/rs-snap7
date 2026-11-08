//! System status list models and pure diagnostic/identification parsers.

use crate::{
    CpuState, Error, Result,
    codec::{
        self,
        datetime::{DateTime, get_dt},
    },
};

/// Complete SZL payload, excluding the outer ID/index and USERDATA headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Szl {
    /// Requested list ID.
    pub id: u16,
    /// Requested list index.
    pub index: u16,
    /// Unmodified list body, including any list-specific record header.
    pub data: Vec<u8>,
}

impl Szl {
    /// Decode a standard LengthDR/NDR record table, rejecting partial records.
    pub fn records(&self) -> Result<Vec<&[u8]>> {
        let width = usize::from(codec::get_word(&self.data, 0)?);
        let count = usize::from(codec::get_word(&self.data, 2)?);
        if width == 0 || self.data.len() != 4 + width * count {
            return Err(Error::Protocol("invalid SZL record table"));
        }
        Ok(self.data[4..].chunks_exact(width).collect())
    }
}

/// Component identification decoded from SZL 0x001c.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuInfo {
    /// Automation system name.
    pub as_name: String,
    /// Module name.
    pub module_name: String,
    /// Manufacturer copyright text.
    pub copyright: String,
    /// Module serial number.
    pub serial_number: String,
    /// CPU/module type description.
    pub module_type_name: String,
}

/// CP fields matching the pinned upstream four-u16 interpretation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpInfo {
    /// Maximum application PDU size.
    pub max_pdu_length: u16,
    /// Maximum connection count.
    pub max_connections: u16,
    /// Maximum MPI rate in the upstream field representation.
    pub max_mpi_rate: u16,
    /// Maximum bus rate in the upstream field representation.
    pub max_bus_rate: u16,
}

/// Module order code and installed firmware version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderCode {
    /// Trimmed MLFB/catalog code.
    pub code: String,
    /// Installed firmware version, preferring record 0x0007 over bootloader data.
    pub version: [u8; 3],
}

/// Five protection fields from SZL 0x0232, preserving wire order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Protection {
    /// Selector protection level (sch_schal).
    pub selector: u16,
    /// Parameterized protection level (sch_par).
    pub parameterized: u16,
    /// Effective protection level (sch_rel).
    pub effective: u16,
    /// Operating protection (bart_sch).
    pub operating: u16,
    /// Startup protection (anl_sch).
    pub startup: u16,
}

/// One upstream 20-byte diagnostic buffer record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticEntry {
    /// Raw event identifier.
    pub event_id: u16,
    /// Timestamp if its BCD/calendar fields are valid; otherwise absent.
    pub timestamp: Option<DateTime>,
    /// Uninterpreted ten-byte event details.
    pub info: [u8; 10],
}

/// Decode a complete CPU identification body; truncated fields are an error.
pub fn parse_cpu_info(data: &[u8]) -> Result<CpuInfo> {
    if data.len() < 208 {
        return Err(Error::Protocol("truncated CPU identification"));
    }
    Ok(CpuInfo {
        as_name: text(&data[6..30]),
        module_name: text(&data[40..64]),
        copyright: text(&data[108..134]),
        serial_number: text(&data[142..166]),
        module_type_name: text(&data[176..208]),
    })
}

/// Decode the upstream communication processor field layout.
pub fn parse_cp_info(data: &[u8]) -> Result<CpInfo> {
    if data.len() < 8 {
        return Err(Error::Protocol("truncated CP information"));
    }
    Ok(CpInfo {
        max_pdu_length: codec::read(data, 0)?,
        max_connections: codec::read(data, 2)?,
        max_mpi_rate: codec::read(data, 4)?,
        max_bus_rate: codec::read(data, 6)?,
    })
}

/// Decode the upstream five-word protection layout.
pub fn parse_protection(data: &[u8]) -> Result<Protection> {
    if data.len() < 10 {
        return Err(Error::Protocol("truncated protection information"));
    }
    Ok(Protection {
        selector: codec::read(data, 0)?,
        parameterized: codec::read(data, 2)?,
        effective: codec::read(data, 4)?,
        operating: codec::read(data, 6)?,
        startup: codec::read(data, 8)?,
    })
}

/// Decode structured and legacy flat order-code lists, preserving firmware priority.
pub fn parse_order_code(szl: &Szl) -> Result<OrderCode> {
    let records = szl.records()?;
    let mut code = OrderCode {
        code: String::new(),
        version: [0; 3],
    };
    let mut structured = false;
    let mut installed = false;
    for record in records {
        if record.len() < 2 {
            return Err(Error::Protocol("order-code record too short"));
        }
        let id = u16::from_be_bytes([record[0], record[1]]);
        match id {
            1 if record.len() >= 22 => {
                structured = true;
                code.code = text(&record[2..22]);
                if record.len() >= 26 && !installed && (record[23] != 0 || record[24] != 0) {
                    code.version.copy_from_slice(&record[23..26]);
                }
            }
            2 if record.len() >= 26 && code.version[0] == 0 && !installed => {
                structured = true;
                code.version.copy_from_slice(&record[23..26]);
            }
            7 if record.len() >= 28 => {
                structured = true;
                installed = true;
                code.version.copy_from_slice(&record[record.len() - 3..]);
            }
            0x81 => structured = true,
            _ => {}
        }
    }
    if !structured {
        let payload = &szl.data[4..];
        if let Some(start) = payload.windows(4).position(|b| b == b"6ES7") {
            if let Some(bytes) = payload.get(start..start + 20) {
                code.code = text(bytes);
            }
            if let Some(version) = payload.get(start + 21..start + 24) {
                code.version.copy_from_slice(version);
            }
        }
    }
    Ok(code)
}

/// Read the state byte from an SZL 0x0424 body; never invent a RUN result.
pub fn parse_cpu_state(data: &[u8]) -> Result<CpuState> {
    let state = *data
        .get(7)
        .ok_or(Error::Protocol("truncated CPU state SZL"))?;
    match state {
        0 => Ok(CpuState::Unknown),
        8 => Ok(CpuState::Run),
        3 | 4 => Ok(CpuState::Stop),
        _ => Err(Error::Protocol("unknown CPU state code")),
    }
}

/// Parse complete diagnostic records. An invalid timestamp leaves raw info accessible.
pub fn parse_diagnostic_buffer(data: &[u8]) -> Result<Vec<DiagnosticEntry>> {
    if !data.len().is_multiple_of(20) {
        return Err(Error::Protocol("partial diagnostic record"));
    }
    data.chunks_exact(20)
        .map(|record| {
            Ok(DiagnosticEntry {
                event_id: codec::read(record, 0)?,
                timestamp: get_dt(record, 2).ok(),
                info: record[10..20]
                    .try_into()
                    .map_err(|_| Error::Protocol("invalid diagnostic info"))?,
            })
        })
        .collect()
}

/// Decode upstream byte strings as Latin-1, trimming NUL/space padding.
fn text(bytes: &[u8]) -> String {
    bytes
        .iter()
        .copied()
        .map(char::from)
        .collect::<String>()
        .trim_end_matches(['\0', ' '])
        .to_owned()
}

/// One CPU force-table entry; process-image writes do not create these records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForceEntry {
    /// Input or output memory area.
    pub area: crate::Area,
    /// Byte offset in the process image.
    pub byte_offset: u16,
    /// Bit index, 0 through 7.
    pub bit: u8,
    /// Forced state.
    pub value: bool,
}

/// Parse complete eight-byte upstream force records, rejecting invalid areas/bits.
pub fn parse_force_table(data: &[u8]) -> Result<Vec<ForceEntry>> {
    if !data.len().is_multiple_of(8) {
        return Err(Error::Protocol("partial force-table record"));
    }
    data.chunks_exact(8)
        .map(|entry| {
            let area = match codec::read::<u16>(entry, 0)? {
                0x81 => crate::Area::Inputs,
                0x82 => crate::Area::Outputs,
                _ => return Err(Error::Protocol("invalid force-table area")),
            };
            if entry[4] > 7 || entry[5] > 1 {
                return Err(Error::Protocol("invalid force-table bit or value"));
            }
            Ok(ForceEntry {
                area,
                byte_offset: codec::read(entry, 2)?,
                bit: entry[4],
                value: entry[5] != 0,
            })
        })
        .collect()
}
