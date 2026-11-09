# Connection Troubleshooting

Start with the [local emulator](server.md) to separate application/API mistakes
from network and CPU configuration. The upstream [connection troubleshooting](https://python-snap7.readthedocs.io/en/latest/connection-issues.html)
organizes similar checks; Rust error and retry behavior are described below.

## Diagnose by stage

| Symptom | Check next |
| --- | --- |
| `InvalidInput` before connecting | Resolved endpoint, rack/slot ranges, timeout and PDU settings |
| `Io` / connection refused | Address, TCP port, listener and network path; DCP discovery is separate |
| `Timeout` during negotiation | CPU endpoint, TSAP/rack/slot and configured deadline |
| Connect succeeds, DB access fails | DB number, exact byte range, PUT/GET permissions and standard DB access |
| DB access works, advanced service fails | Target CPU/firmware service support; retain the PLC error code |
| `BlockingInAsyncContext` | Use async `Client` inside Tokio, including in entered `spawn_blocking` contexts |
| Unexpected values | Signedness, data width, byte offset, byte order and BOOL bit index |

`is_connected()` reports local readiness. A remote peer may have disconnected
since the previous operation; a successful state query is not a liveness probe.

## Recover deliberately

Configure bounded read recovery and heartbeat before connecting; see
[configuration](configuration.md). Known read-only initial jobs can be replayed
once after recovery. Mutations and stateful continuations are not replayed.

After cancellation, explicitly call `reconnect()` before further access.
For `AmbiguousWrite` or `AmbiguousOperation`, first reconcile remote state.
Increasing retries cannot resolve an unknown mutation outcome.

## Shared clients and observation

Clones share one serialized session; independent clients enable concurrent I/O.
Batch/pool results retain per-item errors. Polling faster than the PLC can serve
requests increases queueing; configure `rate_limit` and inspect `operation_stats()`.

Callbacks execute outside session locks. Keep them short, and check
`hook_panics` or `background_hook_events_dropped` when observing blocking clients.
See [errors and limits](errors-and-limits.md) and [API errors](../api/errors.md).
