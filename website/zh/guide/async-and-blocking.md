# 异步与同步

## 默认异步接口

Tokio 应用使用 `rs_snap7::{Client, ClientPool, Logo}`，运行时需要启用 I/O 和计时器。
网络操作使用 `.await`；解析和数值转换仍为普通同步函数。

```rust
let client = rs_snap7::Client::connect(config).await?;
let ready = client.is_connected().await;
let bytes = client.db_read(1, 0, 16).await?;
client.disconnect().await?;
```

单条连接同时执行一个完整操作，独立会话可以并行运行。
最后一个句柄释放后，连接和心跳任务也会释放。
取消正在交换数据的操作会使 socket 失效，防止残留响应影响后续请求。

::: warning 不确定的写入结果
确认丢失或 future 被取消前，写入及控制请求可能已经到达 PLC。
应检查 `AmbiguousWrite` 和 `AmbiguousOperation`，不能静默重放修改操作。
:::

## 可选同步接口

在依赖中启用特性：

```toml
rs-snap7 = { version = "0.1.0", features = ["blocking"] }
```

通过 `rs_snap7::blocking` 使用相同的配置与结果类型：

```rust
use rs_snap7::{blocking::Client, ClientConfig, Result};

fn main() -> Result<()> {
    let endpoint = "127.0.0.1:1102".parse().expect("valid endpoint");
    let client = Client::connect(ClientConfig::new(endpoint, 0, 1)?)?;
    let value = client.db_read_int(1, 0)?;
    println!("{value}");
    assert!(client.is_connected()?);
    client.disconnect()
}
```

全部同步客户端共享一个惰性启动的 Tokio 后台工作线程，调用间隙心跳仍会执行。
运行时保留至进程退出；每个客户端的连接和任务仍会在最后一个克隆释放后清理。
同步封装不会为泛型标量结果额外要求 `Send + 'static`。

::: tip 从普通线程调用
同步网络方法会拒绝所有已经进入 Tokio 的上下文，包括 `spawn_blocking` 和手动进入的
运行时句柄，返回 `Error::BlockingInAsyncContext`。Tokio 应用应使用异步接口。
:::

## DB 快照与回调

| 操作 | 异步 | 同步 |
| --- | --- | --- |
| 连接状态 | `is_connected().await` → `bool` | `is_connected()?` → `bool` |
| DB 行 | `read_row`、`write_row` 配合 `.await` | `read_row_blocking`、`write_row_blocking` |
| 指定字段 | `read_fields`、`write_fields` 配合 `.await` | `read_fields_blocking`、`write_fields_blocking` |
| 连接池 | `rs_snap7::ClientPool` | `rs_snap7::blocking::ClientPool` |
| LOGO! | `rs_snap7::Logo` | `rs_snap7::blocking::Logo` |

两种 DB 接口均直接修改原始快照；后续变更区间写入失败时，已确认区间的进度仍会保留。
同步前台回调在调用线程退出运行时并释放会话锁后执行，允许重入元数据查询。
后台回调使用普通分发线程和容量为 1024 的队列；队列满时丢弃新事件，计入
`operation_stats().background_hook_events_dropped`。回调应尽快返回。

Server、Partner、PPI 和 DCP 保留各自的执行模型。
Partner 的 `send_async`、`receive_async` 使用 Tokio 阻塞线程池，
取消语义与原生 S7 客户端不同。
