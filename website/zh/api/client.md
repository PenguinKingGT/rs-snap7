<script setup>
import { withBase } from 'vitepress';
</script>

# 客户端 API

连接后可克隆句柄共享串行会话，并使用独立连接实现并发。默认 Client 的网络方法需要 await。

## 主要接口

| 接口 | 用途 |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/client/struct.Client.html')" target="_blank" rel="noopener">Client</a> | 异步生命周期、内存、标签、诊断与控制 |
| <a :href="withBase('/rustdoc/rs_snap7/struct.ClientConfig.html')" target="_blank" rel="noopener">ClientConfig</a> | 连接地址、PDU、恢复、限流与回调配置 |
| <a :href="withBase('/rustdoc/rs_snap7/blocking/struct.Client.html')" target="_blank" rel="noopener">blocking::Client</a> | 同步封装，需要 blocking 特性 |
| <a :href="withBase('/rustdoc/rs_snap7/transport/struct.TransportConfig.html')" target="_blank" rel="noopener">TransportConfig</a> | 超时、显式 TSAP 与网关路由 |

点击接口可查看从当前源码生成的签名、参数、错误约定及源码。

使用教程：[查看相关教程](../guide/configuration.md)。
