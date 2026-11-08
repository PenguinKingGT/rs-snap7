//! Scalar and text compatibility tests, including malformed and boundary inputs.

use rs_snap7::codec::*;

/// Numeric encodings are independently checked against big-endian wire bytes.
#[test]
fn numeric_vectors_offsets_and_special_floats() {
    let mut data = [0xaa; 20];
    set_int(&mut data, 1, -32768).unwrap();
    assert_eq!(&data[0..4], &[0xaa, 0x80, 0, 0xaa]);
    assert_eq!(get_int(&data, 1).unwrap(), i16::MIN);
    set_lint(&mut data, 2, i64::MIN).unwrap();
    assert_eq!(get_lint(&data, 2).unwrap(), i64::MIN);
    set_ulint(&mut data, 3, u64::MAX).unwrap();
    assert_eq!(get_ulint(&data, 3).unwrap(), u64::MAX);
    for bits in [
        0,
        1,
        u32::MAX,
        0x80000000,
        0x7f800000,
        0xff800000,
        0x7fc01234,
    ] {
        let mut raw = [0; 4];
        set_real(&mut raw, 0, f32::from_bits(bits)).unwrap();
        assert_eq!(raw, bits.to_be_bytes());
        assert_eq!(get_real(&raw, 0).unwrap().to_bits(), bits);
    }
    for bits in [0, 1, u64::MAX, 0x8000000000000000] {
        let mut raw = [0; 8];
        set_lreal(&mut raw, 0, f64::from_bits(bits)).unwrap();
        assert_eq!(get_lreal(&raw, 0).unwrap().to_bits(), bits);
    }
}

/// Every signed 16-bit bit pattern round-trips without corrupting neighbors.
#[test]
fn all_i16_values_roundtrip() {
    for value in i16::MIN..=i16::MAX {
        let mut data = [0xaa; 4];
        write(&mut data, 1, value).unwrap();
        assert_eq!(read::<i16>(&data, 1).unwrap(), value);
        assert_eq!((data[0], data[3]), (0xaa, 0xaa));
    }
}

/// Bit updates preserve other flags and reject indexes outside the byte.
#[test]
fn bool_updates_preserve_neighbors_and_validate_indexes() {
    for previous in 0..=255 {
        for bit in 0..8 {
            let mut data = [previous];
            set_bool(&mut data, 0, bit, true).unwrap();
            assert_eq!(data[0], previous | (1 << bit));
            assert!(get_bool(&data, 0, bit).unwrap());
            set_bool(&mut data, 0, bit, false).unwrap();
            assert_eq!(data[0], previous & !(1 << bit));
        }
    }
    assert!(get_bool(&[0], 0, 8).is_err());
    assert!(set_bool(&mut [0], 0, 255, true).is_err());
}

/// Invalid buffer sizes and arithmetic must return errors before modifying data.
#[test]
fn short_and_overflowing_ranges_are_atomic() {
    let mut data = [0xaa; 3];
    assert!(set_dword(&mut data, 0, 1).is_err());
    assert!(set_word(&mut data, usize::MAX, 1).is_err());
    assert!(get_lword(&data, 0).is_err());
    assert!(get_byte(&data, usize::MAX).is_err());
    assert_eq!(data, [0xaa; 3]);
    assert!(u32::decode(&[0]).is_err());
}

/// Default STRING encoding follows Latin-1; UTF-8 lengths count bytes.
#[test]
fn string_headers_latin1_utf8_and_padding() {
    let mut data = [0xaa; 10];
    set_string(&mut data, 1, "hé", 5).unwrap();
    assert_eq!(&data[1..8], &[5, 2, b'h', 0xe9, 0, 0, 0]);
    assert_eq!(get_string(&data, 1).unwrap(), "hé");
    set_string_with_encoding(&mut data, 0, "中", 4, TextEncoding::Utf8).unwrap();
    assert_eq!(data[1], 3);
    assert_eq!(
        get_string_with_encoding(&data, 0, TextEncoding::Utf8).unwrap(),
        "中"
    );
    set_fstring(&mut data, 0, "x", 4).unwrap();
    assert_eq!(get_fstring(&data, 0, 4, false).unwrap(), "x   ");
    assert_eq!(get_fstring(&data, 0, 4, true).unwrap(), "x");
    assert_eq!(get_string(&[2, 0], 0).unwrap(), "");
}

/// Malformed headers and unrepresentable input are rejected atomically.
#[test]
fn invalid_strings_do_not_modify_buffers() {
    let mut data = [0xaa; 10];
    assert!(set_string(&mut data, 0, "中", 4).is_err());
    assert!(set_string(&mut data, 0, "long", 2).is_err());
    assert!(set_string(&mut data, 0, "", 255).is_err());
    assert!(set_string(&mut data, 9, "", 0).is_err());
    assert_eq!(data, [0xaa; 10]);
    for malformed in [&[2, 3, 0, 0][..], &[255, 0][..], &[2, 2, b'x'][..], &[][..]] {
        assert!(get_string(malformed, 0).is_err());
    }
    assert!(get_string_with_encoding(&[1, 1, 0xff], 0, TextEncoding::Utf8).is_err());
}

/// WCHAR/WSTRING use UTF-16BE with upstream's BMP-only write restriction.
#[test]
fn wide_characters_and_wstring_limits() {
    let mut data = [0xaa; 12];
    set_wstring(&mut data, 1, "中é", 3).unwrap();
    assert_eq!(&data[1..9], &[0, 3, 0, 2, 0x4e, 0x2d, 0, 0xe9]);
    assert_eq!(get_wstring(&data, 1).unwrap(), "中é");
    set_wchar(&mut data, 0, '中').unwrap();
    assert_eq!(get_wchar(&data, 0).unwrap(), '中');
    assert!(set_wchar(&mut data, 0, '😀').is_err());
    assert!(set_wstring(&mut data, 0, "😀", 2).is_err());
    assert!(set_wstring(&mut data, 0, "", 16383).is_err());
    assert!(get_wstring(&[0, 1, 0, 2], 0).is_err());
    assert!(get_wstring(&[0, 1, 0, 1, 0xd8, 0], 0).is_err());
    assert!(get_wchar(&[0xd8, 0], 0).is_err());
    set_char(&mut data, 0, 'ÿ').unwrap();
    assert_eq!(get_char(&data, 0).unwrap(), 'ÿ');
    assert!(set_char(&mut data, 0, '中').is_err());
}
