//! Pure DB layout/snapshot validation independent of a PLC.
use rs_snap7::{
    Area,
    db::{Db, DbConfig, Layout},
    tags::Value,
};

fn table() -> Db {
    let layout = Layout::parse(
        "4 ID INT\n6 low BOOL\n6.7 high BOOL\n8 value REAL # comment",
        4,
    )
    .unwrap();
    Db::new(
        DbConfig {
            area: Area::Db,
            db_number: 1,
            start: 20,
            row_size: 8,
            row_gap: 2,
            rows: 2,
            writable_start: 2,
        },
        layout,
        vec![0; 18],
    )
    .unwrap()
}

#[test]
fn rows_export_find_and_byte_changes_preserve_padding() {
    let mut db = table();
    assert_eq!(db.len(), 2);
    db.set(1, "ID", &Value::Signed(42)).unwrap();
    db.set(1, "high", &Value::Bool(true)).unwrap();
    db.set(1, "low", &Value::Bool(true)).unwrap();
    assert_eq!(db.data()[12], 0x81);
    assert_eq!(db.find_row("ID", &Value::Signed(42)).unwrap(), Some(1));
    assert!(!db.unchanged(1).unwrap());
    assert_eq!(db.changed_ranges(1).unwrap(), vec![12..13]);
    assert_eq!(db.export().unwrap()[1]["ID"], Value::Signed(42));
    assert_eq!(&db.data()[8..10], &[0, 0]);
    db.set(1, "low", &Value::Bool(false)).unwrap();
    assert_eq!(db.data()[12], 0x80);
    let original = db.data().to_vec();
    assert!(db.set(1, "ID", &Value::Signed(32768)).is_err());
    assert_eq!(db.data(), original);
    db.replace(vec![0; 18]).unwrap();
    assert!(db.unchanged(1).unwrap());
    assert!(db.replace(vec![]).is_err());
    assert!(db.get(2, "ID").is_err());
    assert!(db.get(0, "missing").is_err());
}

#[test]
fn invalid_layouts_and_placements_fail_without_panics() {
    for spec in [
        "",
        "0 x UNKNOWN",
        "0 x STRING",
        "0 x INT\n1 y BYTE",
        "0.1 x BOOL\n0.1 y BOOL",
        "0 x BYTE\n2 x BYTE",
        "-1 x BYTE",
        "0 x",
        "0.9 x BOOL",
    ] {
        assert!(Layout::parse(spec, 0).is_err(), "{spec}");
    }
    assert!(Layout::parse("0 x INT", 4).is_err());
    let mut config = table().config().clone();
    config.rows = usize::MAX;
    assert!(Db::new(config, Layout::parse("0 x INT", 0).unwrap(), vec![]).is_err());
}
