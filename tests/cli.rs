//! CLI process exit status, stable output and emulator/demo lifecycles.
#![cfg(feature = "cli")]
use rs_snap7::{
    Area,
    server::{Server, ServerConfig},
};
use std::{
    io::{BufRead, BufReader},
    process::{Command, Stdio},
};
fn invoke(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_s7"))
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn help_invalid_values_and_invalid_commands_return_expected_exit_codes() {
    assert!(invoke(&["--help"]).status.success());
    assert!(invoke(&["unknown"]).status.code().unwrap() != 0);
    let output = invoke(&["write", "--host", "invalid.example", "DB1:0:INT", "32768"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("overflow"));
    let output = invoke(&["read", "M7.1"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("ambiguous"));
}
#[test]
fn read_write_dump_and_info_work_against_local_emulator() {
    let server = Server::start(ServerConfig::new("127.0.0.1:0".parse().unwrap())).unwrap();
    server.register_area(Area::Db, 1, vec![0; 64]).unwrap();
    let port = server.local_addr().port().to_string();
    let written = invoke(&["write", "--port", &port, "DB1:0:INT", "-42"]);
    assert!(written.status.success(), "{:?}", written);
    let read = invoke(&["read", "--port", &port, "DB1:0:INT"]);
    assert!(read.status.success());
    assert_eq!(String::from_utf8(read.stdout).unwrap().trim(), "-42");
    let dump = invoke(&["dump", "--port", &port, "--db", "1", "--length", "2"]);
    assert!(dump.status.success());
    assert!(String::from_utf8(dump.stdout).unwrap().contains("ff d6"));
    let info = invoke(&["info", "--port", &port]);
    assert!(info.status.success(), "{:?}", info);
    let value: serde_json::Value = serde_json::from_slice(&info.stdout).unwrap();
    assert_eq!(value["state"], "Run");
}
#[test]
fn server_command_reports_bound_endpoint_and_stops_within_duration() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_s7"))
        .args(["server", "--bind", "127.0.0.1:0", "--duration-ms", "200"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert!(line.starts_with("Listening on 127.0.0.1:"));
    assert!(child.wait().unwrap().success());
}
#[cfg(feature = "demo")]
#[test]
fn demo_sensor_db_is_read_only_and_control_db_is_writable() {
    let mut demo =
        rs_snap7::demo::Demo::start(ServerConfig::new("127.0.0.1:0".parse().unwrap())).unwrap();
    demo.sample().unwrap();
    let client = rs_snap7::Client::connect(
        rs_snap7::ClientConfig::new(demo.server().local_addr(), 0, 1).unwrap(),
    )
    .unwrap();
    let cpu = client.db_read_real(1, 0).unwrap();
    assert!(cpu.is_finite() && (0.0..=100.0).contains(&cpu));
    let memory = client.db_read_real(1, 4).unwrap();
    assert!(memory.is_finite() && (0.0..=100.0).contains(&memory));
    assert!(matches!(
        client.db_write_byte(1, 0, 1),
        Err(rs_snap7::Error::Item(3))
    ));
    client.db_write_int(2, 2, 123).unwrap();
    assert_eq!(client.db_read_int(2, 2).unwrap(), 123);
    client.disconnect().unwrap();
}
