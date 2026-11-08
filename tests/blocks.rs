//! Block-service wire vectors and independent stateful PLC transfer scripts.

mod common;
use common::*;
use rs_snap7::{
    BlockType, Client, ClientConfig, Error,
    blocks::*,
    protocol::{blocks as wire, *},
};
use std::{
    io::Read,
    net::{TcpListener, TcpStream},
    thread::{self, JoinHandle},
    time::Duration,
};

/// Encode expected hex independently of the library's protocol builders.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Canonical DB compact header and 200 data bytes, matching the upstream server layout.
fn block() -> Vec<u8> {
    let mut bytes = vec![0; 36];
    bytes[2] = 1;
    bytes[4] = 5;
    bytes[5] = 0x0a;
    bytes[6..8].copy_from_slice(&9u16.to_be_bytes());
    bytes[8..12].copy_from_slice(&236u32.to_be_bytes());
    bytes[34..36].copy_from_slice(&200u16.to_be_bytes());
    bytes.extend((0..200).map(|i| i as u8));
    bytes
}

/// Known request/response vectors come from the pinned Python protocol tests.
#[test]
fn block_wire_vectors_and_download_boundaries() {
    assert_eq!(
        hex(&wire::start_upload(1, BlockType::Db, 1).encode().unwrap()),
        "320100000001001200001d00000000000000095f3041303030303141"
    );
    assert_eq!(
        hex(&wire::upload_job(1, 7, false).encode().unwrap()),
        "320100000001000800001e00000000000007"
    );
    assert_eq!(
        hex(&wire::upload_job(1, 7, true).encode().unwrap()),
        "320100000001000800001f00000000000007"
    );
    assert_eq!(
        hex(&wire::info_request(1, BlockType::Db, 1)
            .unwrap()
            .encode()
            .unwrap()),
        "3207000000010008000c0001120411430300ff0900083041303030303141"
    );
    assert_eq!(
        hex(&wire::download_request(1, 1, &[0; 64])
            .unwrap()
            .encode()
            .unwrap()),
        "320100000001002000001a00010000000000095f30413030303031500d31303030303634303030303238"
    );
    assert_eq!(
        hex(&wire::download_fragment(1, false, &[0xde, 0xad])
            .unwrap()
            .encode()
            .unwrap()),
        "3203000000010002000600001b01000200fbdead"
    );
    assert_eq!(
        hex(&wire::download_end(1).encode().unwrap()),
        "3203000000010001000000001c"
    );
    assert_eq!(
        wire::download_fragment(1, true, &[0; 462])
            .unwrap()
            .encode()
            .unwrap()
            .len(),
        480
    );
    assert!(wire::download_request(0, 1, &vec![0; 1_000_000]).is_err());
}

/// Metadata rejects duplicate types, partial rows, missing fields, and trailing bytes.
#[test]
fn catalog_and_header_models_validate_complete_layouts() {
    let mut counts = Vec::new();
    for code in [0x38, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46] {
        counts.extend_from_slice(&[0x30, code, 0, 2]);
    }
    let list = BlocksList::decode(&counts).unwrap();
    assert_eq!(list.count(BlockType::Db), 2);
    assert_eq!(list.iter().count(), 7);
    for n in 0..28 {
        assert!(BlocksList::decode(&counts[..n]).is_err());
    }
    let mut duplicate = counts.clone();
    duplicate[5] = duplicate[1];
    assert!(BlocksList::decode(&duplicate).is_err());
    let mut unknown = counts;
    unknown[1] = 0xff;
    assert!(BlocksList::decode(&unknown).is_err());
    assert!(decode_entries(&[]).unwrap().is_empty());
    assert!(decode_entries(&[0; 3]).is_err());
    assert_eq!(
        decode_entries(&[0, 9, 2, 5]).unwrap()[0],
        BlockEntry {
            number: 9,
            flags: 2,
            language: 5
        }
    );
    let info = CompactBlockInfo::decode(&block()).unwrap();
    assert_eq!((info.number, info.load_size, info.mc7_size), (9, 236, 200));
    assert_eq!(info.code_timestamp, [0; 6]);
    assert!(CompactBlockInfo::decode(&[0; 36]).is_err());
    assert!(CompactBlockInfo::decode(&block()[..35]).is_err());
    let mut remote = vec![0; 78];
    remote[1] = 0x41;
    remote[12..14].copy_from_slice(&9u16.to_be_bytes());
    remote[42] = 0xe9;
    let info = BlockInfo::decode(&remote).unwrap();
    assert_eq!(info.author, "é");
    assert_eq!(info.number, 9);
    assert!(BlockInfo::decode(&remote[..77]).is_err());
    remote.push(0);
    assert!(BlockInfo::decode(&remote).is_err());
}

/// Reject mismatched unsolicited PLC job types, filenames, lengths, and payloads.
#[test]
fn malformed_download_jobs_and_upload_fragments_are_rejected() {
    let mut params = vec![0x1b, 0, 0, 0, 0, 0, 0, 0, 9];
    params.extend_from_slice(b"_0A00009P");
    let valid = Pdu::job(0xbeef, params, vec![]);
    wire::check_download_job(&valid, 0x1b, 9).unwrap();
    for index in [0, 8, 9, 16, 17] {
        let mut bad = valid.clone();
        bad.parameters[index] ^= 0xff;
        assert!(wire::check_download_job(&bad, 0x1b, 9).is_err());
    }
    let mut bad = valid;
    bad.data.push(0);
    assert!(wire::check_download_job(&bad, 0x1b, 9).is_err());
    let mut upload = ack(1, vec![0x1e, 0], vec![0, 2, 0, 0xfb, 1, 2]);
    assert_eq!(
        wire::parse_upload_fragment(&upload).unwrap(),
        (&[1, 2][..], true)
    );
    upload.data[1] = 3;
    assert!(wire::parse_upload_fragment(&upload).is_err());
    upload.data[1] = 2;
    upload.data[3] = 0;
    assert!(wire::parse_upload_fragment(&upload).is_err());
}

/// Build responses independently; the peer supplies arbitrary server job references.
fn ack(reference: u16, parameters: Vec<u8>, data: Vec<u8>) -> Pdu {
    Pdu {
        kind: PduType::AckData,
        reference,
        error: 0,
        parameters,
        data,
    }
}
/// Send one S7 message as a complete COTP DT frame.
fn send_pdu(stream: &mut TcpStream, pdu: Pdu) {
    let mut frame = vec![2, 0xf0, 0x80];
    frame.extend(pdu.encode().unwrap());
    send_frame(stream, &frame);
}
/// Read one S7 message from the independent framing helper.
fn read_pdu(stream: &mut TcpStream) -> Pdu {
    Pdu::decode(&read_frame(stream)[3..]).unwrap()
}
/// Create one USERDATA fragment with explicit continuation sequence metadata.
fn userdata(reference: u16, sub: u8, sequence: u8, more: bool, payload: &[u8]) -> Pdu {
    let mut data = vec![0xff, 9];
    data.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    data.extend_from_slice(payload);
    Pdu {
        kind: PduType::UserData,
        reference,
        error: 0,
        parameters: vec![
            0,
            1,
            0x12,
            8,
            0x12,
            0x83,
            sub,
            sequence,
            0,
            u8::from(more),
            0,
            0,
        ],
        data,
    }
}
/// Start a scripted peer with deliberately small negotiated S7 PDUs.
fn peer(script: impl FnOnce(TcpStream) + Send + 'static) -> (ClientConfig, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let options = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        negotiate(&mut stream, 128);
        drop(listener);
        script(stream);
    });
    (options, worker)
}

/// Independently implement block catalog, uploads, PLC-driven downloads, and deletion.
fn serve(mut stream: TcpStream) {
    loop {
        let raw = read_frame(&mut stream);
        if raw[1] == 0x80 {
            break;
        }
        let job = Pdu::decode(&raw[3..]).unwrap();
        if job.kind == PduType::UserData {
            let sub = job.parameters[6];
            let payload = match sub {
                1 => [0x38, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46]
                    .into_iter()
                    .flat_map(|kind| [0x30, kind, 0, if kind == 0x41 { 3 } else { 0 }])
                    .collect::<Vec<_>>(),
                2 => vec![0, 1, 0, 5, 0, 9, 0, 5, 0, 17, 0, 5],
                3 => {
                    assert_eq!(&job.data[4..], b"0A00009A");
                    let mut info = vec![0; 78];
                    info[1] = 0x41;
                    info[10] = 5;
                    info[12..14].copy_from_slice(&9u16.to_be_bytes());
                    info[14..18].copy_from_slice(&236u32.to_be_bytes());
                    info[40..42].copy_from_slice(&200u16.to_be_bytes());
                    info
                }
                _ => panic!("unexpected service"),
            };
            if sub == 2 {
                send_pdu(
                    &mut stream,
                    userdata(job.reference, sub, 44, true, &payload[..6]),
                );
                let continuation = read_pdu(&mut stream);
                assert_eq!(
                    continuation.parameters,
                    [0, 1, 0x12, 8, 0x12, 0x43, 2, 44, 0, 0, 0, 0]
                );
                send_pdu(
                    &mut stream,
                    userdata(continuation.reference, sub, 45, false, &payload[6..]),
                );
            } else {
                send_pdu(
                    &mut stream,
                    userdata(job.reference, sub, 0, false, &payload),
                );
            }
        } else {
            match job.parameters[0] {
                0x1d => {
                    assert_eq!(&job.parameters[9..], b"_0A00009A");
                    let mut params = vec![0x1d, 0, 0, 0, 0, 0, 0, 7, 0, 0, 0];
                    params.extend_from_slice(b"00236");
                    send_pdu(&mut stream, ack(job.reference, params, vec![]));
                    let file = block();
                    let parts: Vec<_> = file.chunks(70).collect();
                    for (index, bytes) in parts.iter().enumerate() {
                        let request = read_pdu(&mut stream);
                        assert_eq!(request.parameters, [0x1e, 0, 0, 0, 0, 0, 0, 7]);
                        let mut data = (bytes.len() as u16).to_be_bytes().to_vec();
                        data.extend_from_slice(&[0, 0xfb]);
                        data.extend_from_slice(bytes);
                        send_pdu(
                            &mut stream,
                            ack(
                                request.reference,
                                vec![0x1e, u8::from(index + 1 < parts.len())],
                                data,
                            ),
                        );
                    }
                    let request = read_pdu(&mut stream);
                    assert_eq!(request.parameters, [0x1f, 0, 0, 0, 0, 0, 0, 7]);
                    send_pdu(&mut stream, ack(request.reference, vec![0x1f], vec![]));
                }
                0x1a => {
                    assert_eq!(&job.parameters[9..18], b"_0A00009P");
                    assert_eq!(&job.parameters[19..], b"1000236000200");
                    send_pdu(&mut stream, ack(job.reference, vec![0x1a], vec![]));
                    let mut bytes = Vec::new();
                    for reference in [0x9001, 0x9002, 0x9003] {
                        send_pdu(&mut stream, server_job(reference, 0x1b));
                        let reply = read_pdu(&mut stream);
                        assert_eq!(reply.reference, reference);
                        assert_eq!(reply.kind, PduType::AckData);
                        assert_eq!(reply.parameters, [0x1b, u8::from(reference != 0x9003)]);
                        assert_eq!(&reply.data[2..4], &[0, 0xfb]);
                        assert_eq!(
                            reply.data.len() - 4,
                            usize::from(u16::from_be_bytes([reply.data[0], reply.data[1]]))
                        );
                        bytes.extend_from_slice(&reply.data[4..]);
                    }
                    assert_eq!(bytes, block());
                    send_pdu(&mut stream, server_job(0xbeef, 0x1c));
                    let reply = read_pdu(&mut stream);
                    assert_eq!(reply.reference, 0xbeef);
                    assert_eq!(reply.parameters, [0x1c]);
                    assert!(reply.data.is_empty());
                }
                0x28 => {
                    assert_eq!(&job.parameters[9..], b"4100009P_DELE");
                    send_pdu(&mut stream, ack(job.reference, vec![0x28], vec![]));
                }
                _ => panic!("unexpected job"),
            }
        }
    }
}

/// Create a PLC-initiated download job with a reference outside the client's sequence.
fn server_job(reference: u16, function: u8) -> Pdu {
    let mut p = vec![function, 0, 0, 0, 0, 0, 0, 0, 9];
    p.extend_from_slice(b"_0A00009P");
    Pdu::job(reference, p, vec![])
}

/// Blocking entry points collect split catalogs and finish all three transfer phases.
#[test]
fn catalog_upload_download_delete_and_session_reuse() {
    let (options, worker) = peer(serve);
    let client = Client::connect(options).unwrap();
    assert!(
        client
            .list_blocks_of_type(BlockType::Db, 0)
            .unwrap()
            .is_empty()
    );
    assert_eq!(client.list_blocks().unwrap().count(BlockType::Db), 3);
    assert_eq!(
        client.list_blocks_of_type(BlockType::Db, 2).unwrap(),
        [1, 9]
    );
    assert_eq!(
        client.get_block_info(BlockType::Db, 9).unwrap().mc7_size,
        200
    );
    assert_eq!(client.upload(9).unwrap(), block()[36..]);
    assert_eq!(client.full_upload(BlockType::Db, 9).unwrap(), block());
    client.download(&block(), None).unwrap();
    assert_eq!(
        client.get_block_info(BlockType::Db, 9).unwrap().load_size,
        236
    );
    assert!(client.download(&[0; 7], None).is_err());
    assert!(client.download(&vec![0; 1_000_000], Some(9)).is_err());
    client.delete(BlockType::Db, 9).unwrap();
    client.disconnect().unwrap();
    worker.join().unwrap();
}

/// The native async adapter drives the same state machines and public catalog models.
#[cfg(feature = "async")]
#[tokio::test(flavor = "current_thread")]
async fn async_catalog_and_block_transfers() {
    let (options, worker) = peer(serve);
    let client = rs_snap7::AsyncClient::connect(options).await.unwrap();
    assert!(
        client
            .list_blocks_of_type(BlockType::Db, 0)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(client.list_blocks().await.unwrap().count(BlockType::Db), 3);
    assert_eq!(
        client.list_blocks_of_type(BlockType::Db, 2).await.unwrap(),
        [1, 9]
    );
    assert_eq!(
        client
            .get_block_info(BlockType::Db, 9)
            .await
            .unwrap()
            .mc7_size,
        200
    );
    assert_eq!(client.upload(9).await.unwrap(), block()[36..]);
    assert_eq!(client.full_upload(BlockType::Db, 9).await.unwrap(), block());
    client.download(&block(), None).await.unwrap();
    assert_eq!(
        client
            .get_block_info(BlockType::Db, 9)
            .await
            .unwrap()
            .load_size,
        236
    );
    client.delete(BlockType::Db, 9).await.unwrap();
    client.disconnect().await.unwrap();
    worker.join().unwrap();
}

/// Malformed transfer stages close the socket instead of contaminating later operations.
#[test]
fn malformed_upload_and_early_download_completion_close_session() {
    for download in [false, true] {
        let (options, worker) = peer(move |mut stream| {
            let job = read_pdu(&mut stream);
            if download {
                send_pdu(&mut stream, ack(job.reference, vec![0x1a], vec![]));
                send_pdu(&mut stream, server_job(0xbeef, 0x1c));
            } else {
                let mut params = vec![0x1d, 0, 0, 0, 0, 0, 0, 7, 0, 0, 0];
                params.extend_from_slice(b"00abc");
                send_pdu(&mut stream, ack(job.reference, params, vec![]));
            }
            assert_closed(&mut stream);
        });
        let client = Client::connect(options).unwrap();
        if download {
            assert!(matches!(
                client.download(&block(), Some(9)),
                Err(Error::AmbiguousOperation {
                    operation: "download",
                    ..
                })
            ));
        } else {
            assert!(matches!(client.upload(9), Err(Error::Protocol(_))));
        }
        assert!(!client.is_connected());
        worker.join().unwrap();
    }
}

/// A lost slice request cannot be treated as a known unapplied download or replayed.
#[test]
fn interrupted_download_and_initial_refusal_are_distinct() {
    for refused in [false, true] {
        let (mut options, worker) = peer(move |mut stream| {
            let job = read_pdu(&mut stream);
            let mut reply = ack(job.reference, vec![0x1a], vec![]);
            if refused {
                reply.error = 0x8702;
            }
            send_pdu(&mut stream, reply);
        });
        options.recovery.enabled = true;
        let client = Client::connect(options).unwrap();
        let error = client.download(&block(), Some(9)).unwrap_err();
        if refused {
            assert!(matches!(error, Error::Plc(0x8702)));
        } else {
            assert!(matches!(
                error,
                Error::AmbiguousOperation {
                    operation: "download",
                    ..
                }
            ));
        }
        assert!(!client.is_connected());
        worker.join().unwrap();
    }
}

/// Silence after the offer is bounded by the original logical operation deadline.
#[test]
fn stalled_download_times_out_and_closes() {
    let (mut options, worker) = peer(|mut stream| {
        let job = read_pdu(&mut stream);
        send_pdu(&mut stream, ack(job.reference, vec![0x1a], vec![]));
        assert_closed(&mut stream);
    });
    options.transport.timeout = Duration::from_millis(100);
    let client = Client::connect(options).unwrap();
    assert!(matches!(
        client.download(&block(), Some(9)),
        Err(Error::AmbiguousOperation { .. })
    ));
    assert!(!client.is_connected());
    worker.join().unwrap();
}

/// Cancellation while waiting for a reply permit still closes the whole transfer.
#[cfg(feature = "async")]
#[tokio::test(flavor = "current_thread")]
async fn cancelled_download_between_exchanges_closes_socket() {
    use rs_snap7::policy::{RateAlgorithm, RateLimit};
    let (started, waiting) = tokio::sync::oneshot::channel();
    let (mut options, worker) = peer(move |mut stream| {
        let job = read_pdu(&mut stream);
        send_pdu(&mut stream, ack(job.reference, vec![0x1a], vec![]));
        send_pdu(&mut stream, server_job(0x9001, 0x1b));
        started.send(()).unwrap();
        assert_closed(&mut stream);
    });
    options.transport.timeout = Duration::from_secs(120);
    options.rate_limit = RateLimit {
        max_per_second: 0.1,
        algorithm: RateAlgorithm::TokenBucket,
        burst: 2,
        ..Default::default()
    };
    let client = rs_snap7::AsyncClient::connect(options).await.unwrap();
    let other = client.clone();
    let task = tokio::spawn(async move { other.download(&block(), Some(9)).await });
    tokio::time::timeout(Duration::from_secs(2), waiting)
        .await
        .unwrap()
        .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(!client.is_connected().await);
    worker.join().unwrap();
}

/// Socket cancellation may produce either FIN or RST when peer data was unread.
fn assert_closed(stream: &mut TcpStream) {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut byte = [0];
    match stream.read(&mut byte) {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::UnexpectedEof
            ) => {}
        other => panic!("expected a closed socket, got {other:?}"),
    }
}

/// A permit rejected before the first file job does not invalidate a healthy session.
#[test]
fn rejected_catalog_permit_preserves_connection() {
    let (mut options, worker) = peer(|mut stream| {
        assert_eq!(read_frame(&mut stream)[1], 0x80);
    });
    options.rate_limit.max_per_second = 0.01;
    options.rate_limit.behavior = rs_snap7::policy::RateBehavior::Reject;
    let client = Client::connect(options).unwrap();
    assert!(matches!(client.list_blocks(), Err(Error::RateLimited)));
    assert!(client.is_connected());
    client.disconnect().unwrap();
    worker.join().unwrap();
}

/// The async transfer guard also preserves unsent requests during limiter rejection.
#[cfg(feature = "async")]
#[tokio::test(flavor = "current_thread")]
async fn async_rejected_catalog_permit_preserves_connection() {
    let (mut options, worker) = peer(|mut stream| {
        assert_eq!(read_frame(&mut stream)[1], 0x80);
    });
    options.rate_limit.max_per_second = 0.01;
    options.rate_limit.behavior = rs_snap7::policy::RateBehavior::Reject;
    let client = rs_snap7::AsyncClient::connect(options).await.unwrap();
    assert!(matches!(
        client.list_blocks().await,
        Err(Error::RateLimited)
    ));
    assert!(client.is_connected().await);
    client.disconnect().await.unwrap();
    worker.join().unwrap();
}
