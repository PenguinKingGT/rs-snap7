# python-snap7 Migration Tracker

Baseline: local `python-snap7` version **3.4.0**, commit
`da93f21a06ee5565f7793323ddbdb542f31ec9a2`.
Scanned on 2026-11-08: `snap7/`, `s7/`, `tests/`, `doc/`, and `example/`.
The local upstream checkout is excluded from Git; reference paths below are
relative to `python-snap7/` and require that checkout.

## Progress

The Rust library scaffold is complete. **28 of 28 migration features are Done.**
All baseline feature implementations and their software tests are delivered. Real PLC/PPI/DCP hardware acceptance and remote CI runs are not claimed; see the validation guide.

Use `Todo`, `In progress`, `Done`, or `Deferred` in the tables below. Update the
status and summary in the same PR as implementation changes. Mark a feature
`Done` only after its listed scope, tests, and API documentation are complete;
include Rust source/test links or a PR reference. Record remaining work for
`In progress` and a reason for `Deferred`. Split rows if partial progress needs
independent tracking. Deferred features remain part of the total.

## 1. Core Communication

Start here; later groups build on these capabilities.

| ID | Feature to implement | Upstream reference | Status |
| --- | --- | --- | --- |
| F01 | Memory areas, word lengths, parameters, result types, and structured errors with protocol codes | `type.py`, `error.py`, `datatypes.py` in `snap7/` | Done — [types](../src/types.rs), [errors](../src/error.rs), [tests](../tests/types.rs) |
| F02 | Byte codecs: bits, signed/unsigned integers, floats, characters, STRING/WSTRING, bounds and byte order | `snap7/util/getters.py`, `setters.py`, `datatypes.py` | Done — [codecs](../src/codec/mod.rs), [tests](../tests/codec.rs) |
| F03 | Date/time codecs: S5TIME, TIME, DATE, TOD, DT, DTL, and long variants, preserving upstream read/write limits | `snap7/util/getters.py`, `setters.py` | Done — [date/time codecs](../src/codec/datetime.rs), [tests](../tests/datetime.rs) |
| F04 | TCP/TPKT/COTP transport: connection handshake, TSAPs, timeouts, framing, fragmentation, and disconnect | `snap7/connection.py` | Done — [transport](../src/transport.rs), [tests](../tests/transport.rs) |
| F05 | S7 PDU encoding/decoding, negotiation, request references, stale/lost response detection, and malformed-packet rejection | `snap7/s7protocol.py` | Done — [protocol](../src/protocol.rs), [tests](../tests/protocol.rs) |
| F06 | Synchronous client lifecycle, connection parameters, rack/slot, routed connections, and raw ISO exchange | `snap7/client.py`, `client_base.py` | Done — [client](../src/client.rs), [session](../src/session.rs), [tests](../tests/client.rs) |
| F07 | Raw DB/area reads and writes, DB get/fill, I/O/marker/timer/counter helpers, and large-transfer chunking | `snap7/client.py` | Done — [operations](../src/operations.rs), [client](../src/client.rs), [tests](../tests/memory.rs) |
| F08 | Typed scalar and array DB reads/writes, including bit-preserving updates | `snap7/client.py`, `async_client.py` | Done — [typed operations](../src/client/typed.rs), [tests](../tests/memory.rs); blocking and async exposure implemented in F12 |
| F09 | Multi-variable reads/writes, item limits, PDU splitting, and per-item errors | `snap7/client.py`, `s7protocol.py` | Done — [batch operations](../src/client/batch.rs), [packet planner](../src/planner.rs), [tests](../tests/memory.rs) |

## 2. Client Capabilities & Data Tools

| ID | Feature to implement | Upstream reference | Status |
| --- | --- | --- | --- |
| F10 | Read planning: sorting, merging, packetization, result extraction, plan caching, and parallel dispatch | `snap7/optimizer.py`, `client.py` | Done — [read planner](../src/planner.rs), [blocking pool](../src/pool.rs), [async pool](../src/async_client/pool.rs), [tests](../tests/pool.rs) |
| F11 | Connection recovery: bounded retries, exponential backoff/jitter, heartbeat, and disconnect/reconnect hooks | `snap7/client.py` | Done — [recovery policies](../src/policy.rs), session/heartbeat adapters, [failure and lifecycle tests](../tests/recovery.rs) |
| F12 | Native asynchronous client with supported operation parity, serialized/concurrent request handling, and cleanup | `snap7/async_client.py` | Done — [native async client](../src/async_client/mod.rs), [cancellation tests](../tests/async_client.rs), sync/async parity scenarios in `tests/` |
| F13 | Request limiting: fixed spacing/token bucket, bursts, blocking/rejection, sync/async integration | `snap7/rate_limiter.py` | Done — [policies](../src/policy.rs), [wire tests](../tests/rate_limit.rs); fake-time algorithm tests colocated with implementation |
| F14 | Contextual/structured logging, operation timing/hooks, execution time, and last-error reporting | `snap7/log.py`, `client.py`, `client_base.py` | Done — [events/statistics](../src/observability.rs), [hook tests](../tests/memory.rs); callback/JSON unit tests colocated |
| F15 | CPU/CP/order/protection information, CPU state, SZL lists, fragmented SZL reads, and diagnostic buffer | `snap7/szl.py`, `client.py`, `s7protocol.py` | Done — [SZL models](../src/szl.rs), [diagnostics](../src/client/diagnostics.rs), [tests](../tests/diagnostics.rs) |
| F16 | PLC stop/hot/cold start, clock read/write, compression, and RAM-to-ROM copy | `snap7/client.py`, `s7protocol.py` | Done — [management](../src/client/management.rs), [wire codecs](../src/protocol/management.rs), [tests](../tests/management.rs) |
| F17 | Block listing/metadata, offline block inspection, upload/full upload, download, and deletion | `snap7/client.py`, `client_base.py`, `s7protocol.py` | Done — [block models/state machines](../src/blocks/mod.rs), [wire services](../src/protocol/blocks.rs), [blocking](../src/client/blocks.rs)/[async](../src/async_client/blocks.rs) adapters, [tests](../tests/blocks.rs) |
| F18 | Session password encoding, setting/clearing, and authentication errors | `snap7/client.py`, `s7protocol.py` | Done — [security](../src/client/security.rs), [tests](../tests/security.rs) |
| F19 | I/O force-bit operations, cancellation, and force-table reads | `snap7/client.py`, `async_client.py` | Done — [I/O operations](../src/client/force.rs), [table parser](../src/szl.rs), [tests](../tests/memory.rs), [table tests](../tests/diagnostics.rs) |
| F20 | Typed tags: PLC4X/STEP7 and nodeS7 syntax, strict parsing, read/write/batch reads, CSV/JSON/TIA XML imports, and browse-result conversion | `snap7/tags.py`, `client.py` | Done — [tags](../src/tags/mod.rs), [imports](../src/tags/import.rs), [tests](../tests/tags.rs) and sync/async I/O in memory tests |
| F21 | DB layout specifications, row access, change tracking, export, and selective reads/writes | `snap7/util/db.py` | Done — [DB tools](../src/db.rs), [layout tests](../tests/db.rs), sync/async row/field scenarios in memory tests |

## 3. Additional Products & Validation

| ID | Feature to implement | Upstream reference | Status |
| --- | --- | --- | --- |
| F22 | S7 server/emulator: memory registration/locking, client sessions, protocol services, CPU state, events, callbacks, and configuration | `snap7/server/` | Done — [server](../src/server/mod.rs), isolated protocol engine, [integration tests](../tests/server.rs); emulation limits documented in compatibility guide |
| F23 | LOGO! client: address parsing, VM access, connection options, and recovery | `snap7/logo.py` | Done — [LOGO adapter](../src/logo.rs), [syntax tests](../tests/logo.rs), [I/O tests](../tests/memory.rs), [TSAP recovery tests](../tests/recovery.rs) |
| F24 | Active/passive Partner communication: framed transfers, sync/async send/receive, callbacks, status, and statistics | `snap7/partner.py` | Done — [Partner dispatcher](../src/partner/mod.rs), [wire codec](../src/partner/wire.rs), [tests](../tests/partner.rs); async operations use Tokio blocking pool |
| F25 | Experimental S7-200 PPI: serial frames/checksums, negotiation, retries, area/V-memory reads and writes | `snap7/ppi.py` | Done — [serial client/transport](../src/ppi/mod.rs), [frame codec](../src/ppi/frame.rs), [simulated serial tests](../tests/ppi.rs); physical S7-200 acceptance pending separately |
| F26 | PROFINET DCP discovery and identification with device metadata | `snap7/discovery.py` | Done — [DCP codecs/collection](../src/discovery/mod.rs), [raw adapter](../src/discovery/raw.rs), [vector/mock tests](../tests/discovery.rs); OS and live-network limits documented |
| F27 | CLI read/write/dump/info/server/discover commands, host-metrics demo, and runnable usage examples | `snap7/cli.py`, `demo.py`, `example/` | Done — [CLI](../src/bin/s7.rs), [metrics demo](../src/demo.rs), [examples](../examples/), [process tests](../tests/cli.rs), [usage](usage.md) |
| F28 | Shared validation suite: wire vectors, Python behavioral compatibility, property/malformed-input tests, concurrency/stress tests, and opt-in hardware acceptance | `tests/test_conformance.py`, `test_behavioral_compatibility.py`, `test_hypothesis.py`, `test_stress.py`, `tests/real_plc/` | Done — [properties](../tests/properties.rs), [stress](../tests/stress.rs), [bidirectional Python peers](../tests/python_compat.rs), [opt-in hardware harness](../tests/hardware.rs), [validation/limits](testing.md); physical acceptance not executed |

## Completion Rules & Scope Limits

- Each feature needs normal, boundary, and error-path tests; use the matching
  upstream `tests/test_*.py` as a checklist. F28 supplements these per-feature tests.
- Default tests must work without PLC hardware. Track hardware validation
  separately with CPU/firmware and results; local emulation does not prove it.
- Preserve wire behavior with idiomatic Rust APIs. Python package aliases,
  `ctypes` layouts, ignored legacy arguments, and `as_*` compatibility wrappers
  need an explicit mapping decision rather than automatic duplication. Some
  upstream `as_*` methods are unsupported stubs, distinct from `AsyncClient`.
- Classic S7 scope excludes S7CommPlus, symbolic PLC access, optimized DB access,
  and full project backups. Tag metadata parsing does not provide symbolic access.
- PPI remains single-master/single-slave; multimaster and PPI over TCP are outside
  this baseline. DCP discovery is a separate protocol, not S7 broadcast discovery.
