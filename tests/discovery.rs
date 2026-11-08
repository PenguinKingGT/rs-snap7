//! DCP Identify Ethernet vectors and deterministic adapter scenarios.
#![cfg(feature = "discovery")]
use rs_snap7::{
    Error, Result,
    discovery::{self, DiscoveryConfig, FrameIo, wire},
};
use std::{collections::VecDeque, time::Instant};
const LOCAL: [u8; 6] = [2, 1, 2, 3, 4, 5];
const REMOTE: [u8; 6] = [2, 6, 7, 8, 9, 10];

fn response(xid: u32) -> Vec<u8> {
    let mut bytes = LOCAL.to_vec();
    bytes.extend(REMOTE);
    bytes.extend([0x88, 0x92, 0xfe, 0xff, 5, 1]);
    bytes.extend(xid.to_be_bytes());
    bytes.extend([0, 0, 0, 44]);
    bytes.extend([2, 2, 0, 5, 0, 0, b'p', b'l', b'c', 0]); // odd block has a pad byte
    bytes.extend([
        1, 2, 0, 14, 0, 1, 192, 168, 1, 10, 255, 255, 255, 0, 192, 168, 1, 1,
    ]);
    bytes.extend([2, 3, 0, 6, 0, 0, 0, 42, 0, 7]);
    bytes.extend([2, 1, 0, 2, 0, 0]); // empty family string
    bytes.resize(80, 0);
    bytes
}
struct Fake {
    frames: VecDeque<Vec<u8>>,
    sent: Vec<Vec<u8>>,
}
impl FrameIo for Fake {
    fn mac(&self) -> [u8; 6] {
        LOCAL
    }
    fn send(&mut self, frame: &[u8], _: Instant) -> Result<()> {
        self.sent.push(frame.to_vec());
        Ok(())
    }
    fn receive(&mut self, _: Instant) -> Result<Option<Vec<u8>>> {
        Ok(self.frames.pop_front())
    }
}
#[test]
fn identify_all_wire_header_and_metadata_are_exact() {
    let request = wire::identify_request(LOCAL, None, 0x12345678).unwrap();
    assert_eq!(
        &request[..30],
        &[
            1, 14, 207, 0, 0, 0, 2, 1, 2, 3, 4, 5, 0x88, 0x92, 0xfe, 0xfe, 5, 0, 0x12, 0x34, 0x56,
            0x78, 0, 0x80, 0, 4, 255, 255, 0, 0
        ]
    );
    assert_eq!(request.len(), 60);
    let raw = response(42);
    let device = wire::parse_response(&raw, LOCAL, 42).unwrap().unwrap();
    assert_eq!(device.name, "plc");
    assert_eq!(device.mac, REMOTE);
    assert_eq!(device.ip.unwrap().to_string(), "192.168.1.10");
    assert_eq!(device.netmask.unwrap().to_string(), "255.255.255.0");
    assert_eq!(device.vendor_id, Some(42));
    assert_eq!(device.device_id, Some(7));
    assert!(wire::parse_response(&raw, LOCAL, 43).unwrap().is_none());
    let mut vlan = raw[..12].to_vec();
    vlan.extend([0x81, 0, 0, 1]);
    vlan.extend_from_slice(&raw[12..]);
    assert_eq!(
        wire::parse_response(&vlan, LOCAL, 42).unwrap().unwrap(),
        device
    );
}
#[test]
fn malformed_matching_responses_are_rejected_without_panics() {
    let raw = response(42);
    for n in 14..70 {
        // complete advertised body ends at byte 70
        let truncated = &raw[..n];
        if n >= 26 {
            assert!(wire::parse_response(truncated, LOCAL, 42).is_err(), "{n}");
        } else {
            let _ = wire::parse_response(truncated, LOCAL, 42);
        }
    }
    let mut bad = raw.clone();
    bad[24..26].copy_from_slice(&1501u16.to_be_bytes());
    assert!(wire::parse_response(&bad, LOCAL, 42).is_err());
    let mut bad = raw.clone();
    bad[28..30].copy_from_slice(&65535u16.to_be_bytes());
    assert!(wire::parse_response(&bad, LOCAL, 42).is_err());
    let mut bad = raw.clone();
    bad[17] = 5;
    assert!(wire::parse_response(&bad, LOCAL, 42).is_err());
    assert!(wire::identify_request([0; 6], None, 1).is_err());
    assert!(wire::identify_request(LOCAL, Some([255; 6]), 1).is_err());
}
#[test]
fn collector_deduplicates_ignores_unrelated_and_bounds_traffic() {
    let config = DiscoveryConfig {
        xid: 42,
        ..Default::default()
    };
    let raw = response(42);
    let mut io = Fake {
        frames: vec![vec![0; 60], response(41), raw.clone(), raw.clone()].into(),
        sent: vec![],
    };
    let devices = discovery::discover_with(&mut io, &config).unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(io.sent.len(), 1);
    let mut io = Fake {
        frames: vec![raw].into(),
        sent: vec![],
    };
    assert_eq!(
        discovery::identify_with(&mut io, REMOTE, &config)
            .unwrap()
            .name,
        "plc"
    );
    assert_eq!(&io.sent[0][..6], &REMOTE);
    let mut io = Fake {
        frames: VecDeque::new(),
        sent: vec![],
    };
    assert!(matches!(
        discovery::identify_with(&mut io, REMOTE, &config),
        Err(Error::Timeout)
    ));
    let mut io = Fake {
        frames: vec![vec![0; 60]; 3].into(),
        sent: vec![],
    };
    let bounded = DiscoveryConfig {
        max_frames: 2,
        ..config
    };
    assert!(discovery::discover_with(&mut io, &bounded).is_err());
}
#[test]
fn missing_interface_is_reported_before_capture_or_network_sending() {
    assert!(discovery::RawEthernet::open("192.0.2.254".parse().unwrap()).is_err());
}
