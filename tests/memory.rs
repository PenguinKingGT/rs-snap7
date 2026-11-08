//! Stateful independent wire peer for raw and typed memory compatibility tests.

mod common;
use common::*;
use rs_snap7::{Client, ClientConfig, Error};
use std::{
    net::{SocketAddr, TcpListener},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

/// A local wire peer with controlled failures and a bounded memory region.
struct Peer {
    endpoint: SocketAddr,
    memory: Arc<Mutex<Vec<u8>>>,
    worker: JoinHandle<()>,
}

impl Peer {
    /// Start an ephemeral peer; selected write exchanges may be rejected or lose their ACK.
    fn new(fail_at: Option<usize>, drop_at: Option<usize>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap();
        let memory = Arc::new(Mutex::new(vec![0; 1024]));
        let bytes = Arc::clone(&memory);
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            negotiate(&mut stream, 128);
            let mut writes = 0;
            loop {
                let request = read_frame(&mut stream);
                if request[1] == 0x80 {
                    break;
                }
                assert_eq!(&request[..3], &[2, 0xf0, 0x80]);
                let p = &request[3..];
                let param_len = usize::from(u16::from_be_bytes([p[6], p[7]]));
                let params = &p[10..10 + param_len];
                let payload = &p[10 + param_len..];
                let mut data = Vec::new();
                let (kind, response_params) = if p[1] == 7 {
                    assert_eq!(&params[..7], &[0, 1, 0x12, 4, 0x11, 0x43, 3]);
                    let db: u16 = std::str::from_utf8(&payload[6..11])
                        .unwrap()
                        .parse()
                        .unwrap();
                    let mut info = vec![0; 78];
                    info[1] = 0x41;
                    info[12..14].copy_from_slice(&db.to_be_bytes());
                    info[40..42].copy_from_slice(&1024u16.to_be_bytes());
                    data.extend_from_slice(&[0xff, 9, 0, 78]);
                    data.extend(info);
                    (7, vec![0, 1, 0x12, 8, 0x12, 0x83, 3, 0, 0, 0, 0, 0])
                } else {
                    assert_eq!(params[1], 1);
                    let address = &params[2..14];
                    let count = usize::from(u16::from_be_bytes([address[4], address[5]]));
                    let indexed = matches!(address[8], 0x1c | 0x1d);
                    let wire_offset =
                        u32::from_be_bytes([0, address[9], address[10], address[11]]) as usize;
                    let start = if indexed {
                        wire_offset * 2
                    } else {
                        wire_offset / 8
                    };
                    let size = count * if indexed { 2 } else { 1 };
                    let mut memory = bytes.lock().unwrap();
                    if params[0] == 4 {
                        data.extend_from_slice(&[0xff, if indexed { 9 } else { 4 }]);
                        data.extend_from_slice(
                            &((if indexed { size } else { size * 8 }) as u16).to_be_bytes(),
                        );
                        data.extend_from_slice(&memory[start..start + size]);
                    } else {
                        assert_eq!(params[0], 5);
                        writes += 1;
                        if drop_at == Some(writes) {
                            return;
                        }
                        if fail_at == Some(writes) {
                            data.push(5);
                        } else {
                            memory[start..start + size].copy_from_slice(&payload[4..4 + size]);
                            data.push(0xff);
                        }
                    }
                    (3, vec![params[0], 1])
                };
                let mut response = vec![2, 0xf0, 0x80, 0x32, kind, 0, 0, p[4], p[5]];
                response.extend_from_slice(&(response_params.len() as u16).to_be_bytes());
                response.extend_from_slice(&(data.len() as u16).to_be_bytes());
                if kind == 3 {
                    response.extend_from_slice(&[0, 0]);
                }
                response.extend(response_params);
                response.extend(data);
                send_frame(&mut stream, &response);
            }
        });
        Self {
            endpoint,
            memory,
            worker,
        }
    }

    /// Connect a real library client to this independently implemented peer.
    fn client(&self) -> Client {
        Client::connect(ClientConfig::new(self.endpoint, 0, 1).unwrap()).unwrap()
    }
}

/// Multi-PDU transfers, DB metadata discovery, get/fill, and empty operations.
#[test]
fn chunked_db_read_write_get_and_fill() {
    let peer = Peer::new(None, None);
    let client = peer.client();
    let bytes: Vec<u8> = (0..600).map(|i| i as u8).collect();
    client.db_write(1, 10, &bytes).unwrap();
    assert_eq!(client.db_read(1, 10, 600).unwrap(), bytes);
    assert_eq!(&peer.memory.lock().unwrap()[10..610], bytes);
    assert_eq!(client.db_get(1, None).unwrap().len(), 1024);
    client.db_fill(1, 0xab, Some(600)).unwrap();
    assert_eq!(client.db_get(1, Some(600)).unwrap(), vec![0xab; 600]);
    client.db_fill(1, 0x55, None).unwrap();
    assert_eq!(client.db_get(1, None).unwrap(), vec![0x55; 1024]);
    assert_eq!(client.db_read(1, 0, 0).unwrap(), Vec::<u8>::new());
    client.db_write(1, 0, &[]).unwrap();
    client.disconnect().unwrap();
    peer.worker.join().unwrap();
}

/// Timer/counter offsets count 16-bit elements while normal areas count bytes.
#[test]
fn area_helpers_and_timer_counter_units() {
    let peer = Peer::new(None, None);
    let client = peer.client();
    client.ct_write(2, &[1, 2, 3, 4]).unwrap();
    assert_eq!(client.ct_read(2, 2).unwrap(), [1, 2, 3, 4]);
    client.tm_write(4, &[5, 6]).unwrap();
    assert_eq!(client.tm_read(4, 1).unwrap(), [5, 6]);
    client.mb_write(0, &[7, 8]).unwrap();
    assert_eq!(client.mb_read(0, 2).unwrap(), [7, 8]);
    client.ab_write(0, &[9]).unwrap();
    assert_eq!(client.ab_read(0, 1).unwrap(), [9]);
    client.eb_write(0, &[10]).unwrap();
    assert_eq!(client.eb_read(0, 1).unwrap(), [10]);
    assert!(client.ct_write(0, &[1]).is_err());
    assert!(client.db_read(1, u32::MAX, 1).is_err());
    assert!(client.db_read(1, 0, usize::MAX).is_err());
    assert!(client.db_write(1, 0x1fffff, &[0, 0]).is_err());
    client.disconnect().unwrap();
    peer.worker.join().unwrap();
}

/// A definite later rejection reports acknowledged progress without claiming uncertainty.
#[test]
fn partial_write_reports_acknowledged_bytes() {
    let peer = Peer::new(Some(2), None);
    let client = peer.client();
    let error = client.db_write(1, 0, &[7; 300]).unwrap_err();
    assert!(matches!(error, Error::PartialWrite { confirmed: 100, .. }));
    assert_eq!(
        &peer.memory.lock().unwrap()[..101],
        &[vec![7; 100], vec![0]].concat()
    );
    assert!(client.is_connected());
    client.disconnect().unwrap();
    peer.worker.join().unwrap();
}

/// A lost write acknowledgement is never silently replayed and closes the session.
#[test]
fn lost_write_ack_reports_uncertain_range() {
    let peer = Peer::new(None, Some(2));
    let client = peer.client();
    let error = client.db_write(1, 0, &[7; 300]).unwrap_err();
    assert!(matches!(
        error,
        Error::AmbiguousWrite {
            confirmed: 100,
            uncertain: 100,
            ..
        }
    ));
    assert!(!client.is_connected());
    peer.worker.join().unwrap();
}
