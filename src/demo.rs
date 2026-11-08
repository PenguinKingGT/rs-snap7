//! Live host metrics published in an emulated DB1 with writable demo DB2.
//!
//! Offsets match the upstream demo. Unsupported temperature/fan sensors are zero;
//! byte 37 bit 0 explicitly indicates whether temperature was available.

use crate::{
    Area, Result, codec,
    server::{Memory, Server, ServerConfig},
};
use std::time::Instant;

/// A live sampler and its owned server. Drop shuts down the server.
pub struct Demo {
    server: Server,
    sensors: Memory,
    system: sysinfo::System,
    disks: sysinfo::Disks,
    networks: sysinfo::Networks,
    components: sysinfo::Components,
    previous: Instant,
}
impl Demo {
    /// Start DB1 (40 sensor bytes) and DB2 (64 writable control bytes).
    pub fn start(config: ServerConfig) -> Result<Self> {
        let server = Server::start(config)?;
        let sensors = server.register_area(Area::Db, 1, vec![0; 40])?;
        sensors.set_read_only(true);
        server.register_area(Area::Db, 2, vec![0; 64])?;
        Ok(Self {
            server,
            sensors,
            system: sysinfo::System::new_all()
                .map_err(|error| crate::Error::Io(std::io::Error::other(error)))?,
            disks: sysinfo::Disks::new_with_refreshed_list()
                .map_err(|error| crate::Error::Io(std::io::Error::other(error)))?,
            networks: sysinfo::Networks::new_with_refreshed_list()
                .map_err(|error| crate::Error::Io(std::io::Error::other(error)))?,
            components: sysinfo::Components::new_with_refreshed_list()
                .map_err(|error| crate::Error::Io(std::io::Error::other(error)))?,
            previous: Instant::now(),
        })
    }
    /// Access the running emulator and its bound address.
    pub fn server(&self) -> &Server {
        &self.server
    }
    /// Refresh counters and update all sensor bytes under one memory guard.
    /// Call at intervals of at least 250 ms for useful CPU/rate readings.
    pub fn sample(&mut self) -> Result<()> {
        self.system.refresh_all();
        self.disks.refresh(true);
        self.networks.refresh(true);
        self.components.refresh(true);
        let elapsed = self.previous.elapsed().as_secs_f64().max(0.001);
        self.previous = Instant::now();
        let cpu = self.system.global_cpu_usage();
        let memory = if self.system.total_memory() > 0 {
            self.system.used_memory() as f64 / self.system.total_memory() as f64 * 100.0
        } else {
            0.0
        };
        let disk_read = self
            .disks
            .iter()
            .map(|disk| disk.usage().read_bytes)
            .sum::<u64>() as f64
            / elapsed
            / 1_000_000.0;
        let disk_write = self
            .disks
            .iter()
            .map(|disk| disk.usage().written_bytes)
            .sum::<u64>() as f64
            / elapsed
            / 1_000_000.0;
        let rx = self
            .networks
            .values()
            .map(|network| network.received())
            .sum::<u64>() as f64
            / elapsed
            / 1_000_000.0;
        let tx = self
            .networks
            .values()
            .map(|network| network.transmitted())
            .sum::<u64>() as f64
            / elapsed
            / 1_000_000.0;
        let temperature = self
            .components
            .iter()
            .filter_map(|component| component.temperature())
            .filter(|v| v.is_finite())
            .max_by(f32::total_cmp);
        self.sensors.with_mut(|bytes| -> Result<()> {
            for (offset, value) in [
                (0, cpu),
                (4, memory as f32),
                (8, disk_read as f32),
                (12, disk_write as f32),
                (16, rx as f32),
                (20, tx as f32),
                (24, temperature.unwrap_or(0.0)),
                (28, 0.0),
            ] {
                codec::write(bytes, offset, value)?;
            }
            codec::write(
                bytes,
                32,
                sysinfo::System::uptime()
                    .map_err(|error| crate::Error::Io(std::io::Error::other(error)))?
                    .min(i32::MAX as u64) as i32,
            )?;
            codec::set_bool(bytes, 36, 0, temperature.is_some_and(|v| v > 75.0))?;
            codec::set_bool(bytes, 36, 1, cpu > 80.0)?;
            codec::set_bool(bytes, 36, 2, disk_read + disk_write > 50.0)?;
            codec::set_bool(bytes, 37, 0, temperature.is_some())?;
            Ok(())
        })??;
        Ok(())
    }
}
