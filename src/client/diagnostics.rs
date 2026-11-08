//! Bounded SZL collection and typed diagnostic operations.

use super::Client;
use crate::{
    CpuState, Error, Result, protocol,
    szl::{self, CpInfo, CpuInfo, DiagnosticEntry, OrderCode, Protection, Szl},
};

impl Client {
    /// Read a complete system status list, preserving its unmodified list body.
    /// ID/index, fragment count, total payload (1 MiB), and operation deadline are checked.
    pub fn read_szl(&self, id: u16, index: u16) -> Result<Szl> {
        let mut session = self.session()?;
        let deadline = session.deadline();
        let mut payload = Vec::from(id.to_be_bytes());
        payload.extend_from_slice(&index.to_be_bytes());
        let response = session.transact_until(
            |reference| protocol::userdata_request(reference, 4, 1, Some(&payload)),
            deadline,
        )?;
        let mut fragment = session.validate(protocol::parse_userdata(&response, 4, 1, false))?;
        if fragment.payload.len() < 4 || fragment.payload[..4] != payload {
            return session.validate(Err(Error::Protocol("SZL identity mismatch")));
        }
        let mut data = fragment.payload[4..].to_vec();
        for _ in 0..100 {
            if !fragment.more {
                return Ok(Szl { id, index, data });
            }
            let response = session.transact_until(
                |reference| {
                    Ok(protocol::userdata_followup(
                        reference,
                        4,
                        1,
                        fragment.sequence,
                    ))
                },
                deadline,
            )?;
            fragment = session.validate(protocol::parse_userdata(&response, 4, 1, false))?;
            if fragment.payload.is_empty() && fragment.more
                || data.len() + fragment.payload.len() > 1024 * 1024
            {
                return session.validate(Err(Error::Protocol("SZL continuation exceeds limits")));
            }
            data.extend_from_slice(&fragment.payload);
        }
        if fragment.more {
            return session.validate(Err(Error::Protocol("SZL fragment limit reached")));
        }
        Ok(Szl { id, index, data })
    }

    /// Read the available SZL ID list as the upstream raw list body.
    pub fn read_szl_list(&self) -> Result<Vec<u8>> {
        Ok(self.read_szl(0, 0)?.data)
    }
    /// Read CPU component identification.
    pub fn get_cpu_info(&self) -> Result<CpuInfo> {
        szl::parse_cpu_info(&self.read_szl(0x001c, 0)?.data)
    }
    /// Read actual CPU state through SZL 0x0424, unlike upstream's constant RUN stub.
    pub fn get_cpu_state(&self) -> Result<CpuState> {
        szl::parse_cpu_state(&self.read_szl(0x0424, 0)?.data)
    }
    /// Read communication processor information using the upstream field layout.
    pub fn get_cp_info(&self) -> Result<CpInfo> {
        szl::parse_cp_info(&self.read_szl(0x0131, 0)?.data)
    }
    /// Read module order code, preferring installed firmware over bootloader version.
    pub fn get_order_code(&self) -> Result<OrderCode> {
        szl::parse_order_code(&self.read_szl(0x0011, 0)?)
    }
    /// Read the CPU protection fields.
    pub fn get_protection(&self) -> Result<Protection> {
        szl::parse_protection(&self.read_szl(0x0232, 0)?.data)
    }
    /// Read diagnostic events in wire order (upstream returns newest first).
    pub fn read_diagnostic_buffer(&self) -> Result<Vec<DiagnosticEntry>> {
        szl::parse_diagnostic_buffer(&self.read_szl(0x00a0, 0)?.data)
    }
}
