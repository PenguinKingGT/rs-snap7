# rs-snap7

[English](README.md) | 简体中文

将 `python-snap7` 迁移为 Rust 库，用于 Siemens S7 PLC 通信。

已实现的 28 项基线功能及进度见[迁移清单](docs/migration.md)。
模块职责与实现约定见[架构设计](docs/architecture.md)。
测试范围与剩余验收工作见[验证指南](docs/testing.md)。

默认异步客户端及可选同步封装已支持经典 S7 连接、原始及类型化内存读写、DB 辅助操作、多变量批量读写、连接恢复、限流、操作事件、PLC 诊断及控制，以及块目录、上传和下载。
尚未验证真实 PLC，范围限制见兼容性说明。包名为
`rs-snap7`，Rust 代码中的 crate 名为 `rs_snap7`，采用 Rust 2024 edition。
项目使用 MIT 许可证；上游署名见 `LICENSE` 与 `NOTICE`。发布暂未启用。

## 项目结构

- `src/lib.rs`：Rust 库入口。
- `python-snap7/`：本地 Python 参考实现、文档与测试，不纳入当前仓库的 Git 管理。
- `AGENTS.md`：贡献与代码质量规范。

模块按职责组织，集成测试放在 `tests/`，可运行示例放在
`examples/`。

## 本地开发

安装支持 Rust 2024 edition 的工具链，以及 `rustfmt` 和 `clippy` 组件。
受 sysinfo 依赖要求影响，启用全部功能或 demo 需要 Rust 1.95 及以上；本地使用 Rust 1.97.1 验证。
在仓库根目录执行：

```sh
cargo build
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo test --doc
```

新增功能须同步添加正常、边界及错误路径测试，
默认测试不依赖真实 PLC；修复缺陷须添加回归测试。公开接口需有文档注释，
复杂逻辑需解释设计原因，遵循 `AGENTS.md` 中的规范。

注释和 `AGENTS.md` 使用英文。默认 README 使用英文，中文版与英文版同步更新。

## 异步客户端示例

```rust,no_run
use rs_snap7::{Client, ClientConfig};

#[tokio::main(flavor = "current_thread")]
async fn main() -> rs_snap7::Result<()> {
    let endpoint = "192.168.1.10:102".parse().expect("valid PLC endpoint");
    let client = Client::connect(ClientConfig::new(endpoint, 0, 1)?).await?;
    let value = client.db_read_int(1, 0).await?;
    println!("DB1.DBW0 = {value}");
    client.disconnect().await
}
```

有意调整的接口和行为见[兼容性说明](docs/compatibility.md)。

## 可选同步接口

`Client`、`ClientPool` 和 `Logo` 默认使用 Tokio 异步核心，需要开启 I/O 和计时器。
启用 `blocking` 后可使用 `rs_snap7::blocking::{Client, ClientPool, Logo}`；
方法调用无需 `.await`，配置和结果类型保持一致，不维护第二套协议实现。

```toml
[dependencies]
rs-snap7 = { path = ".", features = ["blocking"] }
```

同步调用必须在未进入 Tokio 运行时的普通线程中执行；在 Tokio 任务、手动进入的
运行时上下文或 `spawn_blocking` 中会返回 `Error::BlockingInAsyncContext`。
所有同步客户端共享一个后台运行时线程，空闲时心跳仍继续执行；最后一个克隆释放连接
与任务，共享运行时保留至进程退出。同步 `is_connected()` 返回 `Result<bool>`。

同步前台操作的回调在调用线程退出运行时及连接锁后执行，可重入元数据查询。
后台回调使用有界队列（1024）和普通线程；队列满时丢弃新事件，计入
`operation_stats().background_hook_events_dropped`。回调应尽快返回。
DB 的异步行/字段操作使用 `read_row`、`write_row`、`read_fields`、`write_fields`；
同步调用使用对应的 `*_blocking` 方法。

取消已经发送的异步操作会使连接失效；被取消的写入可能已经在 PLC 上执行。
后续操作前应显式重新连接。示例见 `examples/read_db.rs` 和 `examples/blocking_read.rs`。

## 数据工具与可选产品

类型化标签支持 PLC4X/nodeS7 解析、数值校验和同步/异步读写。启用 `tag-import` 可导入 CSV/JSON/TIA XML 文本；符号元数据不代表支持符号 PLC 访问。

DB 工具支持布局校验、行快照、字段查询/导出与变更字节跟踪。同步和异步客户端均支持行/字段读取及选择性写入，并保留行间隔和只读前缀。

启用 `server` 可使用有界 S7 仿真器：可注册/锁定内存、独立会话、内存/块/时钟/控制服务、自定义 SZL 和事件回调。它仿真协议行为，不执行 PLC 程序。

启用 `partner` 可使用主/被动双向 PBC 传输，包含 R-ID、ACK 关联、有界队列、回调与统计。其 `send_async`/`receive_async` 适配器使用 Tokio 阻塞线程池；取消等待不会终止已开始的操作。

启用 `ppi` 可使用实验性 S7-200 单主串口访问（8E1）：SD1/SD2/SD3 帧校验、有界读取重试、S7 协商以及 V/区域读写。串口测试使用注入端口，无需硬件。

启用 `discovery` 可使用独立 PROFINET DCP 全部发现/定向识别，包含有界收集、去重和设备元数据。内置原始以太网适配器面向 Linux/macOS；其他系统可提供 `FrameIo`。

启用 `cli` 构建 `s7` 命令（read/write/dump/info/server/discover）；配合 `demo` 可将实时主机指标发布到只读 DB1，并提供可写控制区 DB2。参见[使用说明与示例](docs/usage.md)。
