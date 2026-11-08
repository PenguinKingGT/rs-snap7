//! Pinned PBC wire vectors, concurrent bidirectional transfers and teardown.
#![cfg(feature = "partner")]
mod common;
use rs_snap7::{
    Error,
    partner::{Partner, PartnerConfig, PartnerListener, Status, wire},
    protocol::Pdu,
};
use std::{
    sync::{Arc, Barrier},
    thread,
    time::Duration,
};

fn pair() -> (Partner, Partner) {
    let mut passive = PartnerConfig::new("127.0.0.1:0".parse().unwrap());
    passive.transport.local_tsap = 0x0102;
    passive.pdu_length = 128;
    let listener = PartnerListener::bind(passive).unwrap();
    let address = listener.local_addr().unwrap();
    let peer = thread::spawn(move || listener.accept().unwrap());
    let mut active = PartnerConfig::new(address);
    active.pdu_length = 128;
    let first = Partner::connect(active).unwrap();
    (first, peer.join().unwrap())
}
#[test]
fn pbc_vector_lengths_rid_and_ack_reference_are_exact() {
    let pdu = wire::data(0x1234, 0xaabbccdd, &[1, 2, 3], 128).unwrap();
    assert_eq!(
        pdu.encode().unwrap(),
        [
            0x32, 7, 0, 0, 0x12, 0x34, 0, 12, 0, 17, 0, 1, 0x12, 8, 0x12, 0x46, 1, 0, 0, 0, 0, 0,
            0xff, 9, 0, 13, 0x12, 6, 0x13, 0, 0xaa, 0xbb, 0xcc, 0xdd, 0, 3, 1, 2, 3
        ]
    );
    assert_eq!(wire::parse_data(&pdu).unwrap(), (0xaabbccdd, vec![1, 2, 3]));
    let ack = wire::ack(0x1234);
    wire::check_ack(&ack, 0x1234).unwrap();
    assert!(wire::check_ack(&ack, 0x1235).is_err());
    for n in 0..pdu.encode().unwrap().len() {
        assert!(Pdu::decode(&pdu.encode().unwrap()[..n]).is_err());
    }
    let mut corrupt = pdu.clone();
    corrupt.data[13] = 4;
    assert!(wire::parse_data(&corrupt).is_err());
    assert!(wire::data(1, 0, &[0; 93], 128).is_err());
    let mut refused = ack;
    refused.parameters[10..12].copy_from_slice(&0x8104u16.to_be_bytes());
    assert!(matches!(
        wire::check_ack(&refused, 0x1234),
        Err(Error::Plc(0x8104))
    ));
}
#[test]
fn simultaneous_sends_do_not_block_incoming_ack_dispatch() {
    let (left, right) = pair();
    let barrier = Arc::new(Barrier::new(3));
    let first = left.clone();
    let gate = barrier.clone();
    let a = thread::spawn(move || {
        gate.wait();
        for i in 0..16 {
            first.send(i, &[42; 32]).unwrap();
        }
    });
    let second = right.clone();
    let gate = barrier.clone();
    let b = thread::spawn(move || {
        gate.wait();
        for i in 0..16 {
            second.send(100 + i, &[12; 32]).unwrap();
        }
    });
    barrier.wait();
    a.join().unwrap();
    b.join().unwrap();
    for i in 0..16 {
        let received = left.receive(Duration::from_secs(1)).unwrap();
        assert_eq!(received.r_id, 100 + i);
        assert_eq!(received.data, [12; 32]);
        let received = right.receive(Duration::from_secs(1)).unwrap();
        assert_eq!(received.r_id, i);
    }
    assert_eq!(left.stats().unwrap().bytes_sent, 512);
    assert_eq!(right.stats().unwrap().sends, 16);
    assert!(matches!(
        left.receive(Duration::from_millis(5)),
        Err(Error::Timeout)
    ));
    assert_eq!(left.status(), Status::Connected);
    left.stop();
    assert_eq!(left.status(), Status::Stopped);
    right.stop();
}
#[test]
fn oversized_payload_is_rejected_without_disrupting_session() {
    let (left, right) = pair();
    assert!(matches!(
        left.send(1, &[0; 93]),
        Err(Error::InvalidInput(_))
    ));
    left.send(2, &[]).unwrap();
    assert_eq!(right.receive(Duration::from_secs(1)).unwrap().r_id, 2);
    left.stop();
    right.stop();
    assert!(matches!(left.send(1, &[]), Err(Error::Disconnected)));
}
#[test]
fn passive_accept_timeout_and_tsap_mismatch_are_bounded() {
    let mut config = PartnerConfig::new("127.0.0.1:0".parse().unwrap());
    config.transport.timeout = Duration::from_millis(30);
    let listener = PartnerListener::bind(config).unwrap();
    assert!(matches!(listener.accept(), Err(Error::Timeout)));
    let endpoint = listener.local_addr().unwrap();
    let worker = thread::spawn(move || assert!(listener.accept().is_err()));
    let mut active = PartnerConfig::new(endpoint);
    active.transport.timeout = Duration::from_millis(30);
    assert!(Partner::connect(active).is_err());
    worker.join().unwrap();
}
#[cfg(feature = "async")]
#[tokio::test]
async fn async_send_receive_use_blocking_pool_and_preserve_rid() {
    let (left, right) = pair();
    let (sent, received) = tokio::join!(
        left.send_async(42, vec![1, 2, 3]),
        right.receive_async(Duration::from_secs(1))
    );
    sent.unwrap();
    assert_eq!(received.unwrap().r_id, 42);
    left.stop();
    right.stop();
}

#[test]
fn lost_ack_is_ambiguous_and_never_replayed() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        common::negotiate(&mut stream, 128);
        let bytes = common::read_frame(&mut stream);
        let pdu = Pdu::decode(&bytes[3..]).unwrap();
        assert_eq!(wire::parse_data(&pdu).unwrap().0, 123);
    });
    let left = Partner::connect(PartnerConfig::new(endpoint)).unwrap();
    assert!(matches!(
        left.send(123, &[42]),
        Err(Error::AmbiguousOperation { .. })
    ));
    assert_eq!(left.status(), Status::Failed);
    assert!(left.stats().unwrap().last_error.is_some());
    worker.join().unwrap();
}

#[test]
fn receive_overflow_disconnects_and_completion_hook_runs_outside_send_lock() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let callbacks = Arc::new(AtomicUsize::new(0));
    let counter = callbacks.clone();
    let mut passive = PartnerConfig::new("127.0.0.1:0".parse().unwrap());
    passive.transport.local_tsap = 0x0102;
    passive.queue_capacity = 1;
    passive.callback = Some(Arc::new(move |_| {
        counter.fetch_add(1, Ordering::Relaxed);
    }));
    let listener = PartnerListener::bind(passive).unwrap();
    let address = listener.local_addr().unwrap();
    let worker = thread::spawn(move || listener.accept().unwrap());
    let active = Partner::connect(PartnerConfig::new(address)).unwrap();
    let passive = worker.join().unwrap();
    active.send(1, &[1]).unwrap();
    assert!(matches!(
        active.send(2, &[2]),
        Err(Error::AmbiguousOperation { .. })
    ));
    assert!(callbacks.load(Ordering::Relaxed) >= 1);
    assert_eq!(passive.receive(Duration::from_secs(1)).unwrap().r_id, 1);
    active.stop();
    passive.stop();
}
