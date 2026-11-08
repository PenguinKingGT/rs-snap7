# Reading and Writing

Examples below assume an already connected async `client` and an application
function returning `rs_snap7::Result<()>`.

## Raw DB bytes

```rust
let bytes = client.db_read(1, 0, 16).await?;
client.db_write(1, 16, &[1, 2, 3, 4]).await?;
```

`db` is a DB number, `start` is a byte offset, and `size` is a byte count. Large
transfers split automatically using negotiated PDU limits. Prefer a known length
when available; `db_get(db, None)` discovers the DB size from block metadata.

## Typed values and bits

```rust
client.db_write_int(1, 0, -42).await?;
let value = client.db_read_int(1, 0).await?;
client.db_write_real(1, 4, 3.5).await?;
let values = client.db_read_array::<u16>(1, 8, 4).await?;
client.db_write_bool(1, 20, 3, true).await?;
```

Typed codecs handle PLC byte order. BOOL `bit` is 0–7; read/modify/write holds the
session guard and preserves adjacent flags against other calls on that session.
It cannot prevent an independent PLC program or connection changing the byte.

## Other areas

```rust
use rs_snap7::Area;

let markers = client.read_area(Area::Markers, 0, 0, 8).await?;
client.write_area(Area::Markers, 0, 8, &[0x55, 0xaa]).await?;
```

Non-DB areas use DB number zero. Inputs, outputs, and markers use byte offsets.
Timers and counters use **element** offsets/counts and raw two-byte payloads.
Their write buffers must contain complete 16-bit elements.

## Multi-variable operations

```rust
use rs_snap7::Address;

let addresses = [Address::db(1, 0, 2)?, Address::db(1, 4, 4)?];
let results = client.read_multi_vars(&addresses).await?;
for (address, result) in addresses.iter().zip(results) {
    match result {
        Ok(bytes) => println!("{address:?}: {bytes:?}"),
        Err(error) => eprintln!("{address:?}: {error}"),
    }
}
```

A batch accepts at most 20 variables and preserves input order. The outer result
reports transport/protocol failure; each inner result reports the PLC item result.
A successful outer call does not mean every item succeeded.

`write_multi_vars` takes `(Address, &[u8])` pairs and returns per-item results.
Multiple requests or variables are not an atomic PLC transaction. Check confirmed
and uncertain progress after errors before deciding what to do next.
