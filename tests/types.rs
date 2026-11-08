//! Compatibility vectors and validation for foundational public types.

use rs_snap7::{Address, Area, Error, Parameter, ServerArea, WordLen};
use std::error::Error as _;

/// DB addresses use bit offsets on the wire, even for byte transfers.
#[test]
fn db_address_matches_upstream_s7_any_vector() {
    let address = Address::db(1, 4, 2).unwrap();
    assert_eq!(
        address.encode(),
        [0x12, 0x0a, 0x10, 2, 0, 2, 0, 1, 0x84, 0, 0, 32]
    );
    assert_eq!(address.byte_len(), 2);
    assert_eq!(address.db_number(), 1);
}

/// Counter/timer addresses are indexes and bit addresses are already in bits.
#[test]
fn bit_and_counter_offsets_are_not_scaled() {
    let bit = Address::new(Area::Markers, 0, 19, WordLen::Bit, 1).unwrap();
    assert_eq!(bit.encode()[11], 19);
    let timer = Address::new(Area::Timers, 0, 19, WordLen::Timer, 2).unwrap();
    assert_eq!(timer.encode()[11], 19);
    assert_eq!(timer.byte_len(), 4);
}

/// Full ranges are checked, including valid last-byte transfers.
#[test]
fn rejects_empty_mismatched_and_overflowing_ranges() {
    assert!(Address::db(1, 0, 0).is_err());
    assert!(Address::db(1, 0x1fffff, 1).is_ok());
    assert!(Address::db(1, 0x1fffff, 2).is_err());
    assert!(Address::db(1, u32::MAX, 1).is_err());
    assert!(Address::new(Area::Markers, 1, 0, WordLen::Byte, 1).is_err());
    assert!(Address::new(Area::Timers, 0, 0, WordLen::Byte, 1).is_err());
    assert!(Address::new(Area::Counters, 0, 0, WordLen::Timer, 1).is_err());
}

/// Public identifiers preserve upstream numeric values without ctypes.
#[test]
fn identifiers_roundtrip_and_reject_unknown_codes() {
    for code in [0x81, 0x82, 0x83, 0x84, 0x1c, 0x1d] {
        assert_eq!(Area::try_from(code).unwrap() as u8, code);
    }
    assert!(Area::try_from(0).is_err());
    assert_eq!(ServerArea::Db as u8, 5);
    assert_eq!(Parameter::PduRequest as u8, 10);
    assert_eq!(Area::Counters.word_len(), WordLen::Counter);
    for code in [1, 2, 3, 4, 5, 6, 7, 8, 0x1c, 0x1d, 0x1e, 0x1f] {
        assert_eq!(WordLen::try_from(code).unwrap() as u8, code);
    }
    assert!(WordLen::try_from(0xff).is_err());
}

/// Codes and underlying causes remain observable to applications.
#[test]
fn structured_errors_preserve_codes_and_io_causes() {
    assert!(
        Error::Item(0x0a)
            .to_string()
            .contains("object does not exist")
    );
    assert!(Error::Plc(0x8702).to_string().contains("8702"));
    assert!(matches!(
        Error::from(std::io::Error::from(std::io::ErrorKind::TimedOut)),
        Error::Timeout
    ));
    let error = Error::from(std::io::Error::from(std::io::ErrorKind::ConnectionReset));
    assert!(error.source().is_some());
    assert!(Error::Item(0xfe).to_string().contains("unknown"));
}
