# Async Core and Blocking Adapter Refactor Plan

Status: **Implemented.** This document records the execution-model refactor
and complements the current [architecture](architecture.md).
It is informed by the [library research](rust-io-library-api-research.md).
The project is unpublished, so the migration can make deliberate interface
changes without keeping a permanent compatibility implementation.

## 1. Decision and Scope

Make the main S7 client asynchronous by default, backed by Tokio. Provide an
optional `blocking` module that executes the same client implementation.
One implementation owns negotiation, session serialization, deadlines, recovery,
limiting, heartbeat, transfer execution and mutation-result classification.
Blocking methods adapt invocation; they do not implement those workflows again.

Keep codecs, types, packet construction, parsers, tag parsing, DB snapshots and
planning independent of execution. Preserve every implemented migration feature
and the five recent review regressions. The protocol and on-wire behavior do
not change as a consequence of this refactor.

This phase covers the S7 client, read pool, LOGO adapter, DB I/O helpers and their
CLI/examples. The server, Partner, serial PPI and raw DCP adapters keep their
existing execution models and documented limits. Making those implementations
native async would be separate work. In particular, Partner's existing
`send_async`/`receive_async` remain blocking-pool adapters in this phase.

## 2. Public Interface and Cargo Features

| Current interface | Target interface |
| --- | --- |
| Blocking `rs_snap7::Client` | Async `rs_snap7::Client`; I/O calls require `.await` |
| `AsyncClient`, gated by `async` | Becomes `Client`; remove the old name after migration |
| Existing blocking client use | `rs_snap7::blocking::Client`, gated by `blocking` |
| `AsyncClientPool` / blocking `ClientPool` | Async `ClientPool` / `blocking::ClientPool` |
| Blocking `Logo` | Async `Logo` plus `blocking::Logo`; reuse VM parsing and value rules |
| DB `*_async` I/O methods | Async methods with short names, e.g. `read_row`; optional `read_row_blocking` adapters on the same owned `Db` snapshot |
| Blocking `transport::Connection` | Async `transport::Connection`; explicit `transport::blocking::Connection` for advanced blocking consumers and existing server/Partner adapters |
| `ClientConfig` | Retain its purpose, fields and synchronous validation; move to a separate configuration module |

The target default async client is always available. Tokio becomes a required
non-Wasm dependency for the supported network client. Remove the `async` Cargo
feature and its implementation gates after updating all consumers; there is no
connection-level `sync = true` switch and no permanent legacy alias.

The optional `blocking` feature adds the blocking namespace and Tokio's
`rt-multi-thread` capability. Keep other product features additive. The default
crate will no longer promise a Tokio-free dependency graph, even when consumers
only use codecs. Do not split a separate protocol crate solely for this migration.
If retaining a runtime-free package later becomes a concrete requirement, evaluate
that independently.

Illustrative target usage, **not valid against the current implementation**:

```rust,ignore
// Async application, running on its own Tokio runtime:
use rs_snap7::{Client, ClientConfig};
let client = Client::connect(config).await?;
let value = client.db_read_int(1, 0).await?;
client.disconnect().await?;

// Synchronous application; enable Cargo feature "blocking":
use rs_snap7::blocking::Client;
let client = Client::connect(config)?;
let value = client.db_read_int(1, 0)?;
client.disconnect()?;
```

These are separate calling-context sketches, not one compilable function.
Non-I/O metadata such as `operation_stats()` may remain synchronous. Operations
that inspect session state through an asynchronous lock remain awaited in the
async interface; their blocking counterparts return `Result`, including the
proposed `blocking::Client::is_connected() -> Result<bool>`.

## 3. Runtime Ownership for the Blocking Adapter

**Choose one process-wide, lazily initialized, continuously driven Tokio runtime
for blocking clients.** Use a multi-thread runtime configured with one background
worker initially. All blocking clients and pools share it; clones do not create
threads or runtimes. Async users continue to use their application's runtime and
do not initialize this blocking runtime.

The runtime remains alive until process exit. This deliberately trades one
bounded process-lifetime worker for simple runtime ownership, heartbeat progress
and safe dropping of client handles from arbitrary threads. No public runtime
configuration/shutdown interface is added in this phase. Dropping the final
client must still release its connection and abort its own background tasks;
retaining the shared runtime must not retain client sessions.

A private blocking adapter enters the shared runtime to execute the core future
and wait for its result. Reuse the runtime for all calls; never create a runtime
per method. Allow borrowed buffers and local codec types where the underlying
operation permits them; do not require every operation to cross an owned
`Send + 'static` command queue merely to implement blocking calls.

A runtime driven only by occasional current-thread `block_on` calls would suspend
background tasks between calls, so that design is unsuitable for the existing
idle heartbeat contract. This is documented in the
[Tokio bridging guide](https://tokio.rs/tokio/topics/bridging).

Calling a blocking operation from an entered Tokio runtime returns a new explicit
`Error::BlockingInAsyncContext` before network I/O or session mutation. Users in
async code use `Client` or an ordinary OS thread outside Tokio. The conservative
context check also rejects `spawn_blocking`, whose threads enter a Tokio handle. Check the calling context before initializing/entering the adapter;
do not rely on a nested-runtime panic. Pure codec and snapshot methods are not
subject to this restriction.

Do not expose unchecked conversions from an application-runtime client into a
blocking client. Establish blocking sessions on the adapter's runtime so their
heartbeat and transport lifetime are predictable.

## 4. Shared Execution, Lifetime and Hooks

The session remains a deep module: its interface owns complete logical
operations and exposes validated results. There is one serialized operation per
connection; pools use independent sessions. Introduce no public universal
transport trait or generic executor abstraction merely to bridge sync and async.

Preserve these invariants:

- All chunks, retries and continuations share the original logical-operation
  deadline. Reconnection replans the entire unread range using new PDU limits.
- Cancelling a transmitted async request invalidates its connection. Mutations
  with untrustworthy acknowledgements retain ambiguous-outcome errors and never
  replay automatically. The adapter does not turn a wait timeout into cancellation
  unless the underlying core future is actually cancelled.
- Blocking calls await the core's bounded result; they do not add a second timeout
  that might return while a detached write is still running.
- Client clones share session state and heartbeat ownership. `disconnect()` affects
  all clones, blocks new requests on that session and completes its cleanup;
  `reconnect()` creates fresh negotiation/reference state.
- Final client drop aborts heartbeat and closes resources without waiting for a
  network timeout. A pool adapter delegates to the async pool rather than creating
  another implementation of scheduling and result restoration.
- Keep the observer's bounded-future construction and one-event-per-logical-call
  behavior. Wrapping a core call must not emit a duplicate operation event.

Blocking hooks require explicit adaptation. Capture operation and connection
events in a per-call context, release session guards and leave the runtime's
blocking-entry context, then invoke application hooks on the calling thread.
Preserve request context through any spawned pool work. This permits existing
hook code to inspect the client without nested-runtime errors or held locks.
Background heartbeat events have no waiting caller: use a 1,024-event callback
queue and a separate ordinary callback dispatcher, started only when needed.
On overflow, drop the incoming background event and increment a per-client
`background_hook_events_dropped` counter exposed in operation statistics. Keep
FIFO order for accepted events; live session state remains authoritative. Hook panics remain isolated. Hook delivery must never stall the
runtime's I/O worker or wait for a callback while holding a session guard.

The callback dispatcher is an additional bounded process-lifetime thread when
background hooks are enabled. Recursive callback-generated events must enqueue
without waiting for their own callback completion. Explicitly document that
background hook delivery can be asynchronous.

## 5. Module Changes and Integration

| Module | Change |
| --- | --- |
| `config` | Extract `ClientConfig`; keep validation free of network I/O |
| `client` | Promote the current native async client and operation modules |
| `client::session` | Keep one async session/transfer driver; remove the blocking duplicate |
| `transport` | Separate shared configuration/framing, Tokio transport and the explicit blocking raw adapter required by optional products |
| `operations` / planner | Retain pure chunk/read planning; move execution to the single session implementation |
| `blocking` | Client/pool/LOGO adapters, shared runtime and hook-delivery adaptation; no S7 packet/retry logic |
| `db`, `logo` | Share layout/address/value logic; blocking I/O delegates to the corresponding async operation while preserving DB dirty-state updates |
| CLI and examples | Use async client calls; expose separate blocking examples with their feature requirement |

Keep `Db`'s owned snapshot and selective-write checkpoint semantics. Its blocking
adapters may borrow the same `Db` while waiting; do not clone a whole snapshot
and lose partial-write baseline changes. Keep `Scalar` requirements on the codec
itself unchanged; do not add runtime bounds to pure conversion traits.

The CLI should use an async entry point and awaited client operations. Existing
blocking discovery, metrics sampling and optional product shutdown must execute
on blocking workers when invoked from that entry point. Do not mechanically
retain blocking sleeps or raw capture on the async executor. The `cli` feature
need not enable `blocking` just to call the S7 client.

Update both READMEs, rustdoc, usage examples, feature tables, compatibility notes,
testing instructions and hardware/Python harness imports together with the
interface switch. Keep the feature tracker distinct from this refactor's staged
completion; implemented protocol features do not become unimplemented.

## 6. Implementation Sequence

Each step is a focused commit with relevant tests. Transitional duplication is
allowed only while the replacement is being verified; remove it before completion.

| Step | Deliverable and completion gate |
| --- | --- |
| S1 | Inventory all exported client methods, integration callers, hooks and feature gates; freeze current independent-peer and review-regression behavior. Record baseline test results. |
| S2 | Extract configuration and establish the existing async implementation as the internal execution core without changing the public default yet. Core read/write, recovery, transfer, cancellation and observer tests pass. |
| S3 | Implement the optional blocking runtime/adapter with complete client method coverage, hook handling and lifecycle tests. Compare it against the legacy blocking client using the same independent peers. |
| S4 | Migrate pools, LOGO, DB I/O helpers, CLI/examples, raw transport paths and acceptance harnesses. Switch root names/default execution, remove the old `async` feature, legacy blocking session and duplicate operation execution in one coordinated change. All target feature combinations compile. |
| S5 | Finish documentation and runtime-resource/concurrency validation. Audit remaining duplication and confirm every invariant/test below before declaring completion. |

## 7. Acceptance Checklist

- Default async and `blocking` feature builds; every existing optional feature
  alone; CLI without `blocking`; all-feature build, rustfmt, strict Clippy and
  warning-free rustdoc.
- All existing protocol/codec, mutation, recovery, server, Partner, PPI and DCP
  regressions, with consumer imports adjusted rather than tests deleted.
- The same independent-peer memory, block, diagnostics, security, limiter and
  error scenarios through direct async and blocking calls.
- Idle blocking client heartbeat advances while the application sleeps; multiple
  blocking clients share bounded runtime resources; independent pool sessions
  execute concurrently while clones of one session remain serialized.
- Dropping one clone retains connectivity; final drop releases sockets/tasks;
  disconnect/reconnect and failures during initialization leave no usable partial
  session. Dropping blocking handles in a Tokio context does not panic.
- Blocking use in an entered runtime returns the documented error before any bytes
  are sent; cancellation, malformed ACKs and interrupted writes retain their
  existing session invalidation and uncertain-result behavior.
- Hook reentry for session metadata, panicking hooks, slow/background hooks,
  callback queue overflow, no duplicated logical events and pool context propagation.
- Mutable DB snapshots retain acknowledged dirty-range progress on errors, and
  generic codec helpers do not acquire unnecessary `Send + 'static` bounds.
- Updated examples and both Python interoperability directions. Real hardware
  acceptance remains explicitly opt-in and is reported separately.

## 8. Delivery Record

- S1: Baseline all-feature tests passed before execution-path removal.
- S2: `1f0f75e` extracted configuration and enabled the async core unconditionally.
- S3: `93757e` introduced the shared-runtime blocking facade and hook adaptation.
- S4: `4c9603e` switched public names, removed the old session/client drivers,
  and migrated DB/LOGO/pools, raw transport, CLI, examples, and acceptance callers.
- S5: Documentation, the blocking example, and CI matrices describe the new API.
  Validation results are recorded in [testing.md](testing.md).

The hardware harness intentionally uses optional `blocking` so its panic-safe
fixture restoration remains synchronous. Software acceptance tests exercise
native async and blocking behavior, including Python peers in both directions.
No physical PLC, live PPI/DCP, or remote CI pass is claimed.
