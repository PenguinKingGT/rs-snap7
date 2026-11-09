# 仿真服务器

启用 `server` 即可在没有 PLC 硬件时验证客户端。
示例使用本地临时端口，注册 DB1 和标记区，并验证一次类型化写入。
在 rs-snap7 依赖中加入 `features = ["server"]`。

```rust
use rs_snap7::{Area, Client, ClientConfig, Result, server::{Server, ServerConfig}};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let mut server = Server::start(ServerConfig::new("127.0.0.1:0".parse().unwrap()))?;
    let db = server.register_area(Area::Db, 1, vec![0; 64])?;
    server.register_area(Area::Markers, 0, vec![0; 16])?;
    let client = Client::connect(ClientConfig::new(server.local_addr(), 0, 1)?).await?;
    client.db_write_int(1, 0, -42).await?;
    assert_eq!(client.db_read_int(1, 0).await?, -42);
    assert_eq!(&db.snapshot()?[..2], &(-42i16).to_be_bytes());
    client.disconnect().await?;
    tokio::task::spawn_blocking(move || server.stop()).await.expect("server shutdown");
    Ok(())
}
```

`Memory::with_mut` 可更新注册内存，`set_read_only(true)` 会拒绝客户端写入。
使用不同的 DB 编号调用 `register_area` 可以注册多个块。
非 DB 区域的 DB 编号为零，缓冲区由服务端持有并通过锁保护。

需要固定端口服务时，可用 `127.0.0.1:1102` 或
[CLI server 命令](optional-products.md)。Rust 服务端内部管理会话线程，
不需要 Python 风格的 `mainloop`。

任务结构参考[上游仿真服务器教程](https://python-snap7.readthedocs.io/en/latest/server.html)。
该仿真器不执行 PLC 程序，本地测试通过不代表已经验证真实 CPU 兼容性。
