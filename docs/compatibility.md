# Compatibility Notes

The wire baseline is recorded in [migration.md](migration.md). Rust interfaces
preserve supported representations without duplicating Python-specific layouts.

- `Area`, `WordLen`, `Parameter`, `BlockType`, and `ServerArea` retain upstream
  numeric identifiers; Rust names follow Rust naming conventions. Address
  constructors validate the entire 24-bit range and reject inconsistent areas.
- Scalar codecs use checked slices and explicit Rust numeric types. String
  lengths count encoded bytes. Latin-1 is the default; UTF-8 is explicitly
  supported. Arbitrary Python codec names are not accepted. WSTRING writes
  remain BMP-only. Invalid input is rejected before writes modify buffers.
- TIME/TO(D) and long duration functions return typed integer units, not formatted
  Python strings or timedelta. `Date` and `DateTime` are timezone-free calendar
  models. Long types retain nanoseconds that Python truncates to microseconds.
- DT writes truncate to milliseconds as upstream does. S5TIME is read-only.
  BCD, calendar, weekday ranges, and time-of-day fields are validated strictly;
  malformed values tolerated by permissive Python getters are rejected.

These differences are intentional; per-feature tests document valid wire values
and failure behavior. No PLC hardware has been validated yet.

- Transport validates COTP Class 0 headers and negotiation TLVs more strictly
  than the Python implementation. Fragmentation is bounded by the negotiated
  TPDU size; malformed frames and timeouts invalidate the connection.

- `ClientConfig` replaces numeric parameter getters/setters and ignored legacy
  constructor arguments. Settings are applied before connecting; routing and
  explicit variable-length TSAPs live in `TransportConfig`. `reconnect()` is
  explicit; automatic recovery and heartbeat are not implemented yet.
- `Client` clones serialize all operations on one shared session, improving on
  upstream blocking-client thread safety. `is_connected()` reports local state,
  not remote liveness. Raw S7 jobs receive a session-owned request reference.

- `db_get`/`db_fill` accept `Option<usize>`: `None` uses block metadata; `Some`
  supplies a known size. Timer/counter offsets and counts are element indexes,
  but payloads contain bytes. Definite partial writes and unacknowledged writes
  use different errors, preserving confirmed progress without replay.

- SZL reads validate ID/index and bound continuations to 100 follow-ups, 1 MiB,
  and one operation deadline. Truncated identification layouts/record tables
  return errors rather than zero-filled success models. CP/protection layouts
  follow the pinned Python implementation and are not hardware-validated.
- CPU status uses SZL 0x0424 and its status byte at body offset 7, rather than
  the Python baseline's unconditional RUN result. The wire interpretation was
  checked against the native [Snap7 status operation](https://github.com/SCADACS/snap7/blob/master/src/core/s7_micro_client.cpp).
  No native implementation code is incorporated or linked.

- PLC control validates the acknowledged function. Clock reads accept native
  ten-byte and legacy eight-byte layouts; writes use the upstream native form.
  `set_plc_system_datetime()` sends UTC explicitly rather than Python host-local
  time. Applications should pass a `DateTime` for the PLC's local convention.
  Control and password changes are never automatically replayed.

- `force_bit` and `cancel_force` modify process-image bits, matching upstream;
  they do not establish persistent CPU forces. Force-table reads propagate
  communication errors and only treat explicit unsupported-service responses
  as an empty list. Invalid table records are rejected rather than truncated.

- LOGO! VM parsing consumes the entire address instead of accepting regex-prefix
  matches. VW/VD retain signed values. The adapter preserves explicit TSAPs and
  delegates lifecycle/operations to its standard client; automatic recovery is
  still tracked under F11/F23.

- Native `AsyncClient` requires the optional `async` feature and a Tokio runtime
  with I/O/time enabled. Supported blocking operations have asynchronous
  counterparts, sharing codecs and packet planning. Dropping an in-flight
  transaction future closes its socket; cancellation during a write has an
  unknown remote outcome and must not be treated as a confirmed failure.

- Request limits count S7 negotiation, continuation, and application jobs per
  session; COTP frames are not independently charged. Fixed and token-bucket
  policies share accounting between runtimes. Token bucket burst defaults to
  one request unless explicitly configured. A deadline reached while waiting
  returns `DeadlineBeforeSend`, so an unsent write is not reported as ambiguous.

- Multi-read optimization merges only overlapping/adjacent byte ranges, without
  speculative gaps. Rejected merged reads fall back to the original items so
  valid neighbors retain independent results. Plans are cached per layout/PDU.
  Parallel reads use explicitly independent sessions in bounded pools; clones
  of one session cannot be added as separate workers.

- `OperationHook` receives one redacted event per completed logical operation,
  including nested typed conveniences. `operation_stats()` exposes last elapsed
  time, error kind/code, and counters without taking the network lock. Hooks run
  after session guards are released; hook panics are isolated and counted.
  JSON records are opt-in application sinks; the library sets up no global logger.
  Cancelled async futures have no completion event because no result was returned.
