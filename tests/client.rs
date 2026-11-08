//! Client lifecycle, correlation, and invalid session negotiation tests.

mod common;
use common::*;
use rs_snap7::{Client, ClientConfig, Error, protocol::Pdu};
use std::{net::TcpListener, thread};

/// Negotiation exposes peer limits and clones share disconnect state.
#[test]
fn lifecycle_raw_exchange_and_shared_disconnect() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
    let peer = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        negotiate(&mut stream, 240);
        let request = read_frame(&mut stream);
        assert_eq!(&request[7..9], &[0, 2]);
        send_frame(
            &mut stream,
            &[2, 0xf0, 0x80, 0x32, 3, 0, 0, 0, 2, 0, 1, 0, 0, 0, 0, 0x42],
        );
        assert_eq!(read_frame(&mut stream), [6, 0x80, 0, 7, 0, 1, 0]);
    });
    let client = Client::connect(config).unwrap();
    assert!(client.is_connected());
    assert_eq!(client.pdu_length().unwrap(), 240);
    let result = client
        .iso_exchange(&Pdu::job(999, vec![0x42], vec![]).encode().unwrap())
        .unwrap();
    assert_eq!(Pdu::decode(&result).unwrap().reference, 2);
    let other = client.clone();
    client.disconnect().unwrap();
    assert!(!other.is_connected());
    assert!(matches!(other.pdu_length(), Err(Error::Disconnected)));
    other.disconnect().unwrap();
    peer.join().unwrap();
}

/// Invalid peer limits or unmatched future references cannot publish a session.
#[test]
fn failed_negotiation_and_lost_reference_are_errors() {
    for pdu in [0, 127, 481] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
        let peer = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            negotiate(&mut stream, pdu);
        });
        assert!(Client::connect(config).is_err());
        peer.join().unwrap();
    }
    assert!(ClientConfig::new("127.0.0.1:102".parse().unwrap(), 8, 0).is_err());
}

/// Stale responses are drained rather than causing the request to be resent.
#[test]
fn stale_replies_are_skipped_without_replaying_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
    let peer = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        negotiate(&mut stream, 480);
        read_frame(&mut stream);
        for reference in [1, 2] {
            send_frame(
                &mut stream,
                &[
                    2, 0xf0, 0x80, 0x32, 3, 0, 0, 0, reference, 0, 1, 0, 0, 0, 0, 0x42,
                ],
            );
        }
        read_frame(&mut stream);
    });
    let client = Client::connect(config).unwrap();
    client
        .iso_exchange(&Pdu::job(0, vec![0x42], vec![]).encode().unwrap())
        .unwrap();
    client.disconnect().unwrap();
    peer.join().unwrap();
}

/// Native async counterpart of the blocking compatibility scenario.
#[cfg(feature = "async")]
#[tokio::test(flavor = "current_thread")]
async fn async_lifecycle_raw_exchange_and_shared_disconnect() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
    let peer = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        negotiate(&mut stream, 240);
        let request = read_frame(&mut stream);
        assert_eq!(&request[7..9], &[0, 2]);
        send_frame(
            &mut stream,
            &[2, 0xf0, 0x80, 0x32, 3, 0, 0, 0, 2, 0, 1, 0, 0, 0, 0, 0x42],
        );
        assert_eq!(read_frame(&mut stream), [6, 0x80, 0, 7, 0, 1, 0]);
    });
    let client = rs_snap7::AsyncClient::connect(config).await.unwrap();
    assert!(client.is_connected().await);
    assert_eq!(client.pdu_length().await.unwrap(), 240);
    let result = client
        .iso_exchange(&Pdu::job(999, vec![0x42], vec![]).encode().unwrap())
        .await
        .unwrap();
    assert_eq!(Pdu::decode(&result).unwrap().reference, 2);
    let other = client.clone();
    client.disconnect().await.unwrap();
    assert!(!other.is_connected().await);
    assert!(matches!(other.pdu_length().await, Err(Error::Disconnected)));
    other.disconnect().await.unwrap();
    peer.join().unwrap();
}

/// Native async counterpart of the blocking compatibility scenario.
#[cfg(feature = "async")]
#[tokio::test(flavor = "current_thread")]
async fn async_failed_negotiation_and_lost_reference_are_errors() {
    for pdu in [0, 127, 481] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
        let peer = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            negotiate(&mut stream, pdu);
        });
        assert!(rs_snap7::AsyncClient::connect(config).await.is_err());
        peer.join().unwrap();
    }
    assert!(ClientConfig::new("127.0.0.1:102".parse().unwrap(), 8, 0).is_err());
}

/// Native async counterpart of the blocking compatibility scenario.
#[cfg(feature = "async")]
#[tokio::test(flavor = "current_thread")]
async fn async_stale_replies_are_skipped_without_replaying_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
    let peer = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        negotiate(&mut stream, 480);
        read_frame(&mut stream);
        for reference in [1, 2] {
            send_frame(
                &mut stream,
                &[
                    2, 0xf0, 0x80, 0x32, 3, 0, 0, 0, reference, 0, 1, 0, 0, 0, 0, 0x42,
                ],
            );
        }
        read_frame(&mut stream);
    });
    let client = rs_snap7::AsyncClient::connect(config).await.unwrap();
    client
        .iso_exchange(&Pdu::job(0, vec![0x42], vec![]).encode().unwrap())
        .await
        .unwrap();
    client.disconnect().await.unwrap();
    peer.join().unwrap();
}
