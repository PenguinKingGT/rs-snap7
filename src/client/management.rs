//! PLC control and clock operations over the shared session.

use super::Client;
use crate::{
    Error, Result,
    codec::datetime::{DateTime, get_ldt},
    protocol::{
        self,
        management::{self, Control},
    },
};
use std::time::{SystemTime, UNIX_EPOCH};

impl Client {
    /// Stop the PLC CPU. The operation is never automatically replayed.
    pub fn plc_stop(&self) -> Result<()> {
        self.observe("plc_stop", || self.control(Control::Stop))
    }
    /// Hot-start the PLC CPU without automatically replaying the operation.
    pub fn plc_hot_start(&self) -> Result<()> {
        self.observe("plc_hot_start", || self.control(Control::HotStart))
    }
    /// Cold-start the PLC CPU without automatically replaying the operation.
    pub fn plc_cold_start(&self) -> Result<()> {
        self.observe("plc_cold_start", || self.control(Control::ColdStart))
    }
    /// Compress PLC memory using the configured operation timeout.
    pub fn compress(&self) -> Result<()> {
        self.observe("compress", || self.control(Control::Compress))
    }
    /// Copy PLC RAM to ROM using the configured operation timeout.
    pub fn copy_ram_to_rom(&self) -> Result<()> {
        self.observe("copy_ram_to_rom", || self.control(Control::CopyRamToRom))
    }

    /// Read the PLC's timezone-free clock value, including milliseconds.
    pub fn get_plc_datetime(&self) -> Result<DateTime> {
        self.observe("get_plc_datetime", || {
            let mut session = self.session()?;
            let reply = session.transact(management::clock_read_request)?;
            session.validate(management::parse_clock(&reply))
        })
    }

    /// Set the PLC clock in its configured local calendar convention (1990–2089).
    pub fn set_plc_datetime(&self, value: DateTime) -> Result<()> {
        self.observe("set_plc_datetime", || {
            // Validate before locking or touching the network.
            management::clock_write_request(0, value)?;
            let mut session = self.session()?;
            session.transact_checked(
                |reference| management::clock_write_request(reference, value),
                |reply| protocol::check_userdata_ack(reply, 7, 2),
            )
        })
    }

    /// Set the PLC clock to current UTC system time. Unlike Python's local-time
    /// helper this has no implicit host timezone; supply `DateTime` for local PLC time.
    pub fn set_plc_system_datetime(&self) -> Result<()> {
        self.observe("set_plc_system_datetime", || {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| Error::InvalidInput("system time predates epoch"))?
                .as_nanos();
            let nanos = u64::try_from(nanos)
                .map_err(|_| Error::InvalidInput("system time outside LDT range"))?;
            self.set_plc_datetime(get_ldt(&nanos.to_be_bytes(), 0)?)
        })
    }

    /// Hold one exclusive session through request construction and service validation.
    fn control(&self, operation: Control) -> Result<()> {
        let mut session = self.session()?;
        session.transact_checked(
            |reference| Ok(management::control_request(reference, operation)),
            |reply| management::check_control(reply, operation),
        )
    }
}
