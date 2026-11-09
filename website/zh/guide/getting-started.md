# 快速开始

## 体验本地仿真器

克隆仓库，安装支持 Rust 2024 edition 的工具链。
启用全部特性或 demo 需要 Rust 1.95 及以上。在仓库根目录执行以下命令，无需 PLC 硬件：

```sh
git clone https://github.com/PenguinKingGT/rs-snap7.git
cd rs-snap7
cargo run --features cli --bin s7 -- server --bind 127.0.0.1:1102
```

保持服务运行，在另一个终端进入同一目录：

```sh
cargo run --features cli --bin s7 -- write --port 1102 DB1:0:INT -42
cargo run --features cli --bin s7 -- read --port 1102 DB1:0:INT
cargo run --example read_db -- 127.0.0.1:1102 DB1:0:INT
```

读取命令会输出 `-42`。使用 Ctrl-C 停止仿真器。

## 添加依赖

该 crate 尚未发布，可使用 Git 依赖或本地仓库路径。
应用需要可复现构建时，应固定 Git 提交版本。

```toml
[dependencies]
rs-snap7 = { git = "https://github.com/PenguinKingGT/rs-snap7.git" }
tokio = { version = "1", features = ["macros", "rt", "net", "time"] }
```

## 连接与读取

示例连接本地仿真器。连接硬件时，将地址、机架号和槽号替换为实际 PLC 配置。

```rust
use rs_snap7::{Client, ClientConfig, Result};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let endpoint = "127.0.0.1:1102".parse().expect("valid endpoint");
    let client = Client::connect(ClientConfig::new(endpoint, 0, 1)?).await?;
    client.db_write_int(1, 0, -42).await?;
    let value = client.db_read_int(1, 0).await?;
    println!("DB1.DBW0 = {value}");
    client.disconnect().await
}
```

`Client` 的克隆共享会话，完整操作会串行执行。
需要跨连接并发读取时，使用独立客户端或 `ClientPool`。

## 后续阅读

- [异步与同步](async-and-blocking.md)：选择执行方式。
- [使用与示例](optional-products.md)：特性、命令行、标签与可选产品。
- [API 参考](../api/index.md)：公开类型和本地 Rustdoc。
- [兼容性与限制](errors-and-limits.md)：PLC 行为与取消规则。

连接真实控制器前，请先阅读 [PLC 连接](plc-connections.md)和 [TIA Portal 配置](tia-portal.md)。
