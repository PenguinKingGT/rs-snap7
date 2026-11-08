# Errors and Limits

## Handle structured errors

Library methods return `rs_snap7::Result<T>`. `Error` is non-exhaustive; keep a
fallback branch when matching it.

```rust
use rs_snap7::Error;

match client.db_write(1, 0, &[1, 2, 3]).await {
    Ok(()) => println!("acknowledged"),
    Err(Error::PartialWrite { confirmed, source }) => {
        eprintln!("{confirmed} bytes confirmed; next range failed: {source}");
    }
    Err(Error::AmbiguousWrite { confirmed, uncertain, source }) => {
        eprintln!("{confirmed} confirmed, {uncertain} uncertain: {source}");
    }
    Err(error) => eprintln!("{error}"),
}
```

| Error | Meaning and response |
| --- | --- |
| `InvalidInput` | Fix the address, length, value, or configuration |
| `Plc`, `Item`, `Authentication` | Inspect the preserved PLC code and service permissions |
| `RateLimited`, `DeadlineBeforeSend` | The request was rejected before sending |
| `Timeout`, `Io`, `Protocol`, reference errors | Inspect connection state; an exchange may have invalidated its socket |
| `PartialWrite` | Earlier bytes were acknowledged; a later range failed definitely |
| `AmbiguousWrite`, `AmbiguousOperation` | The mutation may have executed; reconcile remote state before retrying |
| `BlockingInAsyncContext` | Use the async API inside Tokio |

## Recovery and cancellation

Automatic recovery is disabled by default. Enabling it permits bounded reconnect
attempts and one replay of an eligible read-only initial request. Stateful
continuations, raw `iso_exchange`, and mutations are not automatically replayed.
A smaller PDU after reconnect causes unread work to be replanned.

Dropping a transmitted operation future closes its connection. A cancelled
mutation can still have executed on the PLC. Call `reconnect()` explicitly before
continuing after cancellation and determine the remote outcome where necessary.

## Supported scope

- Classic S7 absolute addressing; tag metadata does not enable symbolic access.
- STRING defaults to Latin-1; UTF-8 is explicit. WSTRING writes are BMP-only.
- S5TIME is readable but not writable. Calendar models have no timezone.
- DB snapshots require explicit layout offsets; they do not discover PLC layouts.
- `force_bit` changes process-image bits rather than persistent CPU force state.
- PPI, gateway routing, and some service layouts remain experimental or unverified
  against hardware. The emulator simulates protocol behavior, not PLC execution.

The project is in development and is not published to crates.io. Real PLC, serial
PPI, and live DCP acceptance remain pending. No firmware/model support is implied
by software-only tests.
