<script setup>
import { withBase } from 'vitepress';
</script>

# API 参考

本页提供中文接口索引。构建网站时会从当前源码自动生成全部公开 API，
包含方法签名、参数、返回值、错误说明及源码链接；Rustdoc 正文使用英文。

<a :href="withBase('/rustdoc/rs_snap7/index.html')" target="_blank" rel="noopener">打开完整 Rust API</a>

<a :href="withBase('/rustdoc/rs_snap7/client/struct.Client.html')" target="_blank" rel="noopener">异步 Client</a> ·
<a :href="withBase('/rustdoc/rs_snap7/blocking/struct.Client.html')" target="_blank" rel="noopener">同步 Client</a> ·
<a :href="withBase('/rustdoc/rs_snap7/struct.ClientConfig.html')" target="_blank" rel="noopener">ClientConfig</a> ·
<a :href="withBase('/rustdoc/rs_snap7/error/enum.Error.html')" target="_blank" rel="noopener">Error</a>

`0.1.0` 已发布。也可直接从仓库生成 Rustdoc，查看全部公开方法、类型、参数与错误约定：

```sh
# Run from the repository root.
cargo doc --no-deps --all-features --open
```

入口为 `target/doc/rs_snap7/index.html`。完整 API 以 Rustdoc 为准，
文档站提供按任务组织的指南与模块索引。

## 公开接口索引

| 入口 | 用途 |
| --- | --- |
| `Client`、`ClientConfig` | 异步连接、内存、诊断、控制、块与安全服务 |
| `ClientPool` | 跨独立连接的有界并发读取 |
| `blocking::{Client, ClientPool, Logo}` | 可选同步接口，复用异步核心 |
| `Logo`、`logo::VmAddress` | LOGO! VM 地址校验与读写 |
| `Address`、`Area`、`WordLen` | 内存地址与传输宽度校验 |
| `codec` | 标量、字符串、位与日期时间转换 |
| `tags::{Tag, Value}` | 类型化地址、数值与元数据 |
| `db::{Db, DbConfig, Layout}` | 自有快照、布局、行/字段操作与变更跟踪 |
| `policy`、`observability` | 恢复、限流、事件、回调与操作统计 |
| `blocks`、`szl` | 块信息与诊断模型 |
| `protocol`、`transport` | 高级 S7 PDU 编解码与原始 ISO-on-TCP 访问 |
| `Error`、`Result` | 校验、传输、PLC 及不确定结果错误 |

## 常用客户端操作

| 任务 | 方法 |
| --- | --- |
| 生命周期 | `connect`、`disconnect`、`reconnect`、`is_connected`、`pdu_length` |
| 原始 DB 内存 | `db_read`、`db_write`、`db_get`、`db_fill` |
| 类型化 DB 数值 | `db_read_int`、`db_write_int`、`db_read_real`、`db_write_real` |
| 位访问 | `db_read_bool`、`db_write_bool` |
| 批量访问 | `read_multi_vars`、`write_multi_vars` |
| 类型化标签 | `read_tag`、`write_tag`、`read_tags` |
| 诊断 | `get_cpu_state`、`get_cpu_info`、`get_order_code`、`read_szl` |
| 控制与时钟 | `plc_stop`、`plc_hot_start`、`get_plc_datetime`、`set_plc_datetime` |
| 块服务 | `list_blocks`、`get_block_info`、`upload`、`full_upload`、`download` |

完整入口见[客户端源码](https://github.com/PenguinKingGT/rs-snap7/blob/main/src/client/mod.rs)、
[同步封装](https://github.com/PenguinKingGT/rs-snap7/blob/main/src/blocking/mod.rs)与[可选特性](../guide/optional-products.md)。

## 按模块查阅

- [客户端](client.md)
- [仿真器](server.md)
- [Partner](partner.md)
- [LOGO!](logo.md)
- [PPI](ppi.md)
- [数值编解码](codec.md)
- [标签](tags.md)
- [DB 快照](db.md)
- [连接池、策略与事件](optimization.md)
- [诊断与块模型](diagnostics.md)
- [设备发现](discovery.md)
- [类型与地址](types.md)
- [错误](errors.md)
- [主机指标 Demo](demo.md)
