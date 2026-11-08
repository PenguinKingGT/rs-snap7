//! I/O process-image bit operations and independent CPU force-table reads.

use super::Client;
use crate::{
    Area, Error, Result, codec,
    szl::{ForceEntry, parse_force_table},
};

impl Client {
    /// Update an input/output process-image bit under one local session guard.
    /// This is not a persistent CPU force job and may be overwritten by a scan
    /// or external writer. It does not add an entry to the CPU force table.
    pub fn force_bit(&self, area: Area, byte_offset: u32, bit: u8, value: bool) -> Result<()> {
        if !matches!(area, Area::Inputs | Area::Outputs) || bit > 7 {
            return Err(Error::InvalidInput(
                "force requires I/O area and bit 0 through 7",
            ));
        }
        let mut session = self.session()?;
        let mut bytes = session.read_area(area, 0, byte_offset, 1)?;
        codec::set_bool(&mut bytes, 0, bit, value)?;
        session.write_area(area, 0, byte_offset, &bytes)
    }

    /// Clear one process-image bit, matching upstream cancellation behavior.
    /// The PLC scan may subsequently restore its natural value.
    pub fn cancel_force(&self, area: Area, byte_offset: u32, bit: u8) -> Result<()> {
        self.force_bit(area, byte_offset, bit, false)
    }

    /// Read CPU force records from SZL 0x0025. Explicit unsupported-service codes
    /// yield an empty list; transport failures and malformed data remain errors.
    pub fn read_force_table(&self) -> Result<Vec<ForceEntry>> {
        match self.read_szl(0x25, 0) {
            Ok(szl) => parse_force_table(&szl.data),
            Err(Error::Plc(0x8104 | 0x8305 | 0x8702)) | Err(Error::Item(0x0a)) => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }
}
