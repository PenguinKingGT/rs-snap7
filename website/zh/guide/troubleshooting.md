# 连接故障排查

先使用[本地仿真器](server.md)，区分应用/API 错误与网络、CPU 配置问题。
检查顺序参考[上游连接故障指南](https://python-snap7.readthedocs.io/en/latest/connection-issues.html)，
Rust 的错误与重试行为以下文为准。

## 按阶段定位

| 现象 | 后续检查 |
| --- | --- |
| 连接前 `InvalidInput` | 已解析的地址、机架/槽号范围、超时和 PDU 配置 |
| `Io` 或连接被拒绝 | 地址、TCP 端口、监听服务及网络路径；DCP 发现是独立功能 |
| 协商阶段 `Timeout` | CPU 地址、TSAP/机架/槽号和操作截止时间 |
| 连接成功但 DB 访问失败 | DB 编号、完整字节范围、PUT/GET 权限和 DB 标准访问 |
| DB 正常但高级服务失败 | CPU/固件是否支持该服务，保留并检查 PLC 错误码 |
| `BlockingInAsyncContext` | Tokio 中使用异步 Client，已进入 Tokio 的 spawn_blocking 也会被拒绝 |
| 读取数值异常 | 符号、宽度、字节偏移、字节序和 BOOL 位号 |

`is_connected()` 只报告本地就绪状态，远端可能已在上次操作后断开，
成功查询该状态不等于进行了存活探测。

## 明确恢复策略

在连接前配置有界读取恢复与心跳，参见[连接配置](configuration.md)。
恢复后，符合条件的只读初始请求可以重放一次，修改和有状态的后续请求不会重放。

取消操作后，应先显式调用 `reconnect()`。
出现 `AmbiguousWrite` 或 `AmbiguousOperation` 时，应先确认远端状态。
增加重试次数不能解决修改结果不确定的问题。

## 共享客户端与观测

克隆共享串行会话，独立客户端才能实现并发 I/O。批量和连接池结果保留逐项错误。
轮询速度超过 PLC 处理能力会增加排队，可配置 `rate_limit` 并检查 `operation_stats()`。

回调在会话锁外执行，但仍应尽快返回。
同步客户端可通过 `hook_panics`、`background_hook_events_dropped` 观测回调问题。
继续参见[错误处理与限制](errors-and-limits.md)及[错误 API](../api/errors.md)。
