# 错误处理与限制

## 处理结构化错误

库方法返回 `rs_snap7::Result<T>`。`Error` 为非穷尽枚举，匹配时需要兜底分支。

```rust
use rs_snap7::Error;

match client.db_write(1, 0, &[1, 2, 3]).await {
    Ok(()) => println!("acknowledged"),
    Err(Error::PartialWrite { confirmed, source }) => {
        eprintln!("{confirmed} bytes confirmed; next range failed: {source}");
    }
    Err(Error::AmbiguousWrite { confirmed, uncertain, source }) => {
        eprintln!("{confirmed} confirmed, {uncertain} uncertain: {source}");
    }
    Err(error) => eprintln!("{error}"),
}
```

| 错误 | 含义与处理 |
| --- | --- |
| `InvalidInput` | 修正地址、长度、数值或配置 |
| `Plc`、`Item`、`Authentication` | 检查保留的 PLC 错误码与服务权限 |
| `RateLimited`、`DeadlineBeforeSend` | 请求在发送前被拒绝 |
| `Timeout`、`Io`、`Protocol`、引用错误 | 检查连接状态，交换失败可能已使 socket 失效 |
| `PartialWrite` | 先前字节已确认，后续区间明确失败 |
| `AmbiguousWrite`、`AmbiguousOperation` | 修改可能已执行，重试前应确认远端状态 |
| `BlockingInAsyncContext` | 在 Tokio 中改用异步接口 |

## 恢复与取消

默认不自动恢复。开启后允许有界重连，并对符合条件的只读初始请求重放一次。
有状态的后续请求、原始 `iso_exchange` 和修改操作不会自动重放。
重连协商出更小 PDU 时，会重新规划尚未完成的读取。

释放已发送操作的 future 会关闭该连接；被取消的修改操作仍可能已在 PLC 上执行。
取消后继续访问前，应显式调用 `reconnect()`，并在必要时确认远端结果。

## 支持范围

- 使用经典 S7 绝对地址，标签元数据不代表支持符号访问。
- STRING 默认 Latin-1，可显式使用 UTF-8；WSTRING 写入仅支持 BMP。
- S5TIME 只读；日历模型不带时区。
- DB 快照需要显式布局偏移，不自动发现 PLC 布局。
- `force_bit` 修改过程映像位，不建立持久 CPU 强制状态。
- PPI、网关路由和部分服务布局仍属实验性或尚未经过硬件验证。
  仿真器只模拟协议行为，不执行 PLC 程序。

`0.1.0` 已发布到 crates.io，目前仍属于开发版本。
真实 PLC、PPI 串口与 DCP 网络验收仍待执行，软件测试不构成型号或固件兼容性承诺。
