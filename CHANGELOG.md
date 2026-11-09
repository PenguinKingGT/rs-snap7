# Changelog

## 0.1.0 — 2026-11-09

Initial development release of the Rust port of python-snap7 3.4.0.

### Included

- Native Tokio S7 client, validated connection configuration, shared sessions,
  bounded recovery/heartbeat, request limits, and operation hooks/statistics.
- Optional `blocking` client, pool, and LOGO! facades over the async core.
- Checked TCP/TPKT/COTP and S7 codecs; raw and typed memory operations,
  multi-variable batches, independent read pools, and bit-preserving updates.
- Diagnostic/SZL models, CPU/clock services, block catalog/upload/download/delete,
  session passwords, and process-image bit operations.
- Typed tags and optional CSV/JSON/TIA XML import, DB snapshots/layouts, dirty
  tracking, and selective row/field I/O.
- Optional S7 emulator, Partner/PBC sessions, experimental serial PPI, PROFINET
  DCP discovery, CLI, and host-metrics demo.
- Software regressions for smaller-PDU recovery, malformed writes, DINTEGER
  responses, uncertain mutation acknowledgements, and exact response budgets.
- English/Chinese documentation and a standalone VitePress user manual.

### Scope and requirements

- Requires Rust 1.95 or newer. The default async API uses Tokio with I/O/time.
- Experimental `0.1` API; future `0.x` minor releases may make breaking changes.
- Classic S7 absolute addressing; no S7CommPlus or symbolic PLC I/O.
- Physical PLC, serial PPI, and live DCP acceptance remain pending. Software tests
  and Python cross-peer checks do not certify a controller/firmware combination.
- See [compatibility notes](docs/compatibility.md) for service, string/time,
  emulator, cancellation, and remote-outcome limitations.
