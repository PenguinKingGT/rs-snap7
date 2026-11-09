# Multi-variable Reads and Pools

Use `read_multi_vars` to fetch several absolute addresses while preserving
per-item results. It takes at most 20 addresses per call; PDU limits can require
multiple exchanges. Batch operations are not atomic PLC snapshots.

```rust
use rs_snap7::{Address, codec};

let addresses = [Address::db(1, 0, 2)?, Address::db(1, 4, 4)?];
let mut values = client.read_multi_vars(&addresses).await?.into_iter();
let count_bytes = values.next().expect("first result")?;
let speed_bytes = values.next().expect("second result")?;
let count = codec::read::<i16>(&count_bytes, 0)?;
let speed = codec::read::<f32>(&speed_bytes, 0)?;
```

The outer `Result` reports an operation failure. Every returned item has its own
`Result`; handle an item's PLC error even when the outer result succeeded.
Keep addresses in the order needed by your application; results retain that order.

## Parallel reads across sessions

`Client` clones share one session. A `ClientPool` requires 1–32 **independent**
connections and rejects repeated clones of the same session.

```rust
use rs_snap7::{Client, ClientPool};

let first = Client::connect(config.clone()).await?;
let second = Client::connect(config).await?;
let pool = ClientPool::new(vec![first.clone(), second.clone()])?;
let values = pool.read_multi_vars(&addresses).await?;
first.disconnect().await?;
second.disconnect().await?;
```

Independent reads can run concurrently; they do not guarantee a shared PLC-cycle
view. Cancelling a pool future aborts active work and can invalidate connections
whose requests were already transmitted.

For this crate, `read_tags` performs sequential typed reads. Use raw batches or
the pool when optimized packet dispatch matters. The Python
[multi-variable tutorial](https://python-snap7.readthedocs.io/en/latest/multi-variable.html)
describes the same task; Rust uses owned results rather than ctypes buffers.
