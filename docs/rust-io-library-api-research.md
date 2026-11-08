# Rust I/O Library API Research

Research date: 2026-11-08. Scope excludes the MQTT, Modbus and OPC UA libraries
already discussed. This is a representative cross-domain sample, not a measured
popularity ranking. Versions below are the documentation snapshots observed during
research, not a guarantee of the newest available release. Official documentation
and implementation sources support the findings. Examples illustrate API shape;
they were not compiled here or executed against external services. MongoDB
search/documentation snapshots differ: docs.rs returned a 3.8.2 sync-module
header while the current official guide shows 3.9.1 dependency examples.
The comparison concerns the verified 3.x API shape, not a release ranking.

## Findings

There is no universal Rust convention requiring every I/O library to expose only
async calls, nor one universal implementation of synchronous support. The sample
contains async cores with blocking adapters, independent synchronous/asynchronous
transports, synchronous protocol engines adapted to async, async-only APIs, and
synchronous-only APIs. These distinctions matter more than whether an example
contains `.await`.

| Library / observed snapshot | Caller-facing usage and selection | Implementation / runtime ownership |
| --- | --- | --- |
| reqwest 0.13.5, HTTP | Root `Client` is async; optional `blocking` feature exposes `blocking::Client`. | Blocking implementation feeds the async client through channels to a dedicated thread with a current-thread Tokio runtime. Application owns the runtime for normal async use. [API](https://docs.rs/reqwest/latest/reqwest/), [blocking source](https://docs.rs/reqwest/latest/src/reqwest/blocking/client.rs.html) |
| mongodb 3.8.2 module snapshot, database driver | Root `Client` is async; optional `sync` feature exposes `sync::Client`; action builders use `.await` versus `.run()`. | Sync client wraps async client; sync module owns a lazily initialized Tokio runtime shared through a static. [sync module](https://docs.rs/mongodb/latest/mongodb/sync/index.html), [client source](https://docs.rs/mongodb/latest/src/mongodb/sync/client.rs.html), [runtime source](https://docs.rs/mongodb/latest/src/mongodb/sync.rs.html) |
| postgres 0.19.14 / tokio-postgres 0.7.18, PostgreSQL | Separate crates provide synchronous and asynchronous APIs; not a `sync` feature on one crate. | `postgres` is a thin wrapper over the async client and owns a Tokio runtime; async callers drive the returned connection future. [implementation](https://docs.rs/postgres/latest/postgres/), [runtime construction](https://docs.rs/postgres/0.19.14/src/postgres/config.rs.html), [async usage](https://docs.rs/tokio-postgres/latest/tokio_postgres/) |
| redis-rs 1.8.1, Redis | One root `Client` creates a sync connection with `get_connection()` or an async connection with `get_multiplexed_async_connection().await`. Optional `tokio-comp` / `smol-comp` select async runtime integration. | Sync connection source uses blocking standard-library sockets; async connections live under `aio`. This is not simply a synchronous `block_on` wrapper. [API and features](https://docs.rs/redis/latest/redis/), [sync transport source](https://docs.rs/redis/latest/src/redis/connection.rs.html) |
| tungstenite / tokio-tungstenite 0.30.0, WebSocket | Choose the blocking or Tokio crate; sync uses `read`/`send`, async exposes `Stream`/`Sink`. | `tungstenite` accepts `Read + Write` streams; Tokio adapter reuses its protocol implementation. This demonstrates the opposite direction from an async-core blocking wrapper. [WebSocket API](https://docs.rs/tungstenite/latest/tungstenite/protocol/struct.WebSocket.html), [Tokio adapter](https://docs.rs/tokio-tungstenite/latest/tokio_tungstenite/index.html), [repository](https://github.com/snapview/tokio-tungstenite) |
| SQLx 0.9.0, SQL | Async-facing connection/query API; choose a database and runtime feature, e.g. `postgres` and `runtime-tokio`. No blocking client facade is documented. | Application provides selected async runtime. Feature listing includes additional runtime integrations beyond the root prose; do not infer support solely from older examples. [API](https://docs.rs/sqlx/0.9.0/sqlx/), [features](https://docs.rs/crate/sqlx/0.9.0/features) |
| tonic 0.14.9, gRPC | Async `Channel` connection and generated RPC calls; no blocking facade is advertised. | Standard transport integrates with application Tokio runtime; Channel uses background buffering. [Channel API](https://docs.rs/tonic/latest/tonic/transport/struct.Channel.html), [Cargo features](https://raw.githubusercontent.com/hyperium/tonic/master/tonic/Cargo.toml) |
| async-nats 0.52.0, messaging | Async connect, subscribe and publish; no blocking feature advertised. | Uses Tokio; official docs identify the older synchronous `nats` crate as deprecated/unmaintained. [crate documentation](https://docs.rs/async-nats/latest/async_nats/) |
| rusqlite 0.41.1, embedded SQLite | Synchronous `Connection`, `execute` and `query_row`; no runtime needed for this API. | Useful counterexample for blocking embedded storage, although it is not equivalent to a remote network client. [API](https://docs.rs/rusqlite/latest/rusqlite/) |

## Actual Calling Patterns

### Async core with an optional blocking namespace

For reqwest, enabling the Cargo `blocking` feature adds a separate namespace; it
does not switch the root client into synchronous mode. The application chooses a
type, rather than configuring a per-connection `sync = true` flag.
[Feature and API documentation](https://docs.rs/reqwest/latest/reqwest/blocking/index.html)

```toml
reqwest = { version = "0.13", features = ["blocking"] }
```

```rust,ignore
// Within an async function driven by Tokio:
let client = reqwest::Client::new();
let body = client.get("https://example.com").send().await?.text().await?;

// Within an ordinary synchronous function:
let client = reqwest::blocking::Client::new();
let body = client.get("https://example.com").send()?.text()?;
```

The blocking client starts its runtime machinery internally. Its documentation
explicitly prohibits using blocking APIs directly inside an async runtime and
suggests the async API or `spawn_blocking` for necessary synchronous work. A
blocking facade does not become safe to call on an executor merely because its
internal network stack is async.
[Blocking restrictions](https://docs.rs/reqwest/latest/reqwest/blocking/index.html)

MongoDB similarly selects separate namespaces, but its synchronous action-builder
terminator is `.run()`, not merely deletion of `.await`.
[Official usage guide](https://www.mongodb.com/docs/drivers/rust/current/runtimes/),
[find action implementation](https://docs.rs/mongodb/latest/src/mongodb/action/find.rs.html)

```rust,ignore
// Async function:
let client = mongodb::Client::with_uri_str("mongodb://localhost:27017").await?;
let collection = client.database("demo").collection::<mongodb::bson::Document>("items");
let item = collection.find_one(mongodb::bson::doc! { "id": 1 }).await?;

// Synchronous function, with mongodb feature "sync":
let client = mongodb::sync::Client::with_uri_str("mongodb://localhost:27017")?;
let collection = client.database("demo").collection::<mongodb::bson::Document>("items");
let item = collection.find_one(mongodb::bson::doc! { "id": 1 }).run()?;
```

### One connection factory with separate sync/async connections

Redis demonstrates a materially different API. The same client configuration
creates different connection types; asynchronous operations require runtime
integration and use the async query method. Its documented Tokio integration
spawns work on the selected current runtime; synchronous connections do not
require a hidden Tokio runtime.
[Connection and runtime documentation](https://docs.rs/redis/latest/redis/),
[synchronous source](https://docs.rs/redis/latest/src/redis/connection.rs.html)

```rust,ignore
let client = redis::Client::open("redis://127.0.0.1/")?;

// Synchronous function:
let mut connection = client.get_connection()?;
let value: i64 = redis::cmd("GET").arg("counter").query(&mut connection)?;

// Async function; dependency enables "tokio-comp":
let mut connection = client.get_multiplexed_async_connection().await?;
let value: i64 = redis::cmd("GET").arg("counter").query_async(&mut connection).await?;
```

### Separate crates, or an async-only application boundary

The PostgreSQL pair exposes explicit connection-driving responsibility in its
async API; the synchronous crate handles that responsibility internally.
[Async example](https://docs.rs/tokio-postgres/latest/tokio_postgres/),
[sync implementation](https://docs.rs/postgres/latest/postgres/)

```rust,ignore
// Async function:
let (client, connection) = tokio_postgres::connect(
    "host=localhost user=postgres", tokio_postgres::NoTls,
).await?;
tokio::spawn(async move {
    if let Err(error) = connection.await { eprintln!("{error}"); }
});
let rows = client.query("SELECT 1", &[]).await?;

// Synchronous function, using the postgres crate:
let mut client = postgres::Client::connect("host=localhost user=postgres", postgres::NoTls)?;
let rows = client.query("SELECT 1", &[])?;
```

Other libraries ask callers to remain async: SQLx uses operations such as
`PgPool::connect(url).await?` and
`sqlx::query("SELECT 1").fetch_one(&pool).await?`; tonic connects a Channel with
`.connect().await?`; async-nats exposes awaited connection and publish operations.
These APIs do not imply an official synchronous client merely because callers
could run a future themselves.
[SQLx](https://docs.rs/sqlx/0.9.0/sqlx/),
[tonic](https://docs.rs/tonic/latest/tonic/transport/struct.Channel.html),
[async-nats](https://docs.rs/async-nats/latest/async_nats/)

## Runtime Design Implications for rs-snap7

The following is an architectural inference from the examples, not an industry
requirement or a performance benchmark:

- A shared async S7 execution core with an optional `blocking::Client` is a
  well-supported design option. Separate public types make execution semantics
  clearer than a runtime Boolean on one client type.
- The naming and feature choice need not copy another library exactly: `blocking`,
  `sync`, separate crates and different connection constructors all exist.
- A current-thread runtime driven only during `block_on` stops advancing spawned
  background work between calls. Heartbeat/recovery requirements therefore make
  runtime ownership a deliberate decision. Evaluate a continuously driven shared
  runtime or a dedicated runtime thread with channels rather than mechanically
  wrapping every call. Tokio documents these bridging alternatives explicitly.
  [Tokio bridging guide](https://tokio.rs/tokio/topics/bridging)
- Public sync/async interfaces do not prove internal duplication. Conversely,
  sharing protocol codecs alone does not guarantee shared session/recovery logic;
  inspect execution ownership before choosing a migration strategy.
- Validate clone lifetime, heartbeat progress while callers are idle, deadlines,
  mutation ambiguity, cancellation, nested runtime use and deterministic shutdown
  before adopting a blocking adapter. These are proposed rs-snap7 acceptance
  criteria, not assertions that the sampled libraries behave identically.

No conclusion here claims that async is inherently faster for one PLC request or
that synchronous users are unnecessary. The strongest evidence is that several
successful architectural shapes coexist; choose based on integration requirements
and maintenance cost rather than counting `.await` in examples.
