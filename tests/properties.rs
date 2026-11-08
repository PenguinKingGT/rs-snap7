//! Deterministic property-style invariants and malformed-input regression sweeps.
use rs_snap7::{
    blocks::{BlockInfo, CompactBlockInfo},
    codec,
    protocol::Pdu,
    tags::Tag,
};
/// Fixed seed and specified xorshift keep all failures reproducible.
fn random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}
#[test]
fn numeric_encoding_matches_independent_big_endian_bytes_for_generated_values() {
    let mut state = 0x123456789abcdef0;
    for _ in 0..10000 {
        let value = random(&mut state);
        let mut bytes = [0; 8];
        codec::write(&mut bytes, 0, value).unwrap();
        assert_eq!(bytes, value.to_be_bytes());
        assert_eq!(codec::read::<u64>(&bytes, 0).unwrap(), value);
        let float = f64::from_bits(value);
        codec::write(&mut bytes, 0, float).unwrap();
        assert_eq!(bytes, value.to_be_bytes());
        assert_eq!(codec::read::<f64>(&bytes, 0).unwrap().to_bits(), value);
    }
}
#[test]
fn untrusted_packets_headers_strings_and_addresses_never_panic() {
    let mut state = 0xd15ca11ab1e;
    for n in 0..20000 {
        let len = (random(&mut state) % 257) as usize;
        let bytes: Vec<_> = (0..len).map(|_| random(&mut state) as u8).collect();
        let _ = Pdu::decode(&bytes);
        let _ = BlockInfo::decode(&bytes);
        let _ = CompactBlockInfo::decode(&bytes);
        let _ = codec::get_string(&bytes, 0);
        let _ = codec::get_wstring(&bytes, 0);
        let _ = codec::datetime::get_dt(&bytes, 0);
        let _ = codec::datetime::get_dtl(&bytes, 0);
        let _ = codec::datetime::get_ldt(&bytes, 0);
        let text = String::from_utf8_lossy(&bytes);
        let _ = Tag::parse(&text, n % 2 == 0);
        #[cfg(feature = "ppi")]
        {
            let _ = rs_snap7::ppi::frame::decode(&bytes);
        }
        #[cfg(feature = "discovery")]
        {
            let _ = rs_snap7::discovery::wire::parse_response(&bytes, [2, 0, 0, 0, 0, 1], 1);
        }
        #[cfg(feature = "tag-import")]
        if n < 1000 {
            let _ = rs_snap7::tags::import::load_json(&text);
            let _ = rs_snap7::tags::import::load_csv(&text);
            let _ = rs_snap7::tags::import::load_tia_xml(&text);
        }
    }
}
