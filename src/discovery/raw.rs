//! Platform-owned bounded raw Ethernet capture and transmission.

use super::FrameIo;
use crate::{Error, Result};
use std::{net::Ipv4Addr, time::Instant};

/// OS Ethernet channel. Opening requires raw socket/BPF privileges.
pub struct RawEthernet {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    sender: Box<dyn pnet_datalink::DataLinkSender>,
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    receiver: Box<dyn pnet_datalink::DataLinkReceiver>,
    mac: [u8; 6],
}
impl RawEthernet {
    /// Select an up interface by local IPv4. No adapter is opened on unsupported OSes.
    pub fn open(local_ip: Ipv4Addr) -> Result<Self> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            use pnet_datalink::{self as net, Channel, Config};
            let interface = net::interfaces()
                .into_iter()
                .find(|interface| {
                    interface.is_up()
                        && interface
                            .ips
                            .iter()
                            .any(|ip| ip.ip() == std::net::IpAddr::V4(local_ip))
                })
                .ok_or(Error::InvalidInput("local IPv4 interface not found"))?;
            let mac = interface
                .mac
                .ok_or(Error::InvalidInput("interface has no MAC"))?
                .octets();
            let config = Config {
                read_timeout: Some(std::time::Duration::from_millis(10)),
                write_timeout: Some(std::time::Duration::from_millis(10)),
                promiscuous: false,
                read_buffer_size: 65536,
                write_buffer_size: 4096,
                ..Default::default()
            };
            match net::channel(&interface, config)? {
                Channel::Ethernet(sender, receiver) => Ok(Self {
                    sender,
                    receiver,
                    mac,
                }),
                _ => Err(Error::Unsupported(
                    "interface does not provide Ethernet frames",
                )),
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = local_ip;
            Err(Error::Unsupported(
                "built-in DCP raw Ethernet adapter on this OS; supply FrameIo",
            ))
        }
    }
}
impl FrameIo for RawEthernet {
    /// Bound source address to the opened interface.
    fn mac(&self) -> [u8; 6] {
        self.mac
    }
    /// Send one complete Ethernet frame; report driver refusal.
    fn send(&mut self, frame: &[u8], deadline: Instant) -> Result<()> {
        if Instant::now() >= deadline {
            return Err(Error::Timeout);
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            self.sender
                .send_to(frame, None)
                .ok_or(Error::Unsupported("raw Ethernet send unavailable"))??;
            Ok(())
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = frame;
            Err(Error::Unsupported("raw Ethernet on this OS"))
        }
    }
    /// Poll capture in short bounded waits until the caller's absolute deadline.
    fn receive(&mut self, deadline: Instant) -> Result<Option<Vec<u8>>> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            while Instant::now() < deadline {
                match self.receiver.next() {
                    Ok(frame) => return Ok(Some(frame.to_vec())),
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::TimedOut
                                | std::io::ErrorKind::WouldBlock
                                | std::io::ErrorKind::Interrupted
                        ) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Ok(None)
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = deadline;
            Err(Error::Unsupported("raw Ethernet on this OS"))
        }
    }
}
