# 多变量读取与连接池

`read_multi_vars` 一次读取多个绝对地址并保留逐项结果。
每次调用最多接受 20 个地址，PDU 限制可能使其拆成多次交换。
批量操作不是 PLC 的原子快照。

```rust
use rs_snap7::{Address, codec};

let addresses = [Address::db(1, 0, 2)?, Address::db(1, 4, 4)?];
let mut values = client.read_multi_vars(&addresses).await?.into_iter();
let count_bytes = values.next().expect("first result")?;
let speed_bytes = values.next().expect("second result")?;
let count = codec::read::<i16>(&count_bytes, 0)?;
let speed = codec::read::<f32>(&speed_bytes, 0)?;
```

外层 `Result` 表示整个操作的失败，每个返回项还有自己的 `Result`。
即使外层成功，也应处理某项的 PLC 错误。结果顺序与输入地址顺序相同。

## 跨会话并发读取

`Client` 的克隆共享一个会话。`ClientPool` 需要 1–32 条**独立**连接，
重复加入同一会话的克隆会被拒绝。

```rust
use rs_snap7::{Client, ClientPool};

let first = Client::connect(config.clone()).await?;
let second = Client::connect(config).await?;
let pool = ClientPool::new(vec![first.clone(), second.clone()])?;
let values = pool.read_multi_vars(&addresses).await?;
first.disconnect().await?;
second.disconnect().await?;
```

独立读取可以并发执行，但不保证来自同一 PLC 扫描周期。
取消连接池 future 会终止活动任务，已发送请求的连接可能因此失效。

本库的 `read_tags` 逐个读取类型化标签。需要优化报文调度时，使用原始批量读取或连接池。
Python 的[多变量教程](https://python-snap7.readthedocs.io/en/latest/multi-variable.html)
展示了相同任务；Rust 使用自有结果，应用无需管理 ctypes 缓冲区。
