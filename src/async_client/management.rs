//! PLC control and clock operations over the shared session.

use super::AsyncClient;
use crate::{
    Error, Result,
    codec::datetime::{DateTime, get_ldt},
    protocol::{
        self,
        management::{self, Control},
    },
};
use std::time::{SystemTime, UNIX_EPOCH};

impl AsyncClient {
    /// Stop the PLC CPU. The operation is never automatically replayed.
    pub async fn plc_stop(&self) -> Result<()> {
        self.observe("plc_stop", async { self.control(Control::Stop).await })
            .await
    }
    /// Hot-start the PLC CPU without automatically replaying the operation.
    pub async fn plc_hot_start(&self) -> Result<()> {
        self.observe("plc_hot_start", async {
            self.control(Control::HotStart).await
        })
        .await
    }
    /// Cold-start the PLC CPU without automatically replaying the operation.
    pub async fn plc_cold_start(&self) -> Result<()> {
        self.observe("plc_cold_start", async {
            self.control(Control::ColdStart).await
        })
        .await
    }
    /// Compress PLC memory using the configured operation timeout.
    pub async fn compress(&self) -> Result<()> {
        self.observe("compress", async { self.control(Control::Compress).await })
            .await
    }
    /// Copy PLC RAM to ROM using the configured operation timeout.
    pub async fn copy_ram_to_rom(&self) -> Result<()> {
        self.observe("copy_ram_to_rom", async {
            self.control(Control::CopyRamToRom).await
        })
        .await
    }

    /// Read the PLC's timezone-free clock value, including milliseconds.
    pub async fn get_plc_datetime(&self) -> Result<DateTime> {
        self.observe("get_plc_datetime", async {
            let mut session = self.session().await;
            let reply = session.transact(management::clock_read_request).await?;
            session.validate(management::parse_clock(&reply))
        })
        .await
    }

    /// Set the PLC clock in its configured local calendar convention (1990–2089).
    pub async fn set_plc_datetime(&self, value: DateTime) -> Result<()> {
        self.observe("set_plc_datetime", async {
            // Validate before locking or touching the network.
            management::clock_write_request(0, value)?;
            let mut session = self.session().await;
            let reply = session
                .transact(|reference| management::clock_write_request(reference, value))
                .await?;
            let ack = session.validate(protocol::parse_userdata(&reply, 7, 2, true))?;
            if ack.more || !ack.payload.is_empty() {
                return session
                    .validate(Err(Error::Protocol("invalid clock write acknowledgement")));
            }
            Ok(())
        })
        .await
    }

    /// Set the PLC clock to current UTC system time. Unlike Python's local-time
    /// helper this has no implicit host timezone; supply `DateTime` for local PLC time.
    pub async fn set_plc_system_datetime(&self) -> Result<()> {
        self.observe("set_plc_system_datetime", async {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| Error::InvalidInput("system time predates epoch"))?
                .as_nanos();
            let nanos = u64::try_from(nanos)
                .map_err(|_| Error::InvalidInput("system time outside LDT range"))?;
            self.set_plc_datetime(get_ldt(&nanos.to_be_bytes(), 0)?)
                .await
        })
        .await
    }

    /// Hold one exclusive session through request construction and service validation.
    async fn control(&self, operation: Control) -> Result<()> {
        let mut session = self.session().await;
        let reply = session
            .transact(|reference| Ok(management::control_request(reference, operation)))
            .await?;
        session.validate(management::check_control(&reply, operation))
    }
}
