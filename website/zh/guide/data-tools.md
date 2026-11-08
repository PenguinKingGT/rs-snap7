# 标签与 DB 快照

## 类型化标签

`Tag` 接受显式 PLC4X 或 nodeS7 语法，在 I/O 前校验地址和数据类型。

```rust
use rs_snap7::tags::{Tag, Value};

let speed: Tag = "DB1:0:INT".parse()?;
client.write_tag(&speed, &Value::Signed(120)).await?;
let value = client.read_tag(&speed).await?;
let flags: Tag = "DB1,X4.3".parse()?;
```

优化原始批量读取请使用 `read_multi_vars`。`read_tags` 保留顺序和逐标签错误，
但按顺序逐个读取。导入符号名称不代表支持符号 PLC 寻址。

启用 `tag-import` 后可导入 CSV、JSON 和 TIA XML 文本，文件读取由应用负责：

```rust
let text = std::fs::read_to_string("tags.csv")?;
let tags = rs_snap7::tags::import::load_csv(&text)?;
```

## 自有 DB 快照

显式声明字节/位偏移，不推断 PLC 编译器布局：

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

`Db` 同时拥有快照与已确认基线。传入 `true` 只写入已变更且可写的连续字节区间。
已确认区间标记为未变更，失败区间保留待写状态。
`row_gap` 间隔不会写入，`writable_start` 保护每行的只读前缀。

需要保留远端相邻 BOOL 位时，使用 `write_fields(&client, row, names)`。
整行写入会替换完整字节。读取 BOOL 字段会刷新整个存储字节，并丢弃该字节中的本地编辑。

同步客户端使用同一快照上的 `read_row_blocking`、`write_row_blocking`、
`read_fields_blocking` 与 `write_fields_blocking`。
