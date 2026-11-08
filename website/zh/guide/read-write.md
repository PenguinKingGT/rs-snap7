# 读取与写入

以下示例假设已有异步 `client`，应用函数返回 `rs_snap7::Result<()>`。

## 原始 DB 字节

```rust
let bytes = client.db_read(1, 0, 16).await?;
client.db_write(1, 16, &[1, 2, 3, 4]).await?;
```

`db` 为 DB 编号，`start` 为字节偏移，`size` 为字节数。
大块传输按协商 PDU 自动分片。已知长度时优先显式指定；
`db_get(db, None)` 会通过块元信息发现 DB 大小。

## 类型化数值与位

```rust
client.db_write_int(1, 0, -42).await?;
let value = client.db_read_int(1, 0).await?;
client.db_write_real(1, 4, 3.5).await?;
let values = client.db_read_array::<u16>(1, 8, 4).await?;
client.db_write_bool(1, 20, 3, true).await?;
```

类型化转换自动处理 PLC 字节序。BOOL 的 `bit` 范围为 0–7，
读取、修改和写回过程持有同一会话锁，避免该会话的其他调用覆盖相邻位。
它无法阻止独立 PLC 程序或其他连接同时修改该字节。

## 其他区域

```rust
use rs_snap7::Area;

let markers = client.read_area(Area::Markers, 0, 0, 8).await?;
client.write_area(Area::Markers, 0, 8, &[0x55, 0xaa]).await?;
```

非 DB 区域的 DB 编号为零。输入、输出和标记区使用字节偏移。
计时器与计数器的偏移和数量以**元素**为单位，原始负载的每个元素占两字节，
写入缓冲区必须包含完整的 16 位元素。

## 多变量操作

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

一次调用最多 20 个变量，结果顺序与输入一致。外层结果表示传输/协议失败，
内层结果表示各 PLC 项目的处理结果，外层成功不代表所有项目均成功。

`write_multi_vars` 接受 `(Address, &[u8])` 对，并返回逐项结果。
跨请求或多变量操作不是 PLC 原子事务；出现错误后，应先确认已完成与不确定的范围。
