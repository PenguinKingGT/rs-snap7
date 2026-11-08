# rs-snap7

[English](README.md) | 简体中文

将 `python-snap7` 逐步迁移为 Rust 库，用于 Siemens S7 PLC 通信。

待实现功能与进度见[迁移清单](docs/migration.md)。
模块职责与实现约定见[架构设计](docs/architecture.md)。

当前仅完成 Rust 工程初始化，尚未提供 PLC 通信功能。包名为
`rs-snap7`，Rust 代码中的 crate 名为 `rs_snap7`，采用 Rust 2024 edition。
发布暂未启用，许可证及上游署名将在迁移代码前明确。

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

目前没有功能实现及测试用例。新增功能须同步添加正常、边界及错误路径测试，
默认测试不依赖真实 PLC；修复缺陷须添加回归测试。公开接口需有文档注释，
复杂逻辑需解释设计原因，遵循 `AGENTS.md` 中的规范。

注释和 `AGENTS.md` 使用英文。默认 README 使用英文，中文版与英文版同步更新。
