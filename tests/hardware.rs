//! Explicit opt-in real-PLC acceptance; normal tests never contact hardware.
//!
//! See docs/testing.md for fixture/environment requirements and report collection.

#![cfg(feature = "blocking")]

use rs_snap7::blocking::Client;
use rs_snap7::{ClientConfig, Error, Result};
use std::{
    io::Write,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Require an environment variable rather than selecting an implicit PLC/fixture.
fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("missing {name}; see docs/testing.md"))
}
/// Open only an explicitly enabled PLC endpoint with recorded fixture metadata.
fn fixture() -> (Client, u16, u32, usize) {
    assert_eq!(required("SNAP7_RUN_HARDWARE"), "1");
    let model = required("SNAP7_PLC_MODEL");
    let firmware = required("SNAP7_PLC_FIRMWARE");
    assert!(!model.is_empty() && !firmware.is_empty());
    let endpoint = required("SNAP7_PLC_ENDPOINT")
        .parse()
        .expect("HOST:PORT numeric endpoint");
    let rack = std::env::var("SNAP7_PLC_RACK")
        .unwrap_or_else(|_| "0".into())
        .parse()
        .unwrap();
    let slot = std::env::var("SNAP7_PLC_SLOT")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .unwrap();
    let mut config = ClientConfig::new(endpoint, rack, slot).unwrap();
    config.transport.timeout = Duration::from_secs(3);
    let client = Client::connect(config).unwrap();
    let db = required("SNAP7_FIXTURE_DB").parse().unwrap();
    let start = required("SNAP7_FIXTURE_START").parse().unwrap();
    let size: usize = required("SNAP7_FIXTURE_SIZE").parse().unwrap();
    assert!((1..=4096).contains(&size));
    (client, db, start, size)
}
/// Record acceptance context to stdout and optionally a caller-selected report file.
fn report(client: &Client, scenario: &str) -> Result<()> {
    let cpu = client.get_cpu_info()?;
    let order = client.get_order_code()?;
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::InvalidInput("clock predates epoch"))?
        .as_secs();
    let text = format!(
        "Scenario: {scenario}\nUTC Unix seconds: {epoch}\nEndpoint: {}\nDeclared CPU: {}\nDeclared firmware: {}\nObserved CPU: {cpu:?}\nObserved order: {order:?}\nResult: passed\n\n",
        required("SNAP7_PLC_ENDPOINT"),
        required("SNAP7_PLC_MODEL"),
        required("SNAP7_PLC_FIRMWARE")
    );
    println!("{text}");
    if let Ok(path) = std::env::var("SNAP7_HARDWARE_REPORT") {
        std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)?
            .write_all(text.as_bytes())?;
    }
    Ok(())
}
#[test]
#[ignore = "requires explicit real PLC endpoint, CPU/firmware and dedicated fixture"]
fn hardware_read_diagnostics_and_reconnect() {
    let (client, db, start, size) = fixture();
    assert_eq!(client.db_read(db, start, size).unwrap().len(), size);
    let _ = client.get_cpu_state().unwrap();
    client.disconnect().unwrap();
    client.reconnect().unwrap();
    assert_eq!(client.db_read(db, start, size).unwrap().len(), size);
    report(&client, "read/diagnostics/reconnect").unwrap();
    client.disconnect().unwrap();
}
/// Best-effort restoration also runs if the write/read assertion panics.
struct Restore {
    client: Client,
    db: u16,
    start: u32,
    original: Vec<u8>,
    armed: bool,
}
impl Drop for Restore {
    fn drop(&mut self) {
        if self.armed
            && let Err(error) = self.client.db_write(self.db, self.start, &self.original)
        {
            eprintln!("FIXTURE RESTORATION FAILED: {error}");
        }
    }
}
#[test]
#[ignore = "mutates only a dedicated fixture; additionally requires SNAP7_ALLOW_HARDWARE_WRITES=1"]
fn hardware_fixture_write_round_trip_and_restore() {
    assert_eq!(required("SNAP7_ALLOW_HARDWARE_WRITES"), "1");
    let (client, db, start, size) = fixture();
    let original = client.db_read(db, start, size).unwrap();
    let mut restore = Restore {
        client: client.clone(),
        db,
        start,
        original,
        armed: true,
    };
    let pattern: Vec<_> = restore.original.iter().map(|byte| byte ^ 0x5a).collect();
    client.db_write(db, start, &pattern).unwrap();
    assert_eq!(client.db_read(db, start, size).unwrap(), pattern);
    client.db_write(db, start, &restore.original).unwrap();
    assert_eq!(client.db_read(db, start, size).unwrap(), restore.original);
    restore.armed = false;
    report(&client, "fixture write/read/restore").unwrap();
    client.disconnect().unwrap();
}
