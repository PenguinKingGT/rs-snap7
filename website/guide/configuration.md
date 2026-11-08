# Connection and Configuration

Create settings before connecting. `ClientConfig::new(endpoint, rack, slot)`
accepts a resolved `SocketAddr`, rack 0–7, and slot 0–31.

```rust
use rs_snap7::{Client, ClientConfig};
use std::time::Duration;

let endpoint = "127.0.0.1:1102".parse().expect("valid endpoint");
let mut config = ClientConfig::new(endpoint, 0, 1)?;
config.transport.timeout = Duration::from_secs(3);
config.requested_pdu = 480;
config.validate()?;
let client = Client::connect(config).await?;
println!("Negotiated PDU: {}", client.pdu_length().await?);
```

## Settings

| Field | Default | Meaning |
| --- | --- | --- |
| `transport.timeout` | 5 seconds | Total operation deadline, including exchanges in multi-step operations |
| `requested_pdu` | 480 bytes | Requested size; the peer can negotiate a smaller PDU |
| `transport.local_tsap` | `0x0100` | Calling TSAP |
| `transport.remote_tsap` | Derived from rack/slot | Called TSAP bytes |
| `transport.tpdu_exponent` | 10 | Requested COTP frame size exponent, valid 7–13 |
| `transport.max_message_size` | 65535 bytes | Maximum reassembled transport payload |
| `recovery` | Disabled | Bounded read recovery and optional heartbeat |
| `rate_limit` | Disabled | Per-session wire request limit |

Change explicit TSAPs through `config.transport`. Use `Logo` when working with
LOGO! VM addresses. Optional gateway routing lives in `TransportConfig::routing`
and remains experimental.

## Recovery and heartbeat

```rust
config.recovery.enabled = true;
config.recovery.max_retries = 3;
config.recovery.heartbeat_interval = Some(Duration::from_secs(10));
```

Configure these fields **before** calling `Client::connect`. Recovery can replay
known read-only initial jobs after reconnecting; it does not replay mutations or
stateful continuation jobs. Heartbeat uses the same session lock as application
operations. Explicit `disconnect()` prevents background reconnection.

`is_connected()` reports local state rather than probing the PLC. For an explicit
probe use a diagnostic read such as `get_cpu_state()`.

## Request limits and events

```rust
config.rate_limit.max_per_second = 10.0;
config.operation_hook = rs_snap7::observability::OperationHook::new(|event| {
    eprintln!("{}: {:?}", event.operation, event.elapsed);
});
```

Wire limits include negotiation and continuation requests, not individual COTP
fragments. The default limiter evenly spaces requests; `RateAlgorithm` and
`RateBehavior` select token-bucket or immediate rejection behavior. Keep hooks
short. `operation_stats()` snapshots logical completion/error counters without a
network call. See [errors and limits](errors-and-limits.md) for failure handling.
