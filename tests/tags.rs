//! Tag compatibility vectors and metadata import failure cases.

use rs_snap7::{
    Area,
    tags::{DataType, Tag, Value},
};

#[test]
fn dialects_canonicalize_and_round_trip() {
    for (text, canonical, size) in [
        ("DB1.DBX0.3:BOOL", "DB1:0.3:BOOL", 1),
        ("%db2.dbw10:int", "DB2:10:INT", 2),
        ("DB1:0:REAL[5]", "DB1:0:REAL[5]", 20),
        ("DB1,S10.20", "DB1:10:STRING[20]", 22),
        ("DB1,WS10.10", "DB1:10:WSTRING[10]", 24),
        ("DB3,DI22", "DB3:22:DINT", 4),
        ("MW20:WORD", "M20:WORD", 2),
        ("I7.1:BOOL", "I7.1:BOOL", 1),
    ] {
        let tag: Tag = text.parse().unwrap();
        assert_eq!(tag.size(), size);
        assert_eq!(tag.to_string(), canonical);
        assert_eq!(canonical.parse::<Tag>().unwrap(), tag);
    }
    assert_eq!(Tag::parse("EW22", false).unwrap().area(), Area::Inputs);
    assert_eq!(Tag::parse("AR24", false).unwrap().area(), Area::Outputs);
    assert_eq!(Tag::parse("M7.1", false).unwrap().bit(), 1);
}

#[test]
fn malformed_addresses_never_become_valid_targets() {
    for text in [
        "",
        "IW22",
        "M7.1",
        "DB1,X0.8",
        "DB-1,R4",
        "DB1,R-4",
        "DB1,R4.2",
        "DB1:0:INT[0]",
        "DB1:0:UNKNOWN",
        "DB1:2097152:BYTE",
        "DB1:0.1:REAL",
        "DB1:0:STRING",
        "DB1:0:STRING[255]",
        "DB1:0:WSTRING[16383]",
        "DB1:0:INT[999999999999999999999]",
        "DB1:0:INT[2][2]",
        "é:INT",
        "é,22",
        "DB1,B2junk",
        "DB1,X2",
        "DB1,S0",
    ] {
        assert!(text.parse::<Tag>().is_err(), "{text}");
    }
}

#[test]
fn values_preserve_types_units_ranges_and_array_counts() {
    for (dtype, value) in [
        ("SINT", Value::Signed(-128)),
        ("INT", Value::Signed(-32768)),
        ("LINT", Value::Signed(i64::MIN)),
        ("ULINT", Value::Unsigned(u64::MAX)),
        ("REAL", Value::Real(-12.5)),
        ("LREAL", Value::LReal(0.125)),
        ("BOOL", Value::Bool(true)),
        ("WCHAR", Value::Character('中')),
        ("STRING[8]", Value::Text("café".into())),
        ("WSTRING[8]", Value::Text("中文".into())),
        ("FSTRING[8]", Value::Text("abc".into())),
        ("TIME", Value::Signed(-15)),
        ("TOD", Value::Unsigned(86_399_999)),
        ("LTOD", Value::Unsigned(1)),
    ] {
        let tag: Tag = format!("DB1:0:{dtype}").parse().unwrap();
        let raw = tag.encode(&value).unwrap();
        assert_eq!(tag.decode(&raw).unwrap(), value);
        assert!(tag.decode(&raw[..raw.len() - 1]).is_err());
    }
    let tag: Tag = "DB1:0:INT[3]".parse().unwrap();
    let value = Value::Array(vec![
        Value::Signed(1),
        Value::Signed(-2),
        Value::Signed(32767),
    ]);
    assert_eq!(tag.encode(&value).unwrap(), [0, 1, 255, 254, 127, 255]);
    assert_eq!(tag.decode(&tag.encode(&value).unwrap()).unwrap(), value);
    for (text, value) in [
        ("DB1:0:INT", Value::Signed(32768)),
        ("DB1:0:BYTE", Value::Unsigned(256)),
        ("DB1:0:INT", Value::Bool(false)),
        ("DB1:0:STRING[2]", Value::Text("long".into())),
        ("DB1:0:S5TIME", Value::Unsigned(10)),
        ("DB1:0:TOD", Value::Unsigned(86_400_000)),
    ] {
        assert!(text.parse::<Tag>().unwrap().encode(&value).is_err());
    }
    assert!(tag.encode(&Value::Array(vec![])).is_err());
}

#[test]
fn symbolic_browse_metadata_is_preserved_and_validated() {
    use rs_snap7::tags::{BrowseVariable, from_browse};
    let tag = Tag::from_access_string("8A0E0001.A.1", DataType::Real, 1, 42).unwrap();
    assert_eq!(tag.db_number(), 1);
    assert_eq!(tag.symbolic().unwrap().path, [0x8a0e0001, 10, 1]);
    let variables = vec![BrowseVariable {
        name: "speed".into(),
        tag,
        lid: Some(10),
        symbol_crc: 42,
    }];
    let tags = from_browse(&variables).unwrap();
    assert_eq!(tags["speed"].symbolic().unwrap().crc, 42);
    assert!(from_browse(&[variables[0].clone(), variables[0].clone()]).is_err());
    assert!(Tag::from_access_string("8A0E0001..A", DataType::Int, 1, 0).is_err());
}

#[cfg(feature = "tag-import")]
#[test]
fn imports_accept_upstream_schemas_and_reject_unsafe_metadata() {
    use rs_snap7::tags::import::*;
    let csv = load_csv("tag,db,offset,type,bit\n\"motor,speed\",1,0,REAL,\nenabled,1,4.3,BOOL,3\n")
        .unwrap();
    assert_eq!(csv["enabled"].bit(), 3);
    assert_eq!(csv["motor,speed"].datatype(), DataType::Real);
    let json = load_json(r#"{"enabled":{"db":1,"offset":"4.3","type":"BOOL"},"speed":{"db":1,"offset":0,"type":"REAL"}}"#).unwrap();
    assert_eq!(json["enabled"].offset(), 4);
    let xml = load_tia_xml(r#"<Document xmlns="urn:tia"><AttributeList><Number>2</Number></AttributeList><Member Name="Motor" Datatype="Struct"><Member Name="Speed" Datatype="Real" Offset="0"/><Member Name="Run" Datatype="Bool" Offset="4.1"/></Member></Document>"#).unwrap();
    assert_eq!(xml["Motor.Run"].db_number(), 2);
    for text in [
        "{}",
        r#"{"x":{"db":1,"offset":0.5,"type":"INT"}}"#,
        r#"{"x":{"db":1,"offset":0,"type":"STRING"}}"#,
    ] {
        if text != "{}" {
            assert!(load_json(text).is_err());
        }
    }
    assert!(load_csv("tag,db,offset,type\nx,1,0,INT\nx,1,2,INT").is_err());
    assert!(load_tia_xml(r#"<D><Number>1</Number><Member Name="x" Datatype="Int"/></D>"#).is_err());
    assert!(load_tia_xml("<!DOCTYPE x [<!ENTITY a 'bad'>]><D>&a;</D>").is_err());
}
