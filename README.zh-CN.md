# rs-snap7

[English](README.md) | 简体中文

将 `python-snap7` 逐步迁移为 Rust 库，用于 Siemens S7 PLC 通信。

待实现功能与进度见[迁移清单](docs/migration.md)。
模块职责与实现约定见[架构设计](docs/architecture.md)。
测试范围与剩余验收工作见[验证指南](docs/testing.md)。

同步客户端及可选的原生异步客户端已支持经典 S7 连接、原始及类型化内存读写、DB 辅助操作、多变量批量读写、连接恢复、限流、操作事件、PLC 诊断及控制，以及块目录、上传和下载。
尚未验证真实 PLC，其他能力进度见迁移清单。包名为
`rs-snap7`，Rust 代码中的 crate 名为 `rs_snap7`，采用 Rust 2024 edition。
项目使用 MIT 许可证；上游署名见 `LICENSE` 与 `NOTICE`。发布暂未启用。

## 项目结构

- `src/lib.rs`：Rust 库入口。
- `python-snap7/`：本地 Python 参考实现、文档与测试，不纳入当前仓库的 Git 管理。
- `AGENTS.md`：贡献与代码质量规范。

功能开发时按职责引入模块，集成测试放在 `tests/`，可运行示例放在
`examples/`。

## 本地开发

安装支持 Rust 2024 edition 的工具链，以及 `rustfmt` 和 `clippy` 组件。
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

## 同步客户端示例

```rust,no_run
use rs_snap7::{Client, ClientConfig};

fn main() -> rs_snap7::Result<()> {
    let endpoint = "192.168.1.10:102".parse().expect("valid PLC endpoint");
    let client = Client::connect(ClientConfig::new(endpoint, 0, 1)?)?;
    let value = client.db_read_int(1, 0)?;
    println!("DB1.DBW0 = {value}");
    client.disconnect()
}
```

有意调整的接口和行为见[兼容性说明](docs/compatibility.md)。

## 原生异步支持

启用 Cargo feature `async` 后可使用 `AsyncClient`，运行于开启 I/O 和计时器的
Tokio 运行时。已支持的操作名称与 `Client` 对应，每次调用使用 `.await`。
使用以下命令验证此配置：

```sh
cargo test --all-targets --all-features
cargo test --doc --all-features
```

取消已经发送的操作会使连接失效。被取消的写入可能已经在 PLC 上执行，
后续操作前应显式重新连接。

类型化标签支持 PLC4X/nodeS7 解析、数值校验和同步/异步读写。启用 `tag-import` 可导入 CSV/JSON/TIA XML 文本；符号元数据不代表支持符号 PLC 访问。

DB 工具支持布局校验、行快照、字段查询/导出与变更字节跟踪。同步和异步客户端均支持行/字段读取及选择性写入，并保留行间隔和只读前缀。

启用 `server` 可使用有界 S7 仿真器：可注册/锁定内存、独立会话、内存/块/时钟/控制服务、自定义 SZL 和事件回调。它仿真协议行为，不执行 PLC 程序。

启用 `partner` 可使用主/被动双向 PBC 传输，包含 R-ID、ACK 关联、有界队列、回调与统计。配合 `async` 时收发适配器使用 Tokio 阻塞线程池；取消等待不会终止已开始的操作。

启用 `ppi` 可使用实验性 S7-200 单主串口访问（8E1）：SD1/SD2/SD3 帧校验、有界读取重试、S7 协商以及 V/区域读写。串口测试使用注入端口，无需硬件。
