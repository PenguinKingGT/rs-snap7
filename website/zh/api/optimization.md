<script setup>
import { withBase } from 'vitepress';
</script>

# 连接池、策略与事件 API

使用独立会话进行有界并发读取。限流与恢复在连接前配置，逻辑操作统计无需网络 I/O。

## 主要接口

| 接口 | 用途 |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/client/struct.ClientPool.html')" target="_blank" rel="noopener">ClientPool</a> | 有界并发读取，使用 1–32 个独立会话 |
| <a :href="withBase('/rustdoc/rs_snap7/policy/struct.RateLimit.html')" target="_blank" rel="noopener">RateLimit</a> | 固定间隔或令牌桶限流 |
| <a :href="withBase('/rustdoc/rs_snap7/policy/struct.Recovery.html')" target="_blank" rel="noopener">Recovery</a> | 读取恢复、退避与心跳 |
| <a :href="withBase('/rustdoc/rs_snap7/observability/struct.OperationHook.html')" target="_blank" rel="noopener">OperationHook</a> | 逻辑操作回调 |
| <a :href="withBase('/rustdoc/rs_snap7/observability/struct.OperationStats.html')" target="_blank" rel="noopener">OperationStats</a> | 完成、失败与回调统计 |

点击接口可查看从当前源码生成的签名、参数、错误约定及源码。

使用教程：[查看相关教程](../guide/multi-variable.md)。
