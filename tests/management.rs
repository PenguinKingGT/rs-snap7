//! PLC control and clock wire vectors, plus complete local client exchanges.

mod common;
use common::*;
use rs_snap7::{
    Client, ClientConfig, Error,
    codec::datetime::*,
    protocol::{management::*, *},
};
use std::{net::TcpListener, thread};

/// PI parameters follow the pinned upstream/native test vectors.
#[test]
fn pi_service_vectors_and_strict_acknowledgements() {
    for (operation, expected) in [
        (Control::Stop, "29000000000009505f50524f4752414d"),
        (
            Control::HotStart,
            "28000000000000fd000009505f50524f4752414d",
        ),
        (
            Control::ColdStart,
            "28000000000000fd0002432009505f50524f4752414d",
        ),
        (Control::Compress, "28000000000000fd0000055f47415242"),
        (
            Control::CopyRamToRom,
            "28000000000000fd00024550055f4d4f4455",
        ),
    ] {
        let request = control_request(1, operation);
        let hex: String = request
            .parameters
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(hex, expected);
        let mut response = Pdu {
            kind: PduType::AckData,
            reference: 1,
            error: 0,
            parameters: vec![operation.function()],
            data: vec![],
        };
        check_control(&response, operation).unwrap();
        response.parameters = vec![0xff];
        assert!(check_control(&response, operation).is_err());
        response.error = 0x8702;
        assert!(matches!(
            check_control(&response, operation),
            Err(Error::Plc(0x8702))
        ));
    }
}

/// Milliseconds and legacy clock formats are decoded without century guessing outside DT rules.
#[test]
fn clock_formats_and_invalid_dates() {
    let value = DateTime::new(Date::new(2020, 7, 12).unwrap(), 17, 32, 2, 854_000_000).unwrap();
    let request = clock_write_request(1, value).unwrap();
    assert_eq!(
        &request.data[4..],
        &[0, 0x19, 0x20, 7, 0x12, 0x17, 0x32, 2, 0x85, 0x41]
    );
    let mut reply = request;
    reply.parameters = vec![0, 1, 0x12, 8, 0x12, 0x87, 1, 0, 0, 0, 0, 0];
    assert_eq!(parse_clock(&reply).unwrap(), value);
    reply.data = vec![0xff, 9, 0, 8, 0, 0x20, 7, 0x12, 0x17, 0x32, 2, 1];
    assert_eq!(parse_clock(&reply).unwrap().nanosecond(), 0);
    reply.data[5] = 0xff;
    assert!(parse_clock(&reply).is_err());
    let old = DateTime::new(Date::new(1989, 1, 1).unwrap(), 0, 0, 0, 0).unwrap();
    assert!(clock_write_request(0, old).is_err());
}

/// Real client entry points emit the right operation and accept no-data clock ACKs.
#[test]
fn control_and_clock_client_exchanges() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
    let peer = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        negotiate(&mut stream, 480);
        for _ in 0..8 {
            let job = Pdu::decode(&read_frame(&mut stream)[3..]).unwrap();
            let response = if job.kind == PduType::Job {
                Pdu {
                    kind: PduType::AckData,
                    reference: job.reference,
                    error: 0,
                    parameters: vec![job.parameters[0]],
                    data: vec![],
                }
            } else {
                let sub = job.parameters[6];
                let data = if sub == 1 {
                    vec![
                        0xff, 9, 0, 10, 0, 0x19, 0x20, 7, 0x12, 0x17, 0x32, 2, 0x85, 0x41,
                    ]
                } else {
                    vec![0x0a, 0, 0, 0]
                };
                Pdu {
                    kind: PduType::UserData,
                    reference: job.reference,
                    error: 0,
                    parameters: vec![0, 1, 0x12, 8, 0x12, 0x87, sub, 0, 0, 0, 0, 0],
                    data,
                }
            };
            let mut raw = vec![2, 0xf0, 0x80];
            raw.extend(response.encode().unwrap());
            send_frame(&mut stream, &raw);
        }
        read_frame(&mut stream);
    });
    let client = Client::connect(config).unwrap();
    client.plc_stop().unwrap();
    client.plc_hot_start().unwrap();
    client.plc_cold_start().unwrap();
    client.compress().unwrap();
    client.copy_ram_to_rom().unwrap();
    let clock = client.get_plc_datetime().unwrap();
    client.set_plc_datetime(clock).unwrap();
    client.set_plc_system_datetime().unwrap();
    client.disconnect().unwrap();
    peer.join().unwrap();
}
