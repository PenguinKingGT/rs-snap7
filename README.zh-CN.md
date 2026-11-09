# rs-snap7

[![crates.io](https://img.shields.io/crates/v/rs-snap7.svg)](https://crates.io/crates/rs-snap7)
[![API documentation](https://docs.rs/rs-snap7/badge.svg)](https://docs.rs/rs-snap7)
[![Rust 1.95+](https://img.shields.io/badge/rust-1.95%2B-orange.svg)](Cargo.toml)
[![MIT license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

[English](README.md) | 简体中文

以异步为核心的经典 Siemens S7 通信 Rust 库，迁移自
[python-snap7](https://github.com/gijzelaerr/python-snap7)。默认使用原生 Tokio I/O，
也可按需启用复用同一执行核心的同步封装。

[使用指南](website/zh/guide/getting-started.md) · [API 参考](https://docs.rs/rs-snap7) · [示例](examples) · [变更记录](CHANGELOG.md)

## 安装

需要 **Rust 1.95+**。在应用中添加库和 Tokio 运行时：

```toml
[dependencies]
rs-snap7 = "0.1.0"
tokio = { version = "1", features = ["macros", "rt", "net", "time"] }
```

Cargo 包名为 `rs-snap7`，Rust 导入时使用 `rs_snap7`。

## 快速开始

示例读取 DB1 字节偏移 0 处的 16 位有符号整数，连接下文的本地仿真器。
使用硬件时，请替换为控制器实际的地址、机架号和槽号。

```rust
use rs_snap7::{Client, ClientConfig, Result};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let endpoint = "127.0.0.1:1102".parse().expect("valid endpoint");
    let client = Client::connect(ClientConfig::new(endpoint, 0, 1)?).await?;
    let value = client.db_read_int(1, 0).await?;
    println!("DB1.DBW0 = {value}");
    client.disconnect().await
}
```

### 无需 PLC 即可体验

安装可选 CLI，启动包含 DB1 的仿真器：

```sh
cargo install rs-snap7 --version 0.1.0 --features cli --locked
s7 server --bind 127.0.0.1:1102
```

保持服务运行，在另一个终端写入并读取数值：

```sh
s7 write --port 1102 DB1:0:INT -42
s7 read --port 1102 DB1:0:INT
```

读取命令和 Rust 示例都会输出 `-42`。使用 Ctrl-C 停止仿真器。
在应用中嵌入仿真器的方法见[服务器教程](website/zh/guide/server.md)。

## 主要能力

- 原始 DB/区域访问、类型化标量与数组、字符串及保留相邻位的更新。
- 多变量读写、读取规划和独立连接池。
- PLC 诊断、SZL、CPU/时钟服务、块传输与会话密码。
- PLC4X/nodeS7 标签、DB 布局、自有快照及按变更区间选择性写入。
- 可配置超时、有界读取恢复、心跳、限流与事件回调。
- 可选仿真器、LOGO!、Partner/PBC、串口 PPI、DCP 发现及 CLI/demo 工具。

## 同步访问

同步应用可以启用 `blocking`：

```toml
[dependencies]
rs-snap7 = { version = "0.1.0", features = ["blocking"] }
```

```rust
use rs_snap7::{blocking::Client, ClientConfig, Result};

fn main() -> Result<()> {
    let endpoint = "127.0.0.1:1102".parse().expect("valid endpoint");
    let client = Client::connect(ClientConfig::new(endpoint, 0, 1)?)?;
    println!("DB1.DBW0 = {}", client.db_read_int(1, 0)?);
    client.disconnect()
}
```

同步客户端共用一个后台运行时，应从未进入 Tokio 运行时的普通线程调用。
在 Tokio 上下文中嵌套使用，包括 `spawn_blocking`，会返回 `Error::BlockingInAsyncContext`。
Tokio 应用应使用异步接口。生命周期、回调、连接池和 DB 辅助操作参见
[异步与同步](website/zh/guide/async-and-blocking.md)。

## Cargo 特性

异步 S7、编解码、标签、DB 工具及异步 LOGO! 无需额外特性，以下特性可叠加启用：

| 特性 | 新增能力 |
| --- | --- |
| `blocking` | 同步客户端、读取连接池与 LOGO! 适配器 |
| `tag-import` | CSV、JSON 和 TIA XML 元数据导入 |
| `server` | 注册内存、模拟协议服务的本地 S7 仿真器 |
| `partner` | 主动/被动 Partner/PBC 会话；异步适配器使用阻塞线程池 |
| `ppi` | 实验性 S7-200 单主串口访问（8E1） |
| `discovery` | PROFINET DCP；内置原始以太网支持 Linux/macOS |
| `cli` | `s7` 的 read/write/dump/info/server/discover 命令；启用 server/discovery |
| `demo` | 只读 DB1 主机指标与 DB2 可写控制区 |

## 范围与状态

`0.1.0` 为首个开发版本，后续 `0.x` 次版本可能调整 API。
当前支持经典 S7 绝对地址，S7CommPlus 和符号 PLC I/O 不在支持范围内。
仿真器模拟协议服务，不执行 PLC 程序。

真实 PLC、串口 PPI 和 DCP 网络验收仍待执行。
具体服务能否使用取决于 CPU 型号、固件和访问设置，接入前请阅读
[PLC 连接指南](website/zh/guide/plc-connections.md)和
[TIA Portal 配置](website/zh/guide/tia-portal.md)。

克隆共享串行会话，并行 I/O 需要独立连接。取消已发送的交换会关闭连接。
写入/控制操作可能具有不确定的远端结果，重试前应检查结构化错误。
详见[兼容性说明](docs/compatibility.md)与[故障排查](website/zh/guide/troubleshooting.md)。

## 参与开发

克隆仓库后执行：

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --doc --all-features
```

常规软件测试使用本地对端，无需 PLC；硬件和 Python 验收需要显式启用。
请遵循 [AGENTS.md](AGENTS.md)，贡献者资料见[验证指南](docs/testing.md)、
[迁移清单](docs/migration.md)和[架构设计](docs/architecture.md)。
本地 `python-snap7/` 参考目录不纳入 Git 管理。
独立的[双语文档站](website/README.zh-CN.md)使用自己的 Node 依赖。

## 许可证

MIT。上游署名见 [LICENSE](LICENSE) 和 [NOTICE](NOTICE)。
发布范围见 [CHANGELOG.md](CHANGELOG.md)，维护者发布步骤见 [RELEASING.md](RELEASING.md)。
