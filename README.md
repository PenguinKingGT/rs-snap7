# rs-snap7

[![crates.io](https://img.shields.io/crates/v/rs-snap7.svg)](https://crates.io/crates/rs-snap7)
[![API documentation](https://docs.rs/rs-snap7/badge.svg)](https://docs.rs/rs-snap7)
[![Rust 1.95+](https://img.shields.io/badge/rust-1.95%2B-orange.svg)](Cargo.toml)
[![MIT license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

English | [简体中文](README.zh-CN.md)

An async-first Rust library for classic Siemens S7 communication, ported from
[python-snap7](https://github.com/gijzelaerr/python-snap7). Use native Tokio I/O
by default, or enable a blocking facade over the same execution core.

[User guide](website/guide/getting-started.md) · [API reference](https://docs.rs/rs-snap7) · [Examples](examples) · [Changelog](CHANGELOG.md)

## Install

Requires **Rust 1.95+**. Add the library and a Tokio runtime to your application:

```toml
[dependencies]
rs-snap7 = "0.1.0"
tokio = { version = "1", features = ["macros", "rt", "net", "time"] }
```

The Cargo package is `rs-snap7`; Rust imports use `rs_snap7`.

## Quick start

This example reads a signed 16-bit integer from DB1 at byte offset 0. It targets
the local emulator below; use your controller's endpoint, rack, and slot for hardware.

```rust
use rs_snap7::{Client, ClientConfig, Result};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let endpoint = "127.0.0.1:1102".parse().expect("valid endpoint");
    let client = Client::connect(ClientConfig::new(endpoint, 0, 1)?).await?;
    let value = client.db_read_int(1, 0).await?;
    println!("DB1.DBW0 = {value}");
    client.disconnect().await
}
```

### Try without a PLC

Install the optional CLI and start an emulator with DB1:

```sh
cargo install rs-snap7 --version 0.1.0 --features cli --locked
s7 server --bind 127.0.0.1:1102
```

Leave it running. In another terminal, write and read a value:

```sh
s7 write --port 1102 DB1:0:INT -42
s7 read --port 1102 DB1:0:INT
```

The read command and Rust example print `-42`. Stop the emulator with Ctrl-C.
See the [server tutorial](website/guide/server.md) for embedding it in an application.

## Capabilities

- Raw DB/area access, typed scalars and arrays, strings, and bit-preserving updates.
- Multi-variable reads/writes, read planning, and pools of independent connections.
- PLC diagnostics, SZLs, CPU/clock services, block transfers, and session passwords.
- PLC4X/nodeS7 tags, DB layouts, owned snapshots, and selective dirty-range writes.
- Configurable timeouts, bounded read recovery, heartbeat, request limits, and hooks.
- Optional emulator, LOGO!, Partner/PBC, serial PPI, DCP discovery, and CLI/demo tools.

## Blocking access

For synchronous applications, enable `blocking`:

```toml
[dependencies]
rs-snap7 = { version = "0.1.0", features = ["blocking"] }
```

```rust
use rs_snap7::{blocking::Client, ClientConfig, Result};

fn main() -> Result<()> {
    let endpoint = "127.0.0.1:1102".parse().expect("valid endpoint");
    let client = Client::connect(ClientConfig::new(endpoint, 0, 1)?)?;
    println!("DB1.DBW0 = {}", client.db_read_int(1, 0)?);
    client.disconnect()
}
```

Blocking clients share one background runtime. Call them from ordinary threads
outside any entered Tokio runtime; nested use, including `spawn_blocking`, returns
`Error::BlockingInAsyncContext`. Use the async API inside Tokio applications.
See [async and blocking](website/guide/async-and-blocking.md) for lifecycle,
callback, pool, and DB-helper behavior.

## Cargo features

Async S7 access, codecs, tags, DB tools, and async LOGO! require no optional feature.
Features are additive:

| Feature | Adds |
| --- | --- |
| `blocking` | Synchronous client, read pool, and LOGO! adapters |
| `tag-import` | CSV, JSON, and TIA XML metadata imports |
| `server` | Local S7 emulator with registered memory and service simulation |
| `partner` | Active/passive Partner/PBC sessions; async adapters use the blocking pool |
| `ppi` | Experimental S7-200 single-master serial access (8E1) |
| `discovery` | PROFINET DCP; built-in raw Ethernet on Linux/macOS |
| `cli` | `s7` read/write/dump/info/server/discover commands; enables server/discovery |
| `demo` | Host metrics in read-only DB1 and writable controls in DB2 |

## Scope and status

`0.1.0` is the first development release; APIs may change in later `0.x` minor
versions. Classic S7 absolute addressing is supported; S7CommPlus and symbolic PLC
I/O are outside the current scope. The emulator simulates services and does not
execute PLC programs.

Physical PLC, serial PPI, and live DCP acceptance remain pending. Individual CPU
services depend on model, firmware, and access settings. Start with the
[PLC connection guide](website/guide/plc-connections.md) and
[TIA Portal setup](website/guide/tia-portal.md).

Clones share a serialized session; use independent connections for parallel I/O.
Cancelling a transmitted exchange closes its socket. A write/control operation
can have an uncertain remote outcome, so inspect structured errors before retrying.
See [compatibility notes](docs/compatibility.md) and
[troubleshooting](website/guide/troubleshooting.md).

## Contributing

Clone the repository and run:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --doc --all-features
```

Default software tests use local peers and need no PLC. Hardware/Python acceptance
is explicitly opt-in. Follow [AGENTS.md](AGENTS.md); see the
[validation guide](docs/testing.md), [migration tracker](docs/migration.md), and
[architecture](docs/architecture.md) for contributor details.
The local `python-snap7/` reference is excluded from Git. The standalone
[bilingual documentation site](website/README.md) has its own Node dependencies.

## License

MIT. See [LICENSE](LICENSE) and [NOTICE](NOTICE) for upstream attribution.
Release scope is recorded in [CHANGELOG.md](CHANGELOG.md); maintainer publishing
instructions are in [RELEASING.md](RELEASING.md).
