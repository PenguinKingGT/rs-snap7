# rs-snap7

English | [简体中文](README.zh-CN.md)

An incremental Rust port of `python-snap7` for Siemens S7 PLC communication.

See the [migration tracker](docs/migration.md) for planned features and progress.
The [architecture design](docs/architecture.md) describes module responsibilities
and implementation rules.

The blocking client supports classic S7 connections, raw and typed memory
operations, DB helpers, and multi-variable reads/writes. Hardware validation is
still pending; consult the tracker for remaining capabilities. The package is named `rs-snap7`, its Rust crate name is
`rs_snap7`, and it uses the Rust 2024 edition. The project uses the MIT license; see `LICENSE` and `NOTICE` for upstream
attribution. Publishing is currently disabled.

## Project Structure

- `src/lib.rs`: Rust library entry point.
- `python-snap7/`: local Python reference implementation, documentation, and tests;
  excluded from this repository's Git tracking.
- `AGENTS.md`: contributor and code quality guidelines.

Introduce modules by responsibility as features are developed. Place integration
tests in `tests/` and runnable examples in `examples/`.

## Local Development

Install a toolchain supporting the Rust 2024 edition, including the `rustfmt` and
`clippy` components. Run these commands from the repository root:

```sh
cargo build
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo test --doc
```

Add normal, boundary, and error-path tests with each feature. Default tests must run without PLC
hardware; bug fixes must include regression tests. Document public interfaces and
explain complex logic, following `AGENTS.md`.

Write comments and `AGENTS.md` in English. Keep the default README in English and
update the Chinese translation alongside it.

## Blocking Client Example

```rust,no_run
use rs_snap7::{Client, ClientConfig};

fn main() -> rs_snap7::Result<()> {
    let endpoint = "192.168.1.10:102".parse().expect("valid PLC endpoint");
    let client = Client::connect(ClientConfig::new(endpoint, 0, 1)?)?;
    let value = client.db_read_int(1, 0)?;
    println!("DB1.DBW0 = {value}");
    client.disconnect()
}
```

See [compatibility notes](docs/compatibility.md) for deliberate interface changes.
