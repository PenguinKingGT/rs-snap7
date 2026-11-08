# 可选功能与 CLI

特性可叠加启用，异步 S7 访问无需额外特性。

| 特性 | 功能 |
| --- | --- |
| `blocking` | 同步客户端、连接池与 LOGO! 封装 |
| `tag-import` | CSV、JSON、TIA XML 文本导入 |
| `server` | 多线程 S7 仿真器，提供注册内存与支持的服务 |
| `partner` | 主动/被动双向 PBC 传输 |
| `ppi` | 实验性 S7-200 8E1 串口访问 |
| `discovery` | 独立的 PROFINET DCP 发现 |
| `cli` | `s7` 命令行，启用 server、discovery 及命令依赖 |
| `demo` | DB1 主机指标与 DB2 可写控制；需要 Rust 1.95+ |

## 命令行

在仓库根目录执行。一个终端启动仿真器，另一个终端执行其余命令：

```sh
cargo run --features cli --bin s7 -- server --bind 127.0.0.1:1102
cargo run --features cli --bin s7 -- write --port 1102 DB1:0:INT -42
cargo run --features cli --bin s7 -- read --port 1102 DB1:0:INT
cargo run --features cli --bin s7 -- dump --port 1102 --db 1 --length 16
cargo run --features cli --bin s7 -- info --port 1102
cargo run --features cli --bin s7 -- --help
```

`dump --output db.bin` 保存原始字节。数组和值使用引号保留 shell 语法：

```sh
cargo run --features cli --bin s7 -- write --port 1102 'DB1:4:REAL[2]' '[1.5,2.5]'
cargo run --features cli,demo --bin s7 -- demo --bind 127.0.0.1:1102 --duration-ms 1000
```

Demo 发布主机指标。仿真器不执行 PLC 程序。

## LOGO!、Partner、PPI 与 DCP

`Logo` 接受 `V10`、`VW20`、`VD30`、`V10.3` 等 VM 地址，连接时可指定 TSAP。
VW 和 VD 返回有符号数值。

Partner 具有独立的主动/被动会话、有界接收队列和 R-ID 关联。
异步收发适配器使用 Tokio 阻塞线程池，取消等待不会终止已开始的操作。

`PpiClient::open(path, 9600, PpiConfig::default())` 打开自有串口。
PPI 属于实验功能，尚未通过真实硬件验收。

DCP 发现与 TCP 访问相互独立。内置以太网适配器支持 Linux/macOS，
需要原始套接字/BPF 权限。请选择本地接口已有的 IP；其他系统可以实现 `FrameIo`。

```sh
cargo run --features cli --bin s7 -- discover --interface 192.168.1.100
```
