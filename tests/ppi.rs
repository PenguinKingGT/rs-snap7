//! Pinned PPI frames, partial serial I/O, retry limits and S7-200 areas.
#![cfg(feature = "ppi")]
use rs_snap7::{
    Error,
    ppi::{
        self, Port, PpiArea, PpiClient, PpiConfig, PpiTransport,
        frame::{self, Frame},
    },
    protocol::{Pdu, PduType},
};
use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

struct Fake {
    input: VecDeque<u8>,
    output: Arc<Mutex<Vec<u8>>>,
    timeout_once: bool,
}
impl Read for Fake {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if self.timeout_once {
            self.timeout_once = false;
            return Err(io::ErrorKind::TimedOut.into());
        }
        let n = bytes.len().min(2).min(self.input.len());
        for byte in bytes.iter_mut().take(n) {
            *byte = self.input.pop_front().unwrap();
        }
        Ok(n)
    }
}
impl Write for Fake {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = bytes.len().min(3);
        self.output.lock().unwrap().extend_from_slice(&bytes[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Port for Fake {
    fn set_timeout(&mut self, timeout: Duration) -> io::Result<()> {
        assert!(!timeout.is_zero());
        Ok(())
    }
}
fn fake(input: Vec<u8>) -> (Fake, Arc<Mutex<Vec<u8>>>) {
    let output = Arc::new(Mutex::new(Vec::new()));
    (
        Fake {
            input: input.into(),
            output: output.clone(),
            timeout_once: false,
        },
        output,
    )
}
fn reply(reference: u16, parameters: Vec<u8>, data: Vec<u8>) -> Vec<u8> {
    Pdu {
        kind: PduType::AckData,
        reference,
        error: 0,
        parameters,
        data,
    }
    .encode()
    .unwrap()
}
fn exchange_reply(pdu: &[u8]) -> Vec<u8> {
    let mut input = vec![0xe5];
    input.extend(frame::encode(0x68, 0, 2, 8, pdu).unwrap());
    input
}
fn writes(output: &[u8]) -> Vec<Frame> {
    let mut offset = 0;
    let mut frames = Vec::new();
    while offset < output.len() {
        let size = match output[offset] {
            0x68 => usize::from(output[offset + 1]) + 6,
            0x10 => 6,
            _ => panic!("unexpected output"),
        };
        frames.push(frame::decode(&output[offset..offset + size]).unwrap());
        offset += size;
    }
    frames
}
#[test]
fn exact_frame_vectors_checksums_and_limits() {
    assert_eq!(
        frame::encode(0x10, 2, 0, 0x5c, &[]).unwrap(),
        [0x10, 2, 0, 0x5c, 0x5e, 0x16]
    );
    assert_eq!(
        frame::encode(0x68, 2, 0, 0x6c, &[0x32, 1]).unwrap(),
        [0x68, 5, 5, 0x68, 2, 0, 0x6c, 0x32, 1, 0xa1, 0x16]
    );
    for raw in [
        frame::encode(0xa2, 2, 0, 3, &[0; 8]).unwrap(),
        frame::encode(0x68, 2, 0, 0x6c, &[0; 246]).unwrap(),
    ] {
        assert!(frame::decode(&raw).is_ok());
        for n in 0..raw.len() {
            assert!(frame::decode(&raw[..n]).is_err());
        }
        let mut bad = raw.clone();
        let n = bad.len();
        bad[n - 2] ^= 1;
        assert!(frame::decode(&bad).is_err());
    }
    assert_eq!(frame::decode(&[0xe5]).unwrap(), Frame::Ack);
    assert!(frame::decode(&[0xe5, 0]).is_err());
    assert!(frame::encode(0x68, 127, 0, 0, &[]).is_err());
    assert!(frame::encode(0x68, 2, 0, 0, &[0; 247]).is_err());
    assert!(frame::encode(0xa2, 2, 0, 0, &[0; 7]).is_err());
}
#[test]
fn serial_exchange_partial_io_repolling_and_corrupt_frame_recovery() {
    let mut input = vec![0xe5, 0xe5];
    let mut bad = frame::encode(0x68, 0, 2, 8, b"bad").unwrap();
    bad[7] ^= 1;
    input.extend(bad);
    input.extend(frame::encode(0x68, 0, 2, 8, b"reply").unwrap());
    let (port, out) = fake(input);
    let mut transport = PpiTransport::new(port, PpiConfig::default()).unwrap();
    assert_eq!(transport.exchange(b"request", true).unwrap(), b"reply");
    let frames = writes(&out.lock().unwrap());
    assert_eq!(frames.len(), 3);
    assert!(matches!(
        frames[1],
        Frame::Data {
            delimiter: 0x10,
            control: 0x5c,
            ..
        }
    ));
    assert!(matches!(
        frames[2],
        Frame::Data {
            delimiter: 0x10,
            control: 0x7c,
            ..
        }
    ));
}
#[test]
fn reads_retry_initial_timeouts_but_writes_never_retransmit() {
    let (mut port, out) = fake(exchange_reply(b"ok"));
    port.timeout_once = true;
    let mut transport = PpiTransport::new(port, PpiConfig::default()).unwrap();
    assert_eq!(transport.exchange(b"read", true).unwrap(), b"ok");
    assert_eq!(writes(&out.lock().unwrap()).len(), 3);
    let (port, out) = fake(vec![]);
    let mut transport = PpiTransport::new(port, PpiConfig::default()).unwrap();
    assert!(matches!(
        transport.exchange(b"write", false),
        Err(Error::Timeout)
    ));
    assert_eq!(writes(&out.lock().unwrap()).len(), 1);
    let (port, _) = fake(vec![0xe5, 0x10, 2, 0, 0x5c, 0x5e, 0x16]);
    assert!(
        PpiTransport::new(port, PpiConfig::default())
            .unwrap()
            .exchange(b"read", true)
            .is_err()
    );
}
#[test]
fn negotiated_client_maps_v_and_s7_200_word_areas() {
    let mut input = exchange_reply(&reply(1, vec![0xf0, 0, 0, 1, 0, 1, 0, 240], vec![]));
    input.extend(exchange_reply(&reply(
        2,
        vec![4, 1],
        vec![0xff, 4, 0, 16, 1, 2],
    )));
    input.extend(exchange_reply(&reply(
        3,
        vec![4, 1],
        vec![0xff, 9, 0, 4, 0, 1, 0, 2],
    )));
    input.extend(exchange_reply(&reply(4, vec![5, 1], vec![0xff])));
    let (port, out) = fake(input);
    let mut client =
        PpiClient::connect(PpiTransport::new(port, PpiConfig::default()).unwrap()).unwrap();
    assert_eq!(client.pdu_length(), 240);
    assert_eq!(client.v_read(10, 2).unwrap(), [1, 2]);
    assert_eq!(client.read_area(PpiArea::C, 1, 2).unwrap(), [0, 1, 0, 2]);
    client.v_write(10, &[2, 3]).unwrap();
    assert!(client.read_area(PpiArea::Ai, 0, 200).is_err());
    assert!(client.write_area(PpiArea::T, 0, &[1]).is_err());
    assert!(client.v_read(0x200000, 1).is_err());
    let frames = writes(&out.lock().unwrap());
    let jobs: Vec<_> = frames
        .iter()
        .filter_map(|frame| match frame {
            Frame::Data {
                delimiter: 0x68,
                payload,
                ..
            } => Some(Pdu::decode(payload).unwrap()),
            _ => None,
        })
        .collect();
    assert_eq!(jobs.len(), 4);
    assert_eq!(jobs[1].parameters[10], 0x84);
    assert_eq!(&jobs[1].parameters[8..10], &[0, 1]); // V memory is DB1.
    assert_eq!(jobs[2].parameters[5], 0x1e);
    assert_eq!(jobs[2].parameters[10], 0x1e);
    assert_eq!(&jobs[2].parameters[11..14], &[0, 0, 1]); // Counter offset is an item index.
    let _port = client.disconnect().into_inner();
}
#[test]
fn wrong_reference_invalidates_client_and_os_open_failure_is_reported() {
    let mut input = exchange_reply(&reply(1, vec![0xf0, 0, 0, 1, 0, 1, 0, 240], vec![]));
    input.extend(exchange_reply(&reply(
        3,
        vec![4, 1],
        vec![0xff, 4, 0, 8, 42],
    )));
    let (port, _) = fake(input);
    let mut client =
        PpiClient::connect(PpiTransport::new(port, PpiConfig::default()).unwrap()).unwrap();
    assert!(client.v_read(0, 1).is_err());
    assert!(!client.is_connected());
    assert!(matches!(client.v_read(0, 1), Err(Error::Disconnected)));
    assert!(
        ppi::Serial::open(
            "/definitely/nonexistent/serial",
            9600,
            Duration::from_millis(10)
        )
        .is_err()
    );
}
