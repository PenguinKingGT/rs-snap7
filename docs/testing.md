# Validation Guide

## Local Checks

Run from the repository root:

```sh
cargo build --locked --all-features
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --no-default-features
cargo test --locked --all-targets --all-features
cargo test --locked --doc --all-features
```

Normal tests use ephemeral loopback TCP ports, independent wire peers, fixed
vectors, and deterministic policy inputs. They require neither PLC hardware nor
Python/the ignored upstream checkout. The default library has no async runtime
dependency; enabling `async` exercises native Tokio socket paths as well.

## Coverage by Behavior

| Area | Evidence |
| --- | --- |
| Address/type validation | `tests/types.rs`: identifiers, wire address vector, overflow, area/width mismatches, error causes |
| Scalar/text codecs | `tests/codec.rs`: endian vectors, every i16 value, bit preservation, float bit patterns, string encodings and malformed lengths |
| Calendar/time codecs | `tests/datetime.rs`: BCD vectors, complete supported DATE range, long-type limits, malformed dates |
| ISO transport | `tests/transport.rs` and module tests: partial reads, fragmentation/reassembly, peer references, invalid headers, size limits, timeout cleanup |
| S7 protocol | `tests/protocol.rs`: independent setup/read/write vectors, exact lengths, reference rollover, mixed item outcomes, malformed input |
| Client memory behavior | `tests/memory.rs`: chunking, automatic DB sizing, typed/array/string access, batching, concurrent bit updates, partial and uncertain writes |
| Read optimization | `src/planner.rs` tests and `tests/pool.rs`: sort/merge/extraction, errors isolated after merging, cached layout keys, independent parallel sessions |
| Recovery/lifecycle | `tests/recovery.rs`: bounded reconnect, smaller-PDU replanning, unreplayed writes, retained LOGO TSAPs, heartbeat and explicit shutdown |
| Async cancellation | `tests/async_client.rs`: cancelled transmitted requests close sockets; silent peers yield executor time and expire |
| Block services | `tests/blocks.rs` and shared machine tests: pinned wire vectors, split catalogs, offline metadata, complete upload/download stages, PLC references, limits, refusal versus ambiguity, timeout and between-exchange cancellation |
| Management/diagnostics | `tests/management.rs`, `diagnostics.rs`, `security.rs`: PI vectors, clock layouts, SZL continuation, firmware priority, password vectors/errors, force records |
| Limits/events | `src/policy.rs`, `observability.rs` tests and integration scenarios: fake time, bursts, deadlines before sending, one logical event, reentrant/failed hooks, JSON escaping |

Async counterparts run the same memory, lifecycle, diagnostic, control, and
password scenarios against the same independent peer implementations. Read-pool
coordination and socket waits are bounded so failed concurrency tests can exit.
Numeric coverage percentages have not been measured; test counts do not imply
complete line/branch coverage.

## CI & Remaining Acceptance Work

[CI](../.github/workflows/ci.yml) configures default/all-feature tests on Linux,
Windows, and macOS, plus formatting, strict Clippy, and documentation checks.
Local validation is on macOS; remote matrix results require a pushed workflow run.

F28 remains **In progress**. Automated cross-implementation tests against the
pinned Python peer, sustained stress/resource tests, and an opt-in real-PLC
acceptance harness remain to be implemented. Existing inline wire vectors derive
from the baseline recorded in [NOTICE](../NOTICE) and the migration tracker;
no claim of real-hardware compatibility is made from emulator tests alone.

Future hardware runs must record CPU model, firmware, connection settings, fixture
DB layout, enabled write/control scenarios, and results. Keep hardware tests
explicitly opt-in; they must never become a prerequisite for normal `cargo test`.
