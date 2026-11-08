# Getting Started

## Try the local emulator

Clone the repository and install a Rust toolchain supporting the 2024 edition.
All-feature/demo builds require Rust 1.95 or newer. Run these commands from the
repository root; no PLC hardware is required.

```sh
git clone https://github.com/PenguinKingGT/rs-snap7.git
cd rs-snap7
cargo run --features cli --bin s7 -- server --bind 127.0.0.1:1102
```

Leave the server running, then open another terminal in the same directory:

```sh
cargo run --features cli --bin s7 -- write --port 1102 DB1:0:INT -42
cargo run --features cli --bin s7 -- read --port 1102 DB1:0:INT
cargo run --example read_db -- 127.0.0.1:1102 DB1:0:INT
```

The read commands print `-42`. Stop the emulator with Ctrl-C.

## Add the library

The crate is not yet published. Use the Git repository, or a local path to your
checkout. Pin a revision in applications that need reproducible builds.

```toml
[dependencies]
rs-snap7 = { git = "https://github.com/PenguinKingGT/rs-snap7.git" }
tokio = { version = "1", features = ["macros", "rt", "net", "time"] }
```

## Connect and read

This example targets the local emulator. Replace its endpoint, rack, and slot
with your PLC settings when using hardware.

```rust
use rs_snap7::{Client, ClientConfig, Result};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let endpoint = "127.0.0.1:1102".parse().expect("valid endpoint");
    let client = Client::connect(ClientConfig::new(endpoint, 0, 1)?).await?;
    client.db_write_int(1, 0, -42).await?;
    let value = client.db_read_int(1, 0).await?;
    println!("DB1.DBW0 = {value}");
    client.disconnect().await
}
```

`Client` clones share a session and serialize complete operations. Use independent
clients or a `ClientPool` when reading multiple connections concurrently.

## Next steps

- [Async and blocking](async-and-blocking.md): choose an execution model.
- [Usage and examples](optional-products.md): features, CLI, tags, and optional products.
- [API reference](../api/index.md): public types and local Rustdoc generation.
- [Compatibility and limits](errors-and-limits.md): supported PLC behavior and cancellation rules.
