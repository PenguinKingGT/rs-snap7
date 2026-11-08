//! Native DINTEGER responses from an independent wire peer keep sessions usable.

mod common;
use common::*;
#[cfg(feature = "blocking")]
use rs_snap7::blocking::Client;
use rs_snap7::{
    Address, Area, ClientConfig, WordLen,
    protocol::{Pdu, PduType},
};
use std::{
    net::{SocketAddr, TcpListener},
    thread::{self, JoinHandle},
};

/// Reply with byte-length transport 0x06 twice, checking reference reuse safety.
fn peer() -> (SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        negotiate(&mut stream, 480);
        for _ in 0..2 {
            let job = Pdu::decode(&read_frame(&mut stream)[3..]).unwrap();
            assert_eq!(job.parameters[5], WordLen::DInt as u8);
            let reply = Pdu {
                kind: PduType::AckData,
                reference: job.reference,
                error: 0,
                parameters: vec![4, 1],
                data: vec![0xff, 6, 0, 4, 0xff, 0xfe, 0x1d, 0xc0],
            };
            let mut frame = vec![2, 0xf0, 0x80];
            frame.extend(reply.encode().unwrap());
            send_frame(&mut stream, &frame);
        }
        assert_eq!(read_frame(&mut stream)[1], 0x80);
    });
    (endpoint, worker)
}

/// Blocking clients accept valid DINTEGER data and can issue the next request.
#[test]
#[cfg(feature = "blocking")]
fn blocking_dinteger_responses_preserve_connection() {
    let (endpoint, worker) = peer();
    let client = Client::connect(ClientConfig::new(endpoint, 0, 1).unwrap()).unwrap();
    let address = Address::new(Area::Db, 1, 4, WordLen::DInt, 1).unwrap();
    for _ in 0..2 {
        assert_eq!(
            client
                .read_multi_vars(&[address])
                .unwrap()
                .remove(0)
                .unwrap(),
            (-123456i32).to_be_bytes()
        );
        assert!(client.is_connected().unwrap());
    }
    client.disconnect().unwrap();
    worker.join().unwrap();
}

/// Native async clients use the same transport-unit handling without disconnection.
#[tokio::test]
async fn async_dinteger_responses_preserve_connection() {
    let (endpoint, worker) = peer();
    let client = rs_snap7::Client::connect(ClientConfig::new(endpoint, 0, 1).unwrap())
        .await
        .unwrap();
    let address = Address::new(Area::Db, 1, 4, WordLen::DInt, 1).unwrap();
    for _ in 0..2 {
        assert_eq!(
            client
                .read_multi_vars(&[address])
                .await
                .unwrap()
                .remove(0)
                .unwrap(),
            (-123456i32).to_be_bytes()
        );
        assert!(client.is_connected().await);
    }
    client.disconnect().await.unwrap();
    worker.join().unwrap();
}
