# Async and Blocking

## Default async API

Use `rs_snap7::{Client, ClientPool, Logo}` in Tokio applications. Enable I/O and
timers in your runtime. Network operations are awaited; parsing and value
conversion remain ordinary synchronous functions.

```rust
let client = rs_snap7::Client::connect(config).await?;
let ready = client.is_connected().await;
let bytes = client.db_read(1, 0, 16).await?;
client.disconnect().await?;
```

Each connection permits one complete operation at a time. Independent sessions
can run concurrently. Dropping the final handle releases its socket and heartbeat
task. Cancelling an in-flight exchange invalidates the socket so a partial reply
cannot be mistaken for the next response.

::: warning Unknown mutation outcomes
A write or control request may have reached the PLC before its acknowledgement
was lost or the future was cancelled. Inspect `AmbiguousWrite` and
`AmbiguousOperation`; do not silently replay a mutation.
:::

## Optional blocking API

Enable the feature in your dependency declaration:

```toml
rs-snap7 = { version = "0.1.0", features = ["blocking"] }
```

Use the same configuration and result types with `rs_snap7::blocking`:

```rust
use rs_snap7::{blocking::Client, ClientConfig, Result};

fn main() -> Result<()> {
    let endpoint = "127.0.0.1:1102".parse().expect("valid endpoint");
    let client = Client::connect(ClientConfig::new(endpoint, 0, 1)?)?;
    let value = client.db_read_int(1, 0)?;
    println!("{value}");
    assert!(client.is_connected()?);
    client.disconnect()
}
```

All blocking clients share one lazy background Tokio worker. Heartbeats keep
running between calls. The runtime remains until process exit; each client's
socket and tasks are still released on final drop. Generic scalar results do not
need `Send + 'static` merely to use the blocking adapter.

::: tip Call from an ordinary thread
Blocking network methods reject any entered Tokio context with
`Error::BlockingInAsyncContext`, including `spawn_blocking` and manually entered
handles. Use the async API inside Tokio applications.
:::

## DB snapshots and callbacks

| Operation | Async | Blocking |
| --- | --- | --- |
| Client readiness | `is_connected().await` → `bool` | `is_connected()?` → `bool` |
| DB row | `read_row`, `write_row` with `.await` | `read_row_blocking`, `write_row_blocking` |
| Selected fields | `read_fields`, `write_fields` with `.await` | `read_fields_blocking`, `write_fields_blocking` |
| Pool | `rs_snap7::ClientPool` | `rs_snap7::blocking::ClientPool` |
| LOGO! | `rs_snap7::Logo` | `rs_snap7::blocking::Logo` |

Both DB adapters mutate the same owned snapshot and retain acknowledged progress
when a later dirty range fails. Foreground blocking callbacks run on the caller
after runtime exit and session unlock, allowing metadata reentry. Background
callbacks use an ordinary dispatcher with a 1024-event queue. Incoming overflow
increments `operation_stats().background_hook_events_dropped`; keep sinks short.

Server, Partner, PPI, and DCP retain their own execution models. In particular,
Partner's `send_async` and `receive_async` use Tokio's blocking pool and have
different cancellation semantics from the native S7 client.
