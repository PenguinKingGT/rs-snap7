//! Independent PROFINET DCP Identify discovery; no S7 TCP broadcast is involved.
//!
//! Raw Ethernet is implemented on Linux/macOS through pnet_datalink. Other
//! platforms can supply FrameIo; constructing the built-in adapter returns
//! Unsupported. Raw adapters require OS permission to open their network device.

mod raw;
pub mod wire;
use crate::{Error, Result};
pub use raw::RawEthernet;
use std::{
    collections::BTreeMap,
    net::Ipv4Addr,
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant},
};

/// Identified station metadata; absent IP/device options remain None.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// Station name.
    pub name: String,
    /// Source Ethernet address.
    pub mac: [u8; 6],
    /// IPv4 address, when advertised.
    pub ip: Option<Ipv4Addr>,
    /// Network mask.
    pub netmask: Option<Ipv4Addr>,
    /// Default gateway.
    pub gateway: Option<Ipv4Addr>,
    /// Device family/type string.
    pub family: String,
    /// Advertised vendor ID.
    pub vendor_id: Option<u16>,
    /// Advertised device ID.
    pub device_id: Option<u16>,
}
/// Ethernet seam for deterministic testing or application-owned capture adapters.
pub trait FrameIo {
    /// Local source MAC.
    fn mac(&self) -> [u8; 6];
    /// Send a complete Ethernet frame by the absolute deadline.
    fn send(&mut self, frame: &[u8], deadline: Instant) -> Result<()>;
    /// Return one captured frame; None means the receive window expired.
    fn receive(&mut self, deadline: Instant) -> Result<Option<Vec<u8>>>;
}
/// Resource bounds for one discovery window.
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Total send/listen window, greater than zero and at most sixty seconds.
    pub timeout: Duration,
    /// Maximum distinct stations, 1–4096.
    pub max_devices: usize,
    /// Maximum inspected frames, including unrelated traffic.
    pub max_frames: usize,
    /// Transaction identifier, customizable for deterministic capture tests.
    pub xid: u32,
}
impl Default for DiscoveryConfig {
    /// Five-second window with a process-unique incrementing transaction ID.
    fn default() -> Self {
        static XID: AtomicU32 = AtomicU32::new(0x52530000);
        Self {
            timeout: Duration::from_secs(5),
            max_devices: 1024,
            max_frames: 100_000,
            xid: XID.fetch_add(1, Ordering::Relaxed),
        }
    }
}
impl DiscoveryConfig {
    /// Validate bounds before a request is sent.
    fn validate(&self) -> Result<()> {
        if self.timeout.is_zero()
            || self.timeout > Duration::from_secs(60)
            || !(1..=4096).contains(&self.max_devices)
            || !(1..=1_000_000).contains(&self.max_frames)
        {
            return Err(Error::InvalidInput("invalid DCP discovery limits"));
        }
        Ok(())
    }
}
/// Identify all devices using an injected Ethernet adapter.
/// Matching malformed responses fail explicitly; unrelated traffic is ignored.
/// Responses are deduplicated by MAC, with the latest metadata retained.
pub fn discover_with(io: &mut impl FrameIo, config: &DiscoveryConfig) -> Result<Vec<Device>> {
    collect(io, config, None)
}
/// Identify a specific unicast MAC; a missing reply returns Timeout.
pub fn identify_with(
    io: &mut impl FrameIo,
    target: [u8; 6],
    config: &DiscoveryConfig,
) -> Result<Device> {
    collect(io, config, Some(target))?
        .into_iter()
        .find(|device| device.mac == target)
        .ok_or(Error::Timeout)
}
/// Run a bounded Identify collection without resetting its total deadline.
fn collect(
    io: &mut impl FrameIo,
    config: &DiscoveryConfig,
    target: Option<[u8; 6]>,
) -> Result<Vec<Device>> {
    config.validate()?;
    let local = io.mac();
    let request = wire::identify_request(local, target, config.xid)?;
    let deadline = Instant::now() + config.timeout;
    io.send(&request, deadline)?;
    let mut devices = BTreeMap::new();
    for _ in 0..config.max_frames {
        if Instant::now() >= deadline {
            return Ok(devices.into_values().collect());
        }
        let Some(frame) = io.receive(deadline)? else {
            return Ok(devices.into_values().collect());
        };
        if let Some(device) = wire::parse_response(&frame, local, config.xid)? {
            if target.is_some_and(|mac| mac != device.mac) {
                continue;
            }
            if devices.len() >= config.max_devices && !devices.contains_key(&device.mac) {
                return Err(Error::Protocol("DCP device limit reached"));
            }
            devices.insert(device.mac, device);
            if target.is_some() {
                return Ok(devices.into_values().collect());
            }
        }
    }
    Err(Error::Protocol("DCP frame limit reached"))
}
/// Open the interface selected by its local IPv4 and discover devices.
pub fn discover(local_ip: Ipv4Addr, config: DiscoveryConfig) -> Result<Vec<Device>> {
    discover_with(&mut RawEthernet::open(local_ip)?, &config)
}
/// Open the selected interface and identify one station by MAC.
pub fn identify(local_ip: Ipv4Addr, target: [u8; 6], config: DiscoveryConfig) -> Result<Device> {
    identify_with(&mut RawEthernet::open(local_ip)?, target, &config)
}
