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
  explicit by default; optional automatic recovery/heartbeat use `Recovery`.
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
  delegates lifecycle/operations to its standard client. Automatic recovery now
  preserves these explicit TSAPs through fresh negotiations (F11/F23).

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

- Recovery is disabled by default. Enabling it allows one replay of a known
  read-only initial job after bounded backoff/reconnect attempts. Negotiation
  resets references and cached plans, retains limiter credit, and re-plans reads
  when the replacement PDU is smaller. Stateful continuation jobs never replay.
- Mutation ACK loss remains explicit: byte writes report confirmed/uncertain
  ranges; control, clock, and password jobs use `AmbiguousOperation`. PLC refusal
  codes remain definite errors. Raw `iso_exchange` never automatically replays.
- Optional heartbeat workers send guarded CPU-status SZL probes. Explicit close
  prevents background reconnection; dropping the last clone wakes/aborts its
  worker. Loss/recovery hooks run outside session locks and isolate sink panics.
  Completed async operation hooks use boxed future scopes to bound stack growth.

- Block catalogs collect bounded USERDATA continuations before decoding complete
  entries. Remote block metadata is checked against the requested type/number;
  DB sizing reuses the same parser. Zero block-list limits send no requests.
- `upload` returns DB bytes without the first 36 compact-header bytes;
  `full_upload` returns the complete Vec, whose length replaces Python's redundant
  size tuple. The advertised complete upload length is enforced. Downloads are
  DB-only as in the baseline, accept an explicit or header-derived number, and
  reject short implicit-number input instead of silently choosing DB1.
- Upload/download/catalog state machines share one deadline, limiter accounting,
  and session across blocking/async drivers. Started failed transfers close the
  connection; unsent permit rejection preserves it. Downloads echo PLC job
  references, enforce a 1000-slice budget before offering, and never replay.
  Cancellation between exchanges also invalidates an active async transfer.
- Offline `CompactBlockInfo::decode` reports actual version-1 compact header
  fields (including load size at offset 8 and MC7 size at offset 34), retaining
  raw timestamp bytes. It deliberately replaces the Python helper's inconsistent
  field offsets and hard-coded dates rather than reproducing invented metadata.

## Typed tags

Tag types, ranges, array counts and complete addresses are validated at creation.
Strict autodetection requires a colon/comma; explicit nodeS7 parsing also accepts
bare area shortcuts. Canonical Display uses PLC4X syntax. BOOL arrays retain the
upstream one-byte stride and clients preserve neighboring bits under one guard.
Dynamic Value variants preserve integer signedness, f32/f64 widths and temporal
units. S5TIME writes remain unsupported. Arrays of strings are handled consistently.

The optional tag-import feature reads CSV, JSON and TIA XML text; callers own file
I/O. TIA leaves require explicit absolute offsets rather than assuming zero or
inferring a compiler layout. Nested members get qualified names. Numeric fractional
JSON offsets must be strings to avoid floating-point bit-address rounding.
Symbolic paths and browse metadata are retained but classic I/O rejects them.
Tag batches preserve order and per-item errors through sequential tag reads;
use read_multi_vars for optimized packet batching.

## DB layouts

DbConfig makes PLC start, row width, gap and writable prefix explicit. Layout
offsets are exact decimal strings; distinct BOOL bits may share a byte, while
overlapping fields and duplicate names are rejected. Snapshots own their buffers
and baseline; no mutable Python-style row aliases are exposed. Identifier lookup
uses current values, preventing stale indexes after edits. Selective row writes
coalesce changed writable bytes and checkpoint acknowledged ranges. A failed
range remains dirty conservatively. Field writes use tag semantics to preserve
remote BOOL neighbors; row writes replace whole bytes. Refreshing a BOOL field
refreshes its complete storage byte and discards local edits there.

## S7 emulator

The optional server accepts bounded independent COTP/S7 sessions and exposes
registered DB/I/O/marker/timer/counter memory. Memory handles provide scoped
locking; unregistering does not invalidate an in-flight cloned handle.
Downloads publish a new handle only after a complete compact DB transfer.
Upload/download/catalog services emulate registered DBs only, up to their wire
limits; unsupported program blocks and services return explicit errors.
CPU stop/start changes emulated state. Compression and ROM-copy acknowledge
without physical side effects. The clock is a deterministic stored timestamp;
it does not advance automatically. Password services acknowledge valid envelopes
without implementing PLC access protection. Default identification labels the
server as an emulator; custom SZLs can override defaults. No force services or
PLC-program execution are emulated.

Events are bounded and filtered by configured kinds; hooks run outside internal
locks and hook panics are isolated. Hooks must return promptly and must not wait
for this server to stop or join its worker. stop/drop closes all registered
sockets and joins workers. The configured timeout bounds handshake and idle
sessions. PDU limits and complete write sections are checked before mutation.

## Partner communication

Partner uses the baseline PBC BSEND USERDATA format, including R-ID, exact payload
length and echoed ACK reference. One reader dispatches both pushes and responses;
a separately serialized writer permits simultaneous sends. Transfers fit one
negotiated application PDU (payload <= PDU minus 36); COTP handles transport
fragmentation. No additional S7-level fragmentation protocol is invented.
Receive queues are bounded and overflow closes the session. Wire receive timeouts
terminate idle sessions; receive queue timeouts alone do not. Stats count
acknowledged sends and incoming transfers. Callbacks omit data, run outside locks,
and must return promptly without synchronously stopping/waiting on the dispatcher.

The async send/receive adapters run on Tokio's blocking pool. Cancelling their
future detaches a bounded operation that may still send or consume a transfer.
They do not promise the native cancellation semantics of AsyncClient. Reconnect
uses a new Partner session; unknown send outcomes are never silently replayed.

## Experimental S7-200 PPI

The optional ppi module implements single-master/single-slave 8E1 serial with
serialport (MPL-2.0 library, default libudev dependency disabled). An injected
Port must honor per-call timeouts; owned OS ports close on drop.
Frames validate delimiters, repeated lengths, stations, checksum and terminator.
SD2 request/E5 acknowledgement/SD1 poll/SD2 reply follows the pinned upstream;
polling toggles the frame-count bit on E5. Corrupt responses can be skipped
within a bounded scan. Reads/setup may retry the initial request; writes never
retransmit after possible delivery and report ambiguous outcomes. This is a
deliberate improvement over upstream's unconditional transport retries.

Negotiation starts at 240 bytes. Oversized transfers are rejected rather than
implicitly chunked. Counts are bytes for S/SM/I/Q/M/V and 16-bit elements for
AI/AQ/C/T; timer/counter offsets are item indexes. V maps to wire DB1.
Multimaster, token passing and PPI-over-TCP remain outside this experimental
baseline. No physical serial adapter or S7-200 has been validated locally.

## PROFINET DCP discovery

The discovery feature implements Identify-All and directed Identify over Ethernet,
independent of S7 TCP. Device names/family, IP/netmask/gateway and vendor/device
IDs remain optional where absent. Collection correlates transaction IDs, handles
up to two VLAN tags, skips unknown checked blocks, and deduplicates by source
MAC. Matching malformed replies fail explicitly; unrelated traffic is ignored.
Timeout, device and traffic-count limits bound each window. No DCP Set operation
is exposed.

The built-in pnet_datalink adapter (MIT OR Apache-2.0) selects an up
interface by local IPv4 on Linux/macOS and needs raw-socket/BPF permissions.
Other platforms return Unsupported and may inject FrameIo. No live PLC discovery
or privileged capture has been run locally; normal tests use captured-style
vectors and a fake adapter. Header/block layouts follow the
[Scapy protocol definitions](https://github.com/secdev/scapy/blob/master/scapy/contrib/pnio_dcp.py);
adapter timeout capabilities follow
[pnet_datalink Config](https://docs.rs/pnet_datalink/0.35.0/pnet_datalink/struct.Config.html).

## CLI and host metrics

The s7 binary uses typed tag addresses rather than duplicating Python's separate
type/offset flags. Reads/info print JSON; dump prints hex or writes a raw file.
Values are validated before opening a PLC session. CLI calendar/S5TIME writes
are explicitly unsupported; the library provides calendar codecs. Services
support Ctrl-C and an optional duration limit for automation. Raw DCP adapter
platform/permission limits also apply to discover.

The optional demo uses sysinfo (MIT) to sample CPU, memory, disk and network
rates at 250 ms intervals; temperatures are optional. Fan readings are zero
because the adapter does not expose them. Upstream DB1 offsets are preserved;
byte 37 bit 0 marks temperature availability. DB1 rejects remote memory writes,
delete and replacement downloads; application sampling retains scoped write
access. DB2 remains writable. CPU/metrics values reflect the host, not a PLC.
The stored emulator clock and other server limitations still apply.

## Download emulator difference

Cross-peer tests pin a baseline difference: Python's server requires the entire
download representation to fit an existing registered DB and stores the received
bytes verbatim, including a compact header. The Rust emulator instead validates
a complete compact DB representation and publishes its data body, so full
upload/download round trips preserve DB contents. The clients preserve the same
PLC-driven wire exchange in both cases. Python/Rust cross-peer tests cover both
behaviors without treating Python's memory storage model as physical PLC proof.

## Mutation acknowledgement failures

Control, clock-write and session-password operations classify the complete
exchange and service acknowledgement together. Lost responses, wrong service
identity, unexpected continuation or acknowledgement payloads return
AmbiguousOperation with the original cause and invalidate malformed sessions.
These operations are never automatically replayed. Explicit PLC/authentication
refusals retain their codes; rate-limit, manual-disconnect and other pre-send
rejections remain definite errors. Blocking and async clients share these rules.
