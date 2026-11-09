# PLC 连接

rs-snap7 通过 ISO-on-TCP 使用经典 S7 绝对地址。这些是连接前提，
Rust 版本的真实硬件验收仍待执行。

## 选择连接方式

| 控制器系列 | 需要确认的连接方式 |
| --- | --- |
| S7-300 / S7-400 | 经典 S7 以太网，使用工程中的机架和 CPU 槽号 |
| S7-1200 / S7-1500 / ET 200SP CPU | 固件和配置允许时使用经典 PUT/GET 路径 |
| LOGO! 以太网 | `Logo`，使用工程指定的 TSAP |
| S7-200 串口 | 实验性 `ppi` 特性与 `PpiClient` |

上游 [PLC 支持说明](https://python-snap7.readthedocs.io/en/latest/plc-support.html)
列出了型号限制。其中的硬件实测结果属于 python-snap7，不能据此认定 Rust 版本通过验收。
成功连接也不代表 CPU 支持所有控制、SZL、密码或块传输服务。

## 机架、槽号与端口

上游示例通常采用 S7-300 `0/2`、S7-400 `0/3`、S7-1200/1500 `0/1`。
实际值应以工程布局为准。LOGO! 和串口 PPI 使用其他寻址方式。
参见[上游连接指南](https://python-snap7.readthedocs.io/en/latest/connecting.html)。

```rust
let endpoint = "192.168.1.10:102".parse().expect("valid endpoint");
let config = rs_snap7::ClientConfig::new(endpoint, 0, 1)?;
let client = rs_snap7::Client::connect(config).await?;
```

端口包含在 `SocketAddr` 中，本地仿真器使用 `:1102`。
后续参见 [TIA Portal 配置](tia-portal.md)和[连接配置](configuration.md)。
