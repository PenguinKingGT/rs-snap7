//! Independent S7 wire vectors and hostile packet validation.

use rs_snap7::{Address, Error, protocol::*};

/// Build a response model without calling the request encoder.
fn response(function: u8, count: u8, data: Vec<u8>) -> Pdu {
    Pdu {
        kind: PduType::AckData,
        reference: 1,
        error: 0,
        parameters: vec![function, count],
        data,
    }
}

/// Request bytes agree with upstream's classic S7 wire layout.
#[test]
fn known_setup_read_write_vectors() {
    assert_eq!(
        setup_request(1, 480).unwrap().encode().unwrap(),
        [
            0x32, 1, 0, 0, 0, 1, 0, 8, 0, 0, 0xf0, 0, 0, 1, 0, 1, 1, 0xe0
        ]
    );
    let a = Address::db(1, 0, 2).unwrap();
    let job = read_request(2, &[a]).unwrap();
    assert_eq!(
        job.encode().unwrap(),
        [
            0x32, 1, 0, 0, 0, 2, 0, 14, 0, 0, 4, 1, 0x12, 0x0a, 0x10, 2, 0, 2, 0, 1, 0x84, 0, 0, 0
        ]
    );
    let job = write_request(3, &[(a, &[0xaa, 0xbb])]).unwrap();
    assert_eq!(job.data, [0, 4, 0, 16, 0xaa, 0xbb]);
    assert!(write_request(3, &[(a, &[0])]).is_err());
    assert!(read_request(1, &[]).is_err());
    assert!(read_request(1, &[a; 21]).is_err());
}

/// Length and header checks reject truncation, junk, invalid types, and reserved fields.
#[test]
fn exact_pdu_validation_and_header_errors() {
    let valid = response(4, 1, vec![0xff, 4, 0, 8, 42]).encode().unwrap();
    for n in 0..valid.len() {
        assert!(Pdu::decode(&valid[..n]).is_err());
    }
    let mut trailing = valid.clone();
    trailing.push(0);
    assert!(Pdu::decode(&trailing).is_err());
    for (index, byte) in [(0, 0), (1, 0xff), (2, 1), (6, 0xff), (8, 0xff)] {
        let mut invalid = valid.clone();
        invalid[index] = byte;
        assert!(Pdu::decode(&invalid).is_err());
    }
    let mut pdu = Pdu::decode(&valid).unwrap();
    pdu.error = 1;
    assert!(matches!(pdu.check_response(), Err(Error::Plc(1))));
    assert!(Pdu::job(0, vec![], vec![]).check_response().is_err());
    assert!(Pdu::job(0, vec![0; 65535], vec![]).encode().is_err());
}

/// Stale/lost detection works across rollover instead of using unsigned comparison.
#[test]
fn request_reference_modular_ordering() {
    assert!(validate_reference(1, 1).is_ok());
    assert!(matches!(
        validate_reference(0, 65535),
        Err(Error::StaleReference { .. })
    ));
    assert!(matches!(
        validate_reference(65535, 0),
        Err(Error::LostReference { .. })
    ));
    assert!(matches!(
        validate_reference(42, 41),
        Err(Error::StaleReference { .. })
    ));
}

/// Negotiation rejects invalid values rather than manufacturing a usable session.
#[test]
fn setup_limits_and_response_function_are_checked() {
    let mut pdu = response(0xf0, 0, vec![]);
    pdu.parameters = vec![0xf0, 0, 0, 1, 0, 1, 1, 0xe0];
    assert_eq!(parse_setup(&pdu, 480).unwrap().pdu_length, 480);
    assert!(parse_setup(&pdu, 240).is_err());
    pdu.parameters[3] = 0;
    assert!(parse_setup(&pdu, 480).is_err());
}

/// Mixed successes and errors survive item alignment and exact payload validation.
#[test]
fn multi_read_padding_item_errors_and_truncation() {
    let a = Address::db(1, 0, 1).unwrap();
    let b = Address::db(1, 1, 2).unwrap();
    let pdu = response(
        4,
        3,
        vec![0xff, 4, 0, 8, 42, 0, 0x0a, 0, 0, 0, 0xff, 4, 0, 16, 1, 2],
    );
    let results = parse_read(&pdu, &[a, a, b]).unwrap();
    assert_eq!(results[0].as_ref().unwrap(), &[42]);
    assert!(matches!(results[1], Err(Error::Item(0x0a))));
    assert_eq!(results[2].as_ref().unwrap(), &[1, 2]);
    for n in 0..pdu.data.len() {
        let mut invalid = pdu.clone();
        invalid.data.truncate(n);
        assert!(parse_read(&invalid, &[a, a, b]).is_err());
    }
    let mut invalid = response(4, 1, vec![0xff, 4, 0, 9, 0, 0]);
    assert!(parse_read(&invalid, &[a]).is_err());
    invalid.data = vec![0xff, 4, 0, 8, 0, 0];
    assert!(parse_read(&invalid, &[a]).is_err());
    let writes = parse_write(&response(5, 2, vec![0xff, 5]), 2).unwrap();
    assert!(writes[0].is_ok());
    assert!(matches!(writes[1], Err(Error::Item(5))));
}

/// A deterministic mutation corpus exercises packet decoding without panics.
#[test]
fn arbitrary_short_input_and_declared_lengths_are_bounded() {
    let mut seed = 0x12345678u32;
    for length in 0..512 {
        let mut bytes = Vec::new();
        for _ in 0..length {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            bytes.push(seed as u8);
        }
        let _ = Pdu::decode(&bytes);
    }
}
