//! Real local connection loss, renegotiation, replay, and heartbeat regressions.

mod common;
use common::*;
use rs_snap7::{Client, ClientConfig, Error, policy::*, protocol::*};
use std::{
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

/// Independent peer negotiation additionally verifies explicit LOGO TSAP preservation.
fn handshake(stream: &mut TcpStream, pdu: u16, logo: bool) {
    let cr = read_frame(stream);
    if logo {
        assert_eq!(&cr[7..15], &[0xc1, 2, 0x10, 0, 0xc2, 2, 0x20, 0]);
    }
    send_frame(stream, &[9, 0xd0, 0, 1, 0, 7, 0, 0xc0, 1, 10]);
    let setup = read_frame(stream);
    assert_eq!(setup[13], 0xf0);
    let mut response = vec![
        2, 0xf0, 0x80, 0x32, 3, 0, 0, 0, 1, 0, 8, 0, 0, 0, 0, 0xf0, 0, 0, 1, 0, 1,
    ];
    response.extend_from_slice(&pdu.to_be_bytes());
    send_frame(stream, &response);
}

/// Drop the first read response, then negotiate a smaller PDU for the replacement session.
fn recovery_peer(logo: bool) -> (SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let (mut first, _) = listener.accept().unwrap();
        handshake(&mut first, 480, logo);
        assert_eq!(read_frame(&mut first)[13], 4);
        drop(first);
        let (mut second, _) = listener.accept().unwrap();
        handshake(&mut second, 128, logo);
        loop {
            let raw = read_frame(&mut second);
            if raw[1] == 0x80 {
                break;
            }
            let job = Pdu::decode(&raw[3..]).unwrap();
            let a = &job.parameters[2..];
            let count = usize::from(u16::from_be_bytes([a[4], a[5]]));
            assert!(count <= 110);
            let start = u32::from_be_bytes([0, a[9], a[10], a[11]]) as usize / 8;
            let values: Vec<u8> = if logo {
                vec![0x12, 0x34]
            } else {
                (start..start + count).map(|i| i as u8).collect()
            };
            let mut data = vec![0xff, 4];
            data.extend_from_slice(&((values.len() * 8) as u16).to_be_bytes());
            data.extend(values);
            let reply = Pdu {
                kind: PduType::AckData,
                reference: job.reference,
                error: 0,
                parameters: vec![4, 1],
                data,
            };
            let mut frame = vec![2, 0xf0, 0x80];
            frame.extend(reply.encode().unwrap());
            send_frame(&mut second, &frame);
        }
    });
    (endpoint, worker)
}

/// Recovery is enabled explicitly; tests use immediate deterministic retry delays.
fn config(endpoint: SocketAddr) -> ClientConfig {
    let mut config = ClientConfig::new(endpoint, 0, 1).unwrap();
    config.recovery.enabled = true;
    config.recovery.initial_delay = Duration::ZERO;
    config.recovery.jitter = 0.0;
    config
}

/// A read is rebuilt after recovery and re-split to match the smaller negotiated PDU.
#[test]
fn read_recovery_preserves_data_and_dispatches_unlocked_hooks() {
    let (endpoint, worker) = recovery_peer(false);
    let events = Arc::new(Mutex::new(Vec::new()));
    let holder: Arc<Mutex<Option<Client>>> = Arc::new(Mutex::new(None));
    let sink = Arc::clone(&events);
    let current = Arc::clone(&holder);
    let mut options = config(endpoint);
    options.connection_hook = ConnectionHook::new(move |event| {
        if let Some(client) = current.lock().unwrap().as_ref() {
            assert_eq!(client.pdu_length().unwrap(), 128);
        }
        sink.lock().unwrap().push(event);
    });
    let client = Client::connect(options).unwrap();
    *holder.lock().unwrap() = Some(client.clone());
    assert_eq!(
        client.db_read(1, 0, 300).unwrap(),
        (0..300).map(|i| i as u8).collect::<Vec<_>>()
    );
    assert_eq!(
        *events.lock().unwrap(),
        [ConnectionEvent::Disconnected, ConnectionEvent::Reconnected]
    );
    client.disconnect().unwrap();
    assert!(matches!(client.db_read(1, 0, 1), Err(Error::Disconnected)));
    *holder.lock().unwrap() = None;
    worker.join().unwrap();
}

/// Native async read recovery shares the same policies and smaller-PDU behavior.
#[cfg(feature = "async")]
#[tokio::test(flavor = "current_thread")]
async fn async_read_recovery_and_bounded_future_size() {
    let (endpoint, worker) = recovery_peer(false);
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let mut options = config(endpoint);
    options.connection_hook = ConnectionHook::new(move |event| {
        sink.lock().unwrap().push(event);
    });
    let client = rs_snap7::AsyncClient::connect(options).await.unwrap();
    let operation = client.db_read(1, 0, 300);
    assert!(std::mem::size_of_val(&operation) < 16 * 1024);
    assert_eq!(
        operation.await.unwrap(),
        (0..300).map(|i| i as u8).collect::<Vec<_>>()
    );
    assert_eq!(
        *events.lock().unwrap(),
        [ConnectionEvent::Disconnected, ConnectionEvent::Reconnected]
    );
    client.disconnect().await.unwrap();
    worker.join().unwrap();
}

/// LOGO recovery retains caller-selected TSAPs rather than replacing them with rack/slot.
#[test]
fn logo_recovery_preserves_explicit_tsaps() {
    let (endpoint, worker) = recovery_peer(true);
    let mut options = config(endpoint);
    options.transport.local_tsap = 0x1000;
    options.transport.remote_tsap = vec![0x20, 0];
    let logo = rs_snap7::Logo::connect_with_config(options).unwrap();
    assert_eq!(logo.read("VW0").unwrap(), 0x1234);
    logo.client().disconnect().unwrap();
    worker.join().unwrap();
}

/// Automatic recovery never replays a write whose acknowledgement disappeared.
#[test]
fn uncertain_writes_are_not_replayed_even_with_recovery_enabled() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let options = config(listener.local_addr().unwrap());
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        negotiate(&mut stream, 480);
        assert_eq!(read_frame(&mut stream)[13], 5);
        drop(listener);
        drop(stream);
    });
    let client = Client::connect(options).unwrap();
    assert!(matches!(
        client.db_write(1, 0, &[1]),
        Err(Error::AmbiguousWrite { .. })
    ));
    assert!(!client.is_connected());
    worker.join().unwrap();
}

/// A peer that refuses reconnection exhausts the finite retry budget.
#[test]
fn recovery_exhaustion_does_not_leave_a_ready_session() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let options = config(listener.local_addr().unwrap());
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        negotiate(&mut stream, 480);
        read_frame(&mut stream);
        drop(listener);
        drop(stream);
    });
    let client = Client::connect(options).unwrap();
    assert!(client.db_read(1, 0, 1).is_err());
    assert!(!client.is_connected());
    worker.join().unwrap();
}

/// Respond to arbitrary heartbeat repetitions until the client explicitly disconnects.
fn serve_heartbeats(mut stream: TcpStream, notify: impl FnOnce()) {
    negotiate(&mut stream, 480);
    let mut notify = Some(notify);
    loop {
        let frame = read_frame(&mut stream);
        if frame[1] == 0x80 {
            break;
        }
        let job = Pdu::decode(&frame[3..]).unwrap();
        assert_eq!(&job.data[4..], &[4, 0x24, 0, 0]);
        let response = Pdu {
            kind: PduType::UserData,
            reference: job.reference,
            error: 0,
            parameters: vec![0, 1, 0x12, 8, 0x12, 0x84, 1, 0, 0, 0, 0, 0],
            data: vec![0xff, 9, 0, 12, 4, 0x24, 0, 0, 0, 0, 0, 0, 0, 0, 0, 8],
        };
        let mut frame = vec![2, 0xf0, 0x80];
        frame.extend(response.encode().unwrap());
        send_frame(&mut stream, &frame);
        if let Some(notify) = notify.take() {
            notify();
        }
    }
}

/// Blocking background work uses the normal session guard and stops after explicit close.
#[test]
fn heartbeat_probes_and_manual_disconnect_are_serialized() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut options = config(listener.local_addr().unwrap());
    options.recovery.heartbeat_interval = Some(Duration::from_millis(10));
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        serve_heartbeats(stream, || tx.send(()).unwrap());
    });
    let client = Client::connect(options).unwrap();
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    client.disconnect().unwrap();
    drop(client);
    worker.join().unwrap();
}

/// Async heartbeat work yields and shares the cancellation-safe socket owner.
#[cfg(feature = "async")]
#[tokio::test(flavor = "current_thread")]
async fn async_heartbeat_probe_and_shutdown() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut options = config(listener.local_addr().unwrap());
    options.recovery.heartbeat_interval = Some(Duration::from_millis(10));
    let (tx, rx) = tokio::sync::oneshot::channel();
    let worker = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        serve_heartbeats(stream, || tx.send(()).unwrap());
    });
    let client = rs_snap7::AsyncClient::connect(options).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), rx)
        .await
        .unwrap()
        .unwrap();
    client.disconnect().await.unwrap();
    drop(client);
    worker.join().unwrap();
}
