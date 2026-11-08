# rs-snap7

[English](README.md) | 简体中文

将 `python-snap7` 逐步迁移为 Rust 库，用于 Siemens S7 PLC 通信。

待实现功能与进度见[迁移清单](docs/migration.md)。
模块职责与实现约定见[架构设计](docs/architecture.md)。

同步客户端已支持经典 S7 连接、原始及类型化内存读写、DB 辅助操作与多变量批量读写。
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
