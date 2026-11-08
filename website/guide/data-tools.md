# Tags and DB Snapshots

## Typed tags

`Tag` accepts explicit PLC4X or nodeS7 syntax and checks addresses/types before I/O.

```rust
use rs_snap7::tags::{Tag, Value};

let speed: Tag = "DB1:0:INT".parse()?;
client.write_tag(&speed, &Value::Signed(120)).await?;
let value = client.read_tag(&speed).await?;
let flags: Tag = "DB1,X4.3".parse()?;
```

Use `read_multi_vars` for optimized raw batching. `read_tags` preserves order and
per-tag errors, but reads tags sequentially. Importing symbolic names does not
enable symbolic PLC addressing.

Enable `tag-import` for CSV/JSON/TIA XML text import. Applications own file I/O:

```rust
let text = std::fs::read_to_string("tags.csv")?;
let tags = rs_snap7::tags::import::load_csv(&text)?;
```

## An owned DB snapshot

Declare exact byte/bit offsets rather than inferring a PLC compiler layout:

```rust
use rs_snap7::{Area, db::{Db, DbConfig, Layout}, tags::Value};

let layout = Layout::parse("0 enabled BOOL\n2 speed INT", 0)?;
let mut db = Db::new(
    DbConfig {
        area: Area::Db,
        db_number: 1,
        start: 0,
        row_size: 4,
        row_gap: 0,
        rows: 1,
        writable_start: 0,
    },
    layout,
    vec![0; 4],
)?;
db.read_row(&client, 0).await?;
db.set(0, "speed", &Value::Signed(120))?;
db.write_row(&client, 0, true).await?;
assert!(db.unchanged(0)?);
```

`Db` owns the snapshot and acknowledged baseline. Passing `true` writes only
changed writable byte runs. Acknowledged runs become clean; failed runs remain
dirty. `row_gap` is preserved, and `writable_start` protects the prefix of each row.

Use `write_fields(&client, row, names)` to update BOOL fields while preserving
remote neighboring bits. Whole-row writes replace whole bytes. Reading a BOOL
field refreshes its entire storage byte and discards local edits in that byte.

With a blocking client use `read_row_blocking`, `write_row_blocking`,
`read_fields_blocking`, and `write_fields_blocking` on the same snapshot.
