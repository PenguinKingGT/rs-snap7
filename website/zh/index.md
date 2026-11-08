---
layout: home
hero:
  name: rs-snap7
  text: 使用 Rust 连接 Siemens S7
  tagline: 以异步为核心，按需提供同步接口，覆盖经典 S7 通信与开发工具。
  image:
    src: /logo.svg
    alt: rs-snap7
  actions:
    - theme: brand
      text: 快速开始
      link: /zh/guide/getting-started
    - theme: alt
      text: API 参考
      link: /zh/api/
features:
  - title: 统一异步核心
    details: 原生 Tokio I/O，支持会话串行化、取消、连接恢复和可选同步封装。
  - title: 校验数据访问
    details: 提供原始内存、类型化数值、位更新、标签、DB 布局及多变量操作。
  - title: 完整开发工具
    details: 本地仿真器、命令行工具、诊断服务和不依赖 PLC 硬件的软件测试。
---

## 项目状态

rs-snap7 是 python-snap7 的 Rust 迁移项目，28 项基线迁移功能均已实现。
当前仍处于开发阶段，尚未发布到 crates.io。
真实 PLC、PPI 串口和 DCP 网络验收仍待执行。

可以先使用[本地仿真器](guide/getting-started.md)体验，了解
[异步与同步接口](guide/async-and-blocking.md)，并在接入硬件前阅读
[支持范围与限制](guide/errors-and-limits.md)。
