# Usage and Examples

## Cargo Features

The default client and pool use Tokio, with I/O and timers enabled. Features are additive:

| Feature | Capability |
| --- | --- |
| blocking | Synchronous client/pool/LOGO adapters over the async core |
| tag-import | CSV/JSON/TIA XML text import |
| server | Threaded emulator |
| partner | Active/passive PBC transfers |
| ppi | Experimental S7-200 8E1 serial |
| discovery | PROFINET DCP and Linux/macOS raw Ethernet adapter |
| cli | s7 binary; enables server/discovery and command dependencies |
| demo | Host-metrics sampler; enables server |

```sh
cargo build --features cli,demo --bin s7
cargo run --features cli --bin s7 -- --help
cargo run --features cli --bin s7 -- server --bind 127.0.0.1:1102
cargo run --features cli --bin s7 -- write --port 1102 DB1:0:INT -42
cargo run --features cli --bin s7 -- read --port 1102 DB1:0:INT
cargo run --features cli --bin s7 -- write --port 1102 'DB1:4:REAL[2]' '[1.5,2.5]'
cargo run --features cli --bin s7 -- dump --port 1102 --db 1 --length 64
cargo run --features cli --bin s7 -- info --port 1102
cargo run --features cli,demo --bin s7 -- demo --bind 127.0.0.1:1102
cargo run --features cli --bin s7 -- discover --interface 192.168.1.100
```

Use an IP assigned to the local Ethernet interface for discovery; the built-in
raw adapter needs OS raw-socket/BPF permissions. It is separate from TCP access.
Server/demo accept `--duration-ms 1000` for bounded runs; otherwise use Ctrl-C.
CLI writes accept scalar text or JSON arrays; quoted shell arguments preserve
brackets, spaces and negative values. Calendar writes use the Rust library.
`dump --output db.bin` saves raw bytes instead of printing hex.

## Library Examples

```sh
cargo run --example emulator --features server
cargo run --example read_db -- 127.0.0.1:1102 DB1:0:INT
cargo run --example async_read -- 127.0.0.1:1102
cargo run --example blocking_read --features blocking -- 127.0.0.1:1102 DB1:0:INT
```

Tag imports accept document text, keeping filesystem policy in the application:

```rust,no_run
# #[cfg(feature = "tag-import")] fn main() -> rs_snap7::Result<()> {
let text = std::fs::read_to_string("tags.csv")?;
let tags = rs_snap7::tags::import::load_csv(&text)?;
assert!(tags.contains_key("speed"));
# Ok(()) }
# #[cfg(not(feature = "tag-import"))] fn main() {}
```

For PPI, `PpiClient::open(path, 9600, PpiConfig::default())` opens an owned 8E1
adapter. Dropping the client closes its port. For Partner, bind a
`PartnerListener` with `transport.local_tsap` equal to the active caller's
`transport.remote_tsap`; each connected `Partner` dispatches bidirectional pushes.

## Demo DB Layout

DB1 contains REALs at 0/4/8/12/16/20/24/28: CPU percent, memory percent,
disk read/write MB/s, network RX/TX MB/s, temperature Celsius and fan RPM.
DINT at 32 is host uptime seconds. Byte 36 bits 0/1/2 indicate overheating,
high CPU load and busy disk. Byte 37 bit 0 indicates temperature availability.
DB1 is read-only remotely; missing temperature/fan values are zero.
DB2 is 64 bytes of writable control/demo storage.

## Runtime and Data Helpers

Use `rs_snap7::{Client, ClientPool, Logo}` with awaited network methods. Async
`Db` operations accept `&Client` and borrow the owned snapshot across the await.
Enable `blocking` for `rs_snap7::blocking::{Client, ClientPool, Logo}` and
`Db::{read_row_blocking, write_row_blocking, read_fields_blocking, write_fields_blocking}`.
These adapters preserve partial dirty-range progress in the original snapshot.
The blocking runtime accepts borrowed futures and does not require scalar types
to be `Send + 'static`.

Blocking network calls require an ordinary thread outside any entered Tokio
context, including `spawn_blocking`; misuse returns `BlockingInAsyncContext`.
Use the default async API inside Tokio applications. Blocking clients share one
process-lifetime runtime worker. Foreground hooks run on the caller after locks
and runtime exit; background hooks use a bounded dispatcher, dropping incoming
events on overflow and reporting per-client losses in operation statistics.

Advanced raw COTP consumers use async `transport::Connection`.
`transport::blocking::Connection` is available with `blocking`, `server`, or
`partner` for explicit synchronous transport access. Server, Partner, serial PPI,
and DCP retain their existing execution models. The CLI uses async client I/O and
offloads service loops, host sampling, and discovery to blocking workers; `cli`
does not enable `blocking`.
