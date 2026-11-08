//! Strict LOGO VM syntax and complete address range checks.

use rs_snap7::{WordLen, logo::VmAddress};

/// Valid byte/word/dword/bit addresses preserve byte offsets and signed widths.
#[test]
fn vm_address_valid_forms_and_range_end() {
    for (text, word, bit) in [
        ("V10", WordLen::Byte, None),
        ("VW10", WordLen::Word, None),
        ("VD10", WordLen::DWord, None),
        ("V10.3", WordLen::Bit, Some(3)),
    ] {
        let a: VmAddress = text.parse().unwrap();
        assert_eq!(a.byte_offset(), 10);
        assert_eq!(a.word_len(), word);
        assert_eq!(a.bit(), bit);
    }
    assert!("V2097151.7".parse::<VmAddress>().is_ok());
    assert!("VW2097151".parse::<VmAddress>().is_err());
}

/// Reject partial regex matches accepted by permissive upstream address parsing.
#[test]
fn vm_address_rejects_malformed_overflowing_and_partial_forms() {
    for text in [
        "",
        "V",
        "V-1",
        "V+1",
        "V0.8",
        "V0.33",
        "V0.3junk",
        "VW0.1",
        "VD",
        "v10",
        "V10junk",
        "V99999999999999",
        "V2097152",
    ] {
        assert!(text.parse::<VmAddress>().is_err(), "{text}");
    }
}
