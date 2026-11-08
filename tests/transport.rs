//! Local TCP peers exercise real framing, fragmentation, failure, and deadlines.
#![cfg(feature = "blocking")]

use rs_snap7::{
    Error,
    transport::{TransportConfig, blocking::Connection},
};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::Duration,
};

/// Read a TPKT payload from a test peer independently of the library parser.
fn frame(stream: &mut TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut h = [0; 4];
    stream.read_exact(&mut h).unwrap();
    assert_eq!(&h[..2], &[3, 0]);
    let mut p = vec![0; usize::from(u16::from_be_bytes([h[2], h[3]])) - 4];
    stream.read_exact(&mut p).unwrap();
    p
}

/// Send one byte at a time to exercise partial-read assembly.
fn send(stream: &mut TcpStream, p: &[u8]) {
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut bytes = vec![3, 0];
    bytes.extend_from_slice(&((p.len() + 4) as u16).to_be_bytes());
    bytes.extend_from_slice(p);
    for b in bytes {
        stream.write_all(&[b]).unwrap();
    }
}

/// Accept and negotiate a peer reference distinct from the client's reference.
fn handshake(listener: TcpListener) -> TcpStream {
    let (mut stream, _) = listener.accept().unwrap();
    let cr = frame(&mut stream);
    assert_eq!(cr[1], 0xe0);
    send(&mut stream, &[9, 0xd0, 0, 1, 0x12, 0x34, 0, 0xc0, 1, 7]);
    stream
}

/// Both outbound and inbound COTP messages fragment at negotiated limits.
#[test]
fn fragmented_roundtrip_and_disconnect_reference() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = TransportConfig::new(listener.local_addr().unwrap());
    let peer = thread::spawn(move || {
        let mut stream = handshake(listener);
        let mut received = Vec::new();
        loop {
            let p = frame(&mut stream);
            assert!(p.len() <= 128);
            assert_eq!(&p[..2], &[2, 0xf0]);
            received.extend_from_slice(&p[3..]);
            if p[2] == 0x80 {
                break;
            }
        }
        assert_eq!(received, vec![0xaa; 300]);
        send(&mut stream, &[2, 0xf0, 0, 1, 2]);
        send(&mut stream, &[2, 0xf0, 0x80, 3]);
        assert_eq!(frame(&mut stream), [6, 0x80, 0x12, 0x34, 0, 1, 0]);
    });
    let mut connection = Connection::connect(&config).unwrap();
    assert_eq!(connection.tpdu_size(), 128);
    assert_eq!(connection.exchange(&[0xaa; 300]).unwrap(), [1, 2, 3]);
    connection.disconnect().unwrap();
    assert!(!connection.is_connected());
    connection.disconnect().unwrap();
    assert!(matches!(connection.receive(), Err(Error::Disconnected)));
    peer.join().unwrap();
}

/// Malformed TPKT and COTP input closes the stream instead of desynchronizing it.
#[test]
fn malformed_frames_and_reassembly_limits_close_connection() {
    for invalid in [
        vec![3, 1, 0, 7, 2, 0xf0, 0x80],
        vec![3, 0, 0, 3],
        vec![3, 0, 0, 8, 2, 0xf0, 0x81, 0],
        vec![3, 0, 0, 9, 2, 0xf0, 0x80, 1, 2],
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut config = TransportConfig::new(listener.local_addr().unwrap());
        config.max_message_size = 1;
        let peer = thread::spawn(move || {
            let mut stream = handshake(listener);
            stream.write_all(&invalid).unwrap();
        });
        let mut connection = Connection::connect(&config).unwrap();
        assert!(connection.receive().is_err());
        assert!(!connection.is_connected());
        peer.join().unwrap();
    }
}

/// A silent peer is bounded by the total receive deadline, not an unbounded read.
#[test]
fn silent_peer_times_out_and_releases_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut config = TransportConfig::new(listener.local_addr().unwrap());
    config.timeout = Duration::from_millis(100);
    let (tx, rx) = std::sync::mpsc::channel();
    let peer = thread::spawn(move || {
        let stream = handshake(listener);
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(stream);
    });
    let mut connection = Connection::connect(&config).unwrap();
    assert!(matches!(connection.receive(), Err(Error::Timeout)));
    assert!(!connection.is_connected());
    tx.send(()).unwrap();
    peer.join().unwrap();
}

/// Invalid configuration is rejected before contacting an endpoint.
#[test]
fn transport_rejects_invalid_settings() {
    let mut config = TransportConfig::new("127.0.0.1:1".parse().unwrap());
    config.timeout = Duration::ZERO;
    assert!(matches!(
        Connection::connect(&config),
        Err(Error::InvalidInput(_))
    ));
    config.timeout = Duration::from_secs(1);
    config.remote_tsap = vec![0; 250];
    assert!(config.validate().is_err());
}
