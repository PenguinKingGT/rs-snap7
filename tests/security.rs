//! Password vectors, error layering, and real-PLC null acknowledgement behavior.

mod common;
use common::*;
#[cfg(feature = "blocking")]
use rs_snap7::blocking::Client;
use rs_snap7::{ClientConfig, Error, protocol::*};
use std::{net::TcpListener, thread};

/// The SECRET vector is copied from the pinned upstream/native Snap7 corpus.
#[test]
fn password_known_vector_and_invalid_inputs() {
    assert_eq!(
        encode_password("SECRET").unwrap(),
        [6, 16, 16, 23, 0, 22, 117, 99]
    );
    assert_eq!(encode_password("ABCDEFGH").unwrap()[0], b'A' ^ 0x55);
    for invalid in ["", "123456789", "中文"] {
        assert!(encode_password(invalid).is_err());
    }
}

/// Null acknowledgements apply only to no-data operations, with zero parameter errors.
#[test]
fn userdata_identity_lengths_and_error_layers() {
    let valid = Pdu {
        kind: PduType::UserData,
        reference: 1,
        error: 0,
        parameters: vec![0, 1, 0x12, 8, 0x12, 0x85, 1, 0, 0, 0, 0, 0],
        data: vec![0x0a, 0, 0, 0],
    };
    assert!(parse_userdata(&valid, 5, 1, true).is_ok());
    assert!(parse_userdata(&valid, 5, 1, false).is_err());
    assert!(parse_userdata(&valid, 5, 2, true).is_err());
    for index in [0, 1, 2, 3, 4, 5, 6, 9] {
        let mut bad = valid.clone();
        bad.parameters[index] = 0xff;
        assert!(parse_userdata(&bad, 5, 1, true).is_err());
    }
    let mut bad = valid.clone();
    bad.parameters[10] = 0xd6;
    bad.parameters[11] = 2;
    assert!(matches!(
        parse_userdata(&bad, 5, 1, true),
        Err(Error::Authentication(0xd602))
    ));
    for data in [
        vec![],
        vec![0xff, 9, 0, 1],
        vec![0xff, 9, 0, 0, 0],
        vec![5, 0, 0, 0],
    ] {
        let mut bad = valid.clone();
        bad.data = data;
        assert!(parse_userdata(&bad, 5, 1, true).is_err());
    }
}

/// The actual client transmits encoded passwords and preserves authentication failures.
#[test]
#[cfg(feature = "blocking")]
fn password_set_clear_and_rejection_over_tcp() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
    let peer = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        negotiate(&mut stream, 480);
        for (sub, error) in [(1, 0u16), (2, 0), (1, 0xd602)] {
            let request = read_frame(&mut stream);
            let pdu = Pdu::decode(&request[3..]).unwrap();
            assert_eq!(pdu.parameters, [0, 1, 0x12, 4, 0x11, 0x45, sub, 0]);
            if sub == 1 {
                assert_eq!(&pdu.data[4..], &[6, 16, 16, 23, 0, 22, 117, 99]);
            } else {
                assert_eq!(pdu.data, [0x0a, 0, 0, 0]);
            }
            let mut params = vec![0, 1, 0x12, 8, 0x12, 0x85, sub, 0, 0, 0];
            params.extend_from_slice(&error.to_be_bytes());
            let response = Pdu {
                kind: PduType::UserData,
                reference: pdu.reference,
                error: 0,
                parameters: params,
                data: vec![0x0a, 0, 0, 0],
            };
            let mut frame = vec![2, 0xf0, 0x80];
            frame.extend(response.encode().unwrap());
            send_frame(&mut stream, &frame);
        }
        read_frame(&mut stream);
    });
    let client = Client::connect(config).unwrap();
    assert!(client.set_session_password("").is_err());
    client.set_session_password("SECRET").unwrap();
    client.clear_session_password().unwrap();
    assert!(matches!(
        client.set_session_password("SECRET"),
        Err(Error::Authentication(0xd602))
    ));
    assert!(client.is_connected().unwrap());
    client.disconnect().unwrap();
    peer.join().unwrap();
}

/// Native async counterpart of the blocking compatibility scenario.
#[tokio::test(flavor = "current_thread")]
async fn async_password_set_clear_and_rejection_over_tcp() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
    let peer = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        negotiate(&mut stream, 480);
        for (sub, error) in [(1, 0u16), (2, 0), (1, 0xd602)] {
            let request = read_frame(&mut stream);
            let pdu = Pdu::decode(&request[3..]).unwrap();
            assert_eq!(pdu.parameters, [0, 1, 0x12, 4, 0x11, 0x45, sub, 0]);
            if sub == 1 {
                assert_eq!(&pdu.data[4..], &[6, 16, 16, 23, 0, 22, 117, 99]);
            } else {
                assert_eq!(pdu.data, [0x0a, 0, 0, 0]);
            }
            let mut params = vec![0, 1, 0x12, 8, 0x12, 0x85, sub, 0, 0, 0];
            params.extend_from_slice(&error.to_be_bytes());
            let response = Pdu {
                kind: PduType::UserData,
                reference: pdu.reference,
                error: 0,
                parameters: params,
                data: vec![0x0a, 0, 0, 0],
            };
            let mut frame = vec![2, 0xf0, 0x80];
            frame.extend(response.encode().unwrap());
            send_frame(&mut stream, &frame);
        }
        read_frame(&mut stream);
    });
    let client = rs_snap7::Client::connect(config).await.unwrap();
    assert!(client.set_session_password("").await.is_err());
    client.set_session_password("SECRET").await.unwrap();
    client.clear_session_password().await.unwrap();
    assert!(matches!(
        client.set_session_password("SECRET").await,
        Err(Error::Authentication(0xd602))
    ));
    assert!(client.is_connected().await);
    client.disconnect().await.unwrap();
    peer.join().unwrap();
}
