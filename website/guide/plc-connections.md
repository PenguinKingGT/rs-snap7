# PLC Connections

rs-snap7 uses classic S7 absolute addressing over ISO-on-TCP. These are connection
prerequisites; Rust hardware acceptance is still pending.

## Choose the connection

| Controller family | Connection to investigate |
| --- | --- |
| S7-300 / S7-400 | Classic S7 Ethernet; use the configured rack and CPU slot |
| S7-1200 / S7-1500 / ET 200SP CPU | Classic PUT/GET path, where enabled by firmware and configuration |
| LOGO! Ethernet | `Logo` with project-specific TSAPs |
| S7-200 serial | Experimental `ppi` feature and `PpiClient` |

The upstream [PLC support matrix](https://python-snap7.readthedocs.io/en/latest/plc-support.html)
explains model restrictions. Its verified hardware results belong to python-snap7;
rs-snap7 does not inherit those results. A connection does not imply that control,
SZL, password, or block-transfer services are available.

## Rack, slot, and port

Typical upstream examples use rack/slot `0/2` for S7-300, `0/3` for S7-400, and
`0/1` for S7-1200/1500. Confirm the actual project layout; LOGO! and serial PPI
use different addressing. See [upstream connection guidance](https://python-snap7.readthedocs.io/en/latest/connecting.html).

```rust
let endpoint = "192.168.1.10:102".parse().expect("valid endpoint");
let config = rs_snap7::ClientConfig::new(endpoint, 0, 1)?;
let client = rs_snap7::Client::connect(config).await?;
```

The port is part of `SocketAddr`: use `:1102` for the local emulator.
Continue with [TIA Portal setup](tia-portal.md) and [configuration](configuration.md).
