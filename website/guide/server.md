# Server Setup for Testing

Enable `server` to exercise a client without PLC hardware. The example starts on
an ephemeral loopback port, registers DB1 and markers, and verifies a typed write.
Add `features = ["server"]` to your rs-snap7 dependency.

```rust
use rs_snap7::{Area, Client, ClientConfig, Result, server::{Server, ServerConfig}};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let mut server = Server::start(ServerConfig::new("127.0.0.1:0".parse().unwrap()))?;
    let db = server.register_area(Area::Db, 1, vec![0; 64])?;
    server.register_area(Area::Markers, 0, vec![0; 16])?;
    let client = Client::connect(ClientConfig::new(server.local_addr(), 0, 1)?).await?;
    client.db_write_int(1, 0, -42).await?;
    assert_eq!(client.db_read_int(1, 0).await?, -42);
    assert_eq!(&db.snapshot()?[..2], &(-42i16).to_be_bytes());
    client.disconnect().await?;
    tokio::task::spawn_blocking(move || server.stop()).await.expect("server shutdown");
    Ok(())
}
```

`Memory::with_mut` updates registered bytes; `set_read_only(true)` rejects client
writes. Call `register_area` again with different DB numbers for multiple blocks.
Non-DB areas use DB number zero. Registered buffers are owned and lock-protected.

For a fixed-port service, use `127.0.0.1:1102` or the
[CLI server command](optional-products.md). The Rust server runs its session
threads internally; there is no Python-style `mainloop` requirement.

The upstream [server tutorial](https://python-snap7.readthedocs.io/en/latest/server.html)
provides the task structure. This emulator does not execute PLC programs, and
passing local tests does not establish real-CPU compatibility.
