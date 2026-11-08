# Optional Features and CLI

Features are additive; async S7 access needs no optional feature.

| Feature | Capability |
| --- | --- |
| `blocking` | Synchronous client, pool, and LOGO! wrappers |
| `tag-import` | CSV/JSON/TIA XML text import |
| `server` | Threaded S7 emulator with registered memory and supported services |
| `partner` | Active/passive bidirectional PBC transfers |
| `ppi` | Experimental S7-200 serial access, 8E1 |
| `discovery` | Independent PROFINET DCP discovery |
| `cli` | `s7` binary; enables server/discovery and command dependencies |
| `demo` | Host metrics in DB1 with writable controls in DB2; requires Rust 1.95+ |

## CLI commands

Run from the repository root. Start the emulator in one terminal, then use the
other commands in another terminal.

```sh
cargo run --features cli --bin s7 -- server --bind 127.0.0.1:1102
cargo run --features cli --bin s7 -- write --port 1102 DB1:0:INT -42
cargo run --features cli --bin s7 -- read --port 1102 DB1:0:INT
cargo run --features cli --bin s7 -- dump --port 1102 --db 1 --length 16
cargo run --features cli --bin s7 -- info --port 1102
cargo run --features cli --bin s7 -- --help
```

`dump --output db.bin` saves raw bytes. Quoting array values preserves shell syntax:

```sh
cargo run --features cli --bin s7 -- write --port 1102 'DB1:4:REAL[2]' '[1.5,2.5]'
cargo run --features cli,demo --bin s7 -- demo --bind 127.0.0.1:1102 --duration-ms 1000
```

The demo publishes host metrics; the emulator does not execute PLC programs.

## LOGO!, Partner, PPI, and DCP

`Logo` accepts VM addresses `V10`, `VW20`, `VD30`, and `V10.3`. Explicit TSAPs
can be supplied when connecting. VW and VD return signed values.

Partner has independent active/passive sessions, bounded receive queues, and
R-ID correlation. Its async send/receive adapters use Tokio's blocking pool;
cancelling the wait does not stop an operation already running.

`PpiClient::open(path, 9600, PpiConfig::default())` opens an owned serial port.
PPI is experimental and has not been accepted against physical hardware.

DCP discovery is separate from TCP access. The built-in Ethernet adapter supports
Linux/macOS and requires raw-socket/BPF permissions. Select an IP assigned to the
local interface; other systems can implement `FrameIo`.

```sh
cargo run --features cli --bin s7 -- discover --interface 192.168.1.100
```
