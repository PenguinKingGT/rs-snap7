//! Opt-in cross-implementation acceptance against the pinned local Python tree.
#![cfg(feature = "server")]
use rs_snap7::{
    Area, BlockType, Client, ClientConfig,
    server::{Server, ServerConfig},
};
use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const BASELINE: &str = "da93f21a06ee5565f7793323ddbdb542f31ec9a2";
/// Validate the exact upstream revision before launching any Python peer.
fn command(mode: &str) -> Command {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let reference = std::env::var_os("SNAP7_PYTHON_REFERENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("python-snap7"));
    let output = Command::new("git")
        .args(["-C"])
        .arg(&reference)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("upstream Git checkout required");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        BASELINE,
        "upstream baseline differs"
    );
    let python = std::env::var_os("SNAP7_PYTHON").unwrap_or_else(|| "python3".into());
    let mut command = Command::new(python);
    command
        .arg(root.join("tools/python_peer.py"))
        .arg(mode)
        .arg("--reference")
        .arg(reference);
    command
}
/// Kill on panic/drop so failed compatibility scenarios cannot leak child servers.
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
/// Wait for a child with a hard process deadline, including failure diagnostics.
fn wait(child: &mut Child) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "Python peer exceeded process deadline"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[tokio::test]
#[ignore = "requires pinned python-snap7 and Python >=3.10; see docs/testing.md"]
async fn rust_client_interoperates_with_pinned_python_server() {
    let child = command("server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);
    let stdout = guard.0.stdout.take().unwrap();
    // Read readiness on a separate thread so a broken peer cannot hang this test.
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line);
        let _ = send.send(result.map(|_| line));
    });
    let line = receive
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let port: u16 = line.trim().parse().unwrap();
    let client = Client::connect(
        ClientConfig::new(format!("127.0.0.1:{port}").parse().unwrap(), 0, 1).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        client.db_read(1, 0, 300).await.unwrap(),
        (0..300).map(|i| i as u8).collect::<Vec<_>>()
    );
    client.db_write_int(1, 10, -1234).await.unwrap();
    assert_eq!(client.db_read_int(1, 10).await.unwrap(), -1234);
    assert_eq!(
        client
            .get_block_info(BlockType::Db, 1)
            .await
            .unwrap()
            .mc7_size,
        600
    );
    assert_eq!(
        client.upload(1).await.unwrap(),
        client.db_read(1, 0, 600).await.unwrap()
    );
    assert_eq!(
        client.list_block_entries(BlockType::Db).await.unwrap()[0].number,
        1
    );
    let offline = client.full_upload(BlockType::Db, 1).await.unwrap();
    let mut sample = offline[..128].to_vec();
    rs_snap7::codec::write(&mut sample, 8, 128u32).unwrap();
    rs_snap7::codec::write(&mut sample, 34, 92u16).unwrap();
    client.download(&sample, Some(1)).await.unwrap();
    // Python's emulator stores download bytes verbatim, including compact header.
    assert_eq!(client.db_read(1, 0, 128).await.unwrap(), sample);
    client.disconnect().await.unwrap();
    guard.0.stdin.take().unwrap().write_all(b"stop\n").unwrap();
    assert!(wait(&mut guard.0).success());
}
#[test]
#[ignore = "requires pinned python-snap7 and Python >=3.10; see docs/testing.md"]
fn python_client_interoperates_with_rust_server() {
    let server = Server::start(ServerConfig::new("127.0.0.1:0".parse().unwrap())).unwrap();
    let memory = server
        .register_area(Area::Db, 1, (0..600).map(|i| i as u8).collect())
        .unwrap();
    let child = command("client")
        .arg("--port")
        .arg(server.local_addr().port().to_string())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);
    assert!(wait(&mut guard.0).success());
    assert_eq!(memory.snapshot().unwrap()[10..12], (-1234i16).to_be_bytes());
}
