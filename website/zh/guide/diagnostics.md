# 诊断与块操作

以下示例假设已有异步 `client`，函数返回 `rs_snap7::Result<()>`。
可以先在本地仿真器上体验。真实 PLC 的服务可用性取决于型号、配置及访问权限。

## 读取 PLC 信息

```rust
let state = client.get_cpu_state().await?;
let cpu = client.get_cpu_info().await?;
let order = client.get_order_code().await?;
let clock = client.get_plc_datetime().await?;
println!("{state:?}: {cpu:?}, {order:?}, {clock:?}");
```

`read_szl` 支持有界的后续响应收集。
`get_cp_info`、`get_protection` 和 `read_diagnostic_buffer` 可解码其他服务模型。

## 块列表与上传

```rust
use rs_snap7::BlockType;

let blocks = client.list_blocks().await?;
let info = client.get_block_info(BlockType::Db, 1).await?;
let payload = client.upload(1).await?;
let complete = client.full_upload(BlockType::Db, 1).await?;
```

`upload` 返回移除紧凑头后的 DB 字节，`full_upload` 返回完整块表示。
`download(data, Some(number))` 接受 **DB 块表示**，不是任意内存写入缓冲区。
传入 `None` 时从第 6–7 字节提取 DB 编号，短数据必须显式指定编号。
普通字节写入请使用 `db_write`。

## 控制与时钟操作

`plc_stop`、`plc_hot_start`、`plc_cold_start`、`compress`、`copy_ram_to_rom`、
`set_plc_datetime` 以及块删除/下载都会修改远端状态，应由应用明确发起。
确认丢失后不会静默重放。

`DateTime` 不带时区。`set_plc_system_datetime()` 发送 UTC；
PLC 采用其他本地时间约定时，请显式传入 `DateTime`。

`force_bit`/`cancel_force` 修改过程映像位，不会建立持久 CPU 强制状态。
方法签名和错误约定见[完整 API](../api/index.md)。
