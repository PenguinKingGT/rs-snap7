# python-snap7 Migration Tracker

Baseline: local `python-snap7` version **3.4.0**, commit
`da93f21a06ee5565f7793323ddbdb542f31ec9a2`.
Scanned on 2026-11-08: `snap7/`, `s7/`, `tests/`, `doc/`, and `example/`.
The local upstream checkout is excluded from Git; reference paths below are
relative to `python-snap7/` and require that checkout.

## Progress

The Rust library scaffold is complete. **0 of 28 migration features are Done.**
There are currently no Rust feature implementations or tests.

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
| F01 | Memory areas, word lengths, parameters, result types, and structured errors with protocol codes | `type.py`, `error.py`, `datatypes.py` in `snap7/` | Todo |
| F02 | Byte codecs: bits, signed/unsigned integers, floats, characters, STRING/WSTRING, bounds and byte order | `snap7/util/getters.py`, `setters.py`, `datatypes.py` | Todo |
| F03 | Date/time codecs: S5TIME, TIME, DATE, TOD, DT, DTL, and long variants, preserving upstream read/write limits | `snap7/util/getters.py`, `setters.py` | Todo |
| F04 | TCP/TPKT/COTP transport: connection handshake, TSAPs, timeouts, framing, fragmentation, and disconnect | `snap7/connection.py` | Todo |
| F05 | S7 PDU encoding/decoding, negotiation, request references, stale/lost response detection, and malformed-packet rejection | `snap7/s7protocol.py` | Todo |
| F06 | Synchronous client lifecycle, connection parameters, rack/slot, routed connections, and raw ISO exchange | `snap7/client.py`, `client_base.py` | Todo |
| F07 | Raw DB/area reads and writes, DB get/fill, I/O/marker/timer/counter helpers, and large-transfer chunking | `snap7/client.py` | Todo |
| F08 | Typed scalar and array DB reads/writes, including bit-preserving updates | `snap7/client.py`, `async_client.py` | Todo |
| F09 | Multi-variable reads/writes, item limits, PDU splitting, and per-item errors | `snap7/client.py`, `s7protocol.py` | Todo |

## 2. Client Capabilities & Data Tools

| ID | Feature to implement | Upstream reference | Status |
| --- | --- | --- | --- |
| F10 | Read planning: sorting, merging, packetization, result extraction, plan caching, and parallel dispatch | `snap7/optimizer.py`, `client.py` | Todo |
| F11 | Connection recovery: bounded retries, exponential backoff/jitter, heartbeat, and disconnect/reconnect hooks | `snap7/client.py` | Todo |
| F12 | Native asynchronous client with supported operation parity, serialized/concurrent request handling, and cleanup | `snap7/async_client.py` | Todo |
| F13 | Request limiting: fixed spacing/token bucket, bursts, blocking/rejection, sync/async integration | `snap7/rate_limiter.py` | Todo |
| F14 | Contextual/structured logging, operation timing/hooks, execution time, and last-error reporting | `snap7/log.py`, `client.py`, `client_base.py` | Todo |
| F15 | CPU/CP/order/protection information, CPU state, SZL lists, fragmented SZL reads, and diagnostic buffer | `snap7/szl.py`, `client.py`, `s7protocol.py` | Todo |
| F16 | PLC stop/hot/cold start, clock read/write, compression, and RAM-to-ROM copy | `snap7/client.py`, `s7protocol.py` | Todo |
| F17 | Block listing/metadata, offline block inspection, upload/full upload, download, and deletion | `snap7/client.py`, `client_base.py`, `s7protocol.py` | Todo |
| F18 | Session password encoding, setting/clearing, and authentication errors | `snap7/client.py`, `s7protocol.py` | Todo |
| F19 | I/O force-bit operations, cancellation, and force-table reads | `snap7/client.py`, `async_client.py` | Todo |
| F20 | Typed tags: PLC4X/STEP7 and nodeS7 syntax, strict parsing, read/write/batch reads, CSV/JSON/TIA XML imports, and browse-result conversion | `snap7/tags.py`, `client.py` | Todo |
| F21 | DB layout specifications, row access, change tracking, export, and selective reads/writes | `snap7/util/db.py` | Todo |

## 3. Additional Products & Validation

| ID | Feature to implement | Upstream reference | Status |
| --- | --- | --- | --- |
| F22 | S7 server/emulator: memory registration/locking, client sessions, protocol services, CPU state, events, callbacks, and configuration | `snap7/server/` | Todo |
| F23 | LOGO! client: address parsing, VM access, connection options, and recovery | `snap7/logo.py` | Todo |
| F24 | Active/passive Partner communication: framed transfers, sync/async send/receive, callbacks, status, and statistics | `snap7/partner.py` | Todo |
| F25 | Experimental S7-200 PPI: serial frames/checksums, negotiation, retries, area/V-memory reads and writes | `snap7/ppi.py` | Todo |
| F26 | PROFINET DCP discovery and identification with device metadata | `snap7/discovery.py` | Todo |
| F27 | CLI read/write/dump/info/server/discover commands, host-metrics demo, and runnable usage examples | `snap7/cli.py`, `demo.py`, `example/` | Todo |
| F28 | Shared validation suite: wire vectors, Python behavioral compatibility, property/malformed-input tests, concurrency/stress tests, and opt-in hardware acceptance | `tests/test_conformance.py`, `test_behavioral_compatibility.py`, `test_hypothesis.py`, `test_stress.py`, `tests/real_plc/` | Todo |

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
