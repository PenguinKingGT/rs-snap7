//! SZL parsers and bounded continuation tests against a local wire peer.

mod common;
use common::*;
use rs_snap7::{Client, ClientConfig, CpuState, protocol::*, szl::*};
use std::{net::TcpListener, thread};

/// Record 7 wins over catalog and bootloader version records regardless of order.
#[test]
fn order_code_firmware_priority_and_flat_fallback() {
    let mut data = vec![0, 28, 0, 3];
    let mut installed = [0; 28];
    installed[1] = 7;
    installed[25..].copy_from_slice(&[2, 9, 2]);
    let mut catalog = [0; 28];
    catalog[1] = 1;
    catalog[2..22].copy_from_slice(b"6ES7-CPU-CATALOG    ");
    catalog[23..26].copy_from_slice(&[3, 3, 0]);
    let mut loader = [0; 28];
    loader[1] = 0x81;
    loader[25..].copy_from_slice(&[99, 1, 2]);
    data.extend(installed);
    data.extend(catalog);
    data.extend(loader);
    let result = parse_order_code(&Szl {
        id: 0x11,
        index: 0,
        data,
    })
    .unwrap();
    assert_eq!(result.version, [2, 9, 2]);
    assert_eq!(result.code, "6ES7-CPU-CATALOG");
    let mut data = vec![0, 28, 0, 1];
    data.extend_from_slice(b"6ES7-CPU-CATALOG    ");
    data.extend_from_slice(&[0, 4, 6, 0, 0, 0, 0, 0]);
    let result = parse_order_code(&Szl {
        id: 0x11,
        index: 0,
        data,
    })
    .unwrap();
    assert_eq!(result.version, [4, 6, 0]);
}

/// Truncated layouts and record tables fail, while invalid event timestamps remain optional.
#[test]
fn information_parsers_validate_sizes_and_state() {
    let mut cpu = [0; 208];
    cpu[6..9].copy_from_slice(b"PLC");
    cpu[142..145].copy_from_slice(b"123");
    let result = parse_cpu_info(&cpu).unwrap();
    assert_eq!(result.as_name, "PLC");
    assert_eq!(result.serial_number, "123");
    assert!(parse_cpu_info(&cpu[..207]).is_err());
    assert_eq!(
        parse_cp_info(&[1, 224, 0, 8, 0, 3, 0, 4])
            .unwrap()
            .max_connections,
        8
    );
    assert!(parse_cp_info(&[0; 7]).is_err());
    assert_eq!(
        parse_protection(&[0, 1, 0, 2, 0, 3, 0, 4, 0, 5])
            .unwrap()
            .startup,
        5
    );
    assert!(parse_protection(&[0; 9]).is_err());
    assert_eq!(
        parse_cpu_state(&[0, 0, 0, 0, 0, 0, 0, 4]).unwrap(),
        CpuState::Stop
    );
    assert!(parse_cpu_state(&[0; 7]).is_err());
    assert!(parse_cpu_state(&[255; 8]).is_err());
    let event = parse_diagnostic_buffer(&[0; 20]).unwrap();
    assert!(event[0].timestamp.is_none());
    assert!(parse_diagnostic_buffer(&[0; 19]).is_err());
    assert!(
        Szl {
            id: 0,
            index: 0,
            data: vec![0, 0, 0, 1]
        }
        .records()
        .is_err()
    );
    assert!(
        Szl {
            id: 0,
            index: 0,
            data: vec![0, 2, 0, 2, 0]
        }
        .records()
        .is_err()
    );
}

/// Each additional fragment is requested with the prior fragment's data reference.
#[test]
fn fragmented_szl_and_client_information_methods() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
    let peer = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        negotiate(&mut stream, 128);
        loop {
            let raw = read_frame(&mut stream);
            if raw[1] == 0x80 {
                break;
            }
            let mut job = Pdu::decode(&raw[3..]).unwrap();
            assert_eq!(&job.parameters[..7], &[0, 1, 0x12, 4, 0x11, 0x44, 1]);
            let id = u16::from_be_bytes([job.data[4], job.data[5]]);
            let index = u16::from_be_bytes([job.data[6], job.data[7]]);
            assert_eq!(index, 0);
            let body = match id {
                0x1c => {
                    let mut b = vec![0; 208];
                    b[6..9].copy_from_slice(b"PLC");
                    b
                }
                0x424 => vec![0, 0, 0, 0, 0, 0, 0, 4],
                0x131 => vec![1, 224, 0, 8, 0, 3, 0, 4],
                0x232 => vec![0, 1, 0, 2, 0, 3, 0, 4, 0, 5],
                0xa0 => vec![0; 20],
                0x25 => vec![0, 0x81, 0, 10, 3, 1, 0, 0],
                0 => vec![0, 2, 0, 1, 0, 0x1c],
                _ => unreachable!(),
            };
            let parts: Vec<_> = body.chunks(70).collect();
            for (n, part) in parts.iter().enumerate() {
                let more = n + 1 < parts.len();
                let mut payload = Vec::new();
                if n == 0 {
                    payload.extend_from_slice(&id.to_be_bytes());
                    payload.extend_from_slice(&index.to_be_bytes());
                }
                payload.extend_from_slice(part);
                let mut data = vec![0xff, 9];
                data.extend_from_slice(&(payload.len() as u16).to_be_bytes());
                data.extend(payload);
                let reply = Pdu {
                    kind: PduType::UserData,
                    reference: job.reference,
                    error: 0,
                    parameters: vec![
                        0,
                        1,
                        0x12,
                        8,
                        0x12,
                        0x84,
                        1,
                        (n + 1) as u8,
                        0,
                        u8::from(more),
                        0,
                        0,
                    ],
                    data,
                };
                let mut raw = vec![2, 0xf0, 0x80];
                raw.extend(reply.encode().unwrap());
                send_frame(&mut stream, &raw);
                if more {
                    job = Pdu::decode(&read_frame(&mut stream)[3..]).unwrap();
                    assert_eq!(
                        job.parameters,
                        [0, 1, 0x12, 8, 0x12, 0x44, 1, (n + 1) as u8, 0, 0, 0, 0]
                    );
                }
            }
        }
    });
    let client = Client::connect(config).unwrap();
    assert_eq!(client.get_cpu_info().unwrap().as_name, "PLC");
    assert_eq!(client.get_cpu_state().unwrap(), CpuState::Stop);
    assert_eq!(client.get_cp_info().unwrap().max_pdu_length, 480);
    assert_eq!(client.get_protection().unwrap().effective, 3);
    assert_eq!(client.read_diagnostic_buffer().unwrap().len(), 1);
    assert_eq!(client.read_szl_list().unwrap(), [0, 2, 0, 1, 0, 0x1c]);
    let table = client.read_force_table().unwrap();
    assert_eq!(table[0].byte_offset, 10);
    assert!(table[0].value);
    client.disconnect().unwrap();
    peer.join().unwrap();
}

/// Malformed force records are not quietly truncated or interpreted as valid I/O.
#[test]
fn force_table_layout_and_rejected_records() {
    let valid = [0, 0x81, 0, 10, 3, 1, 0, 0];
    assert!(parse_force_table(&[]).unwrap().is_empty());
    let parsed = parse_force_table(&valid).unwrap();
    assert_eq!(parsed[0].area, rs_snap7::Area::Inputs);
    for n in 1..8 {
        assert!(parse_force_table(&valid[..n]).is_err());
    }
    for index in [0, 1, 4, 5] {
        let mut invalid = valid;
        invalid[index] = 255;
        assert!(parse_force_table(&invalid).is_err());
    }
}

/// Native async counterpart of the blocking compatibility scenario.
#[cfg(feature = "async")]
#[tokio::test(flavor = "current_thread")]
async fn async_fragmented_szl_and_client_information_methods() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
    let peer = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        negotiate(&mut stream, 128);
        loop {
            let raw = read_frame(&mut stream);
            if raw[1] == 0x80 {
                break;
            }
            let mut job = Pdu::decode(&raw[3..]).unwrap();
            assert_eq!(&job.parameters[..7], &[0, 1, 0x12, 4, 0x11, 0x44, 1]);
            let id = u16::from_be_bytes([job.data[4], job.data[5]]);
            let index = u16::from_be_bytes([job.data[6], job.data[7]]);
            assert_eq!(index, 0);
            let body = match id {
                0x1c => {
                    let mut b = vec![0; 208];
                    b[6..9].copy_from_slice(b"PLC");
                    b
                }
                0x424 => vec![0, 0, 0, 0, 0, 0, 0, 4],
                0x131 => vec![1, 224, 0, 8, 0, 3, 0, 4],
                0x232 => vec![0, 1, 0, 2, 0, 3, 0, 4, 0, 5],
                0xa0 => vec![0; 20],
                0x25 => vec![0, 0x81, 0, 10, 3, 1, 0, 0],
                0 => vec![0, 2, 0, 1, 0, 0x1c],
                _ => unreachable!(),
            };
            let parts: Vec<_> = body.chunks(70).collect();
            for (n, part) in parts.iter().enumerate() {
                let more = n + 1 < parts.len();
                let mut payload = Vec::new();
                if n == 0 {
                    payload.extend_from_slice(&id.to_be_bytes());
                    payload.extend_from_slice(&index.to_be_bytes());
                }
                payload.extend_from_slice(part);
                let mut data = vec![0xff, 9];
                data.extend_from_slice(&(payload.len() as u16).to_be_bytes());
                data.extend(payload);
                let reply = Pdu {
                    kind: PduType::UserData,
                    reference: job.reference,
                    error: 0,
                    parameters: vec![
                        0,
                        1,
                        0x12,
                        8,
                        0x12,
                        0x84,
                        1,
                        (n + 1) as u8,
                        0,
                        u8::from(more),
                        0,
                        0,
                    ],
                    data,
                };
                let mut raw = vec![2, 0xf0, 0x80];
                raw.extend(reply.encode().unwrap());
                send_frame(&mut stream, &raw);
                if more {
                    job = Pdu::decode(&read_frame(&mut stream)[3..]).unwrap();
                    assert_eq!(
                        job.parameters,
                        [0, 1, 0x12, 8, 0x12, 0x44, 1, (n + 1) as u8, 0, 0, 0, 0]
                    );
                }
            }
        }
    });
    let client = rs_snap7::AsyncClient::connect(config).await.unwrap();
    assert_eq!(client.get_cpu_info().await.unwrap().as_name, "PLC");
    assert_eq!(client.get_cpu_state().await.unwrap(), CpuState::Stop);
    assert_eq!(client.get_cp_info().await.unwrap().max_pdu_length, 480);
    assert_eq!(client.get_protection().await.unwrap().effective, 3);
    assert_eq!(client.read_diagnostic_buffer().await.unwrap().len(), 1);
    assert_eq!(client.read_szl_list().await.unwrap(), [0, 2, 0, 1, 0, 0x1c]);
    let table = client.read_force_table().await.unwrap();
    assert_eq!(table[0].byte_offset, 10);
    assert!(table[0].value);
    client.disconnect().await.unwrap();
    peer.join().unwrap();
}
