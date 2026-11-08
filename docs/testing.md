# Validation Guide

## Local Checks

Run from the repository root:

```sh
cargo build --locked --all-features
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --no-default-features
cargo test --locked --all-targets --features blocking
cargo test --locked --all-targets --all-features
cargo test --locked --doc --all-features
```

Normal tests use ephemeral loopback TCP ports, independent wire peers, fixed
vectors, and deterministic policy inputs. They require neither PLC hardware nor
Python/the ignored upstream checkout. The default client uses native Tokio sockets;
`blocking` tests the optional facade over the same core.

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


## Cross-implementation Acceptance

The ignored Python tests validate the pinned upstream Git revision before starting
either peer. They cover Rust client → Python server and Python client → Rust server,
including chunked DB transfers, typed values, catalogs, metadata, uploads and
downloads. The deliberate emulator download-storage difference is documented
in the compatibility guide.

```sh
SNAP7_PYTHON=python3.12 cargo test --test python_compat --features server -- --ignored --nocapture
```

Python must be >=3.10. SNAP7_PYTHON_REFERENCE can select another checkout at the
same baseline revision; no pip installation, pytest or PLC is required.
Normal tests never require Python or that checkout. On this workstation, both
directions passed using Python 3.12 and the local baseline.

## Deterministic Properties and Stress

```sh
cargo test --test properties --all-features
cargo test --test stress --features server
```

Properties use fixed xorshift seeds: 10,000 independent endian/float-bit cases and
20,000 malformed packets, headers, date/string payloads and address strings.
Optional codecs/importers participate when enabled. These reproducible sweeps
supplement targeted boundary tests; they are not exhaustive fuzzing or measured
branch coverage. Stress tests run eight independent sessions through 1,600
write/read cycles, then churn 64 more connections and assert session cleanup.

## Explicit Hardware Acceptance

No physical PLC, serial PPI adapter or privileged DCP network has been exercised.
The ignored hardware harness requires explicit endpoint and fixture metadata.
Use a dedicated standard-access DB whose writable bytes are not used by a running
process. Hardware acceptance results remain pending until these tests are run in
a documented environment.

```sh
export SNAP7_RUN_HARDWARE=1
export SNAP7_PLC_ENDPOINT=192.168.1.10:102
export SNAP7_PLC_MODEL='actual CPU model'
export SNAP7_PLC_FIRMWARE='actual firmware version'
export SNAP7_FIXTURE_DB=100
export SNAP7_FIXTURE_START=0
export SNAP7_FIXTURE_SIZE=16
# Optional: SNAP7_PLC_RACK, SNAP7_PLC_SLOT, SNAP7_HARDWARE_REPORT
cargo test --features blocking --test hardware hardware_read -- --ignored --nocapture

# Separate explicit mutation opt-in; original bytes are saved and restored.
SNAP7_ALLOW_HARDWARE_WRITES=1 cargo test --features blocking --test hardware hardware_fixture_write -- --ignored --nocapture
```

Report records include observed CPU/order data, declared firmware/model, endpoint
and UTC epoch. A write failure triggers best-effort restoration; recovery after an
ambiguous failure may require operator verification. CPU control/force/block
mutation, PPI serial and live DCP acceptance need separate lab scenarios; the
harness deliberately does not infer permission to run these operations.

## CI

[CI](../.github/workflows/ci.yml) configures default/blocking/all-feature tests on Linux,
Windows and macOS, plus formatting, strict Clippy and documentation checks.
Individual feature compilation runs separately to catch accidental coupling.
Remote matrix results require a pushed workflow run; local results are macOS.
F28's software suite and opt-in hardware entry points are implemented; physical
acceptance and remote CI execution remain validation activities, not claimed
passes.

## Recorded Local Run

On 2026-11-08, macOS with Rust 1.97.1 and Python 3.12:

| Check | Result |
| --- | --- |
| Locked default/no-default all-target tests | 101 passed; hardware harness requires blocking |
| Locked blocking all-target tests | 152 passed; 2 hardware tests ignored |
| Locked all-feature all-target tests | 190 passed; 4 opt-in tests ignored |
| Default and all-feature doctests | 3 passed in each configuration |
| Explicit pinned Python cross-peer tests | 2 passed, including download in both directions |
| Individual blocking/tag-import/server/partner/ppi/discovery/cli/demo builds | All eight passed |
| rustfmt; strict default/blocking/all-feature Clippy; rustdoc with warnings denied | Passed |

The four all-feature opt-in tests are two Python peer tests and two real-PLC
tests. Python tests were then run explicitly and passed. The two real-PLC tests
were not run. Remote operating-system matrix jobs and live PPI/DCP acceptance
were not executed. Test totals are test-function counts, not branch coverage.

## Review Regression Cases

The following cases supplement the baseline suite:

| Behavior | Regression evidence |
| --- | --- |
| Smaller PDU after reconnect | recovery tests read 1000 bytes/elements from nonzero offsets, lose the first or a later chunk, preserve acknowledged prefixes, and replan all unread DB/timer chunks in blocking/async clients |
| Exact write lengths | server engine tests corrupt the second item's bit/byte length and assert no item modifies memory; valid BYTE/INT/REAL/CHAR writes still succeed |
| DINTEGER transport 0x06 | protocol tests cover scalar/array byte lengths, malformed widths and mixed alignment; independent peers verify subsequent requests keep blocking/async connections usable |
| Complete mutation acknowledgement | mutation_ack tests cover all CPU control, clock-write and password operations, wrong services, unexpected payload/continuation, definite refusals and unsent errors, with no replay and matching sync/async behavior |
| Final-item read padding | server tests cover PDU 128/129/130, exact single/multiple-item responses, nonfinal padding and refusal when the actual response exceeds the limit by one byte |

These regressions run without hardware; optional Python peer acceptance can be
run separately using the command above.

## Async Core Refactor Regressions

`tests/async_transport.rs` covers raw async COTP fragmentation, malformed frames,
timeouts, and cancellation invalidation without optional features.
`tests/blocking.rs` covers runtime-context rejection, idle heartbeat progress,
clone/final-drop lifetime, non-Send scalar values, caller-thread callback reentry,
and pool task callback propagation. Private runtime/hook tests verify concurrent
runtime reuse, entered blocking-pool rejection, bounded queue overflow accounting,
late completion delivery, panic isolation, and a slow background sink that does
not stall the reactor. Memory tests preserve DB dirty-state checkpoints after a
later run fails. The hardware harness requires `blocking` to retain synchronous
panic-safe fixture restoration.
