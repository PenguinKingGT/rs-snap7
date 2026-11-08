# 连接与配置

在连接前创建配置。`ClientConfig::new(endpoint, rack, slot)` 接受已解析的
`SocketAddr`、0–7 的机架号及 0–31 的槽号。

```rust
use rs_snap7::{Client, ClientConfig};
use std::time::Duration;

let endpoint = "127.0.0.1:1102".parse().expect("valid endpoint");
let mut config = ClientConfig::new(endpoint, 0, 1)?;
config.transport.timeout = Duration::from_secs(3);
config.requested_pdu = 480;
config.validate()?;
let client = Client::connect(config).await?;
println!("Negotiated PDU: {}", client.pdu_length().await?);
```

## 配置项

| 字段 | 默认值 | 含义 |
| --- | --- | --- |
| `transport.timeout` | 5 秒 | 整个操作的截止时间，涵盖多步骤交换 |
| `requested_pdu` | 480 字节 | 请求值，对端可以协商为更小的 PDU |
| `transport.local_tsap` | `0x0100` | 主叫 TSAP |
| `transport.remote_tsap` | 由机架/槽号计算 | 被叫 TSAP 字节 |
| `transport.tpdu_exponent` | 10 | 请求的 COTP 帧大小指数，范围 7–13 |
| `transport.max_message_size` | 65535 字节 | 最大重组传输负载 |
| `recovery` | 关闭 | 有界读取恢复与可选心跳 |
| `rate_limit` | 关闭 | 单会话线路请求限流 |

显式 TSAP 通过 `config.transport` 修改。LOGO! VM 地址可使用 `Logo`。
可选网关路由由 `TransportConfig::routing` 配置，目前属于实验能力。

## 恢复与心跳

```rust
config.recovery.enabled = true;
config.recovery.max_retries = 3;
config.recovery.heartbeat_interval = Some(Duration::from_secs(10));
```

以上设置必须在 `Client::connect` **之前**完成。恢复可以在重连后重放已知只读初始请求，
不会重放修改操作或有状态的后续请求。心跳与应用操作共用会话锁。
显式调用 `disconnect()` 后不会在后台重新连接。

`is_connected()` 只报告本地状态，不会探测 PLC。
需要主动探测时，可调用 `get_cpu_state()` 等诊断读取。

## 限流与事件

```rust
config.rate_limit.max_per_second = 10.0;
config.operation_hook = rs_snap7::observability::OperationHook::new(|event| {
    eprintln!("{}: {:?}", event.operation, event.elapsed);
});
```

限流包含协商及后续请求，不按 COTP 分片计数。默认均匀安排请求，
可通过 `RateAlgorithm`、`RateBehavior` 选择令牌桶或立即拒绝。
回调应尽快返回。`operation_stats()` 不进行网络调用，返回逻辑操作完成数与错误统计。
失败处理参见[错误处理与限制](errors-and-limits.md)。
