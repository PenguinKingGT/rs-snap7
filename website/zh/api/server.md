<script setup>
import { withBase } from 'vitepress';
</script>

# 仿真器 API

注册内存，在本地验证客户端行为。仿真器提供支持的服务，不执行 PLC 程序。

**Cargo 特性：** `server`。

## 主要接口

| 接口 | 用途 |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/server/struct.Server.html')" target="_blank" rel="noopener">Server</a> | 启动、注册区域、读取事件与停止 |
| <a :href="withBase('/rustdoc/rs_snap7/server/struct.ServerConfig.html')" target="_blank" rel="noopener">ServerConfig</a> | 监听地址、会话限制与 PDU 设置 |
| <a :href="withBase('/rustdoc/rs_snap7/server/struct.Memory.html')" target="_blank" rel="noopener">Memory</a> | 快照、修改与保护注册内存 |
| <a :href="withBase('/rustdoc/rs_snap7/server/struct.Event.html')" target="_blank" rel="noopener">Event</a> | 结构化仿真事件 |

点击接口可查看从当前源码生成的签名、参数、错误约定及源码。

使用教程：[查看相关教程](../guide/server.md)。
