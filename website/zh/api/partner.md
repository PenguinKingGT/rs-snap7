<script setup>
import { withBase } from 'vitepress';
</script>

# Partner API

Partner 使用独立的双向 PBC 会话，维护传输 ID、接收队列与生命周期；异步适配器使用 Tokio 阻塞线程池。

**Cargo 特性：** `partner`。

## 主要接口

| 接口 | 用途 |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/partner/struct.Partner.html')" target="_blank" rel="noopener">Partner</a> | 连接、收发、统计与停止 |
| <a :href="withBase('/rustdoc/rs_snap7/partner/struct.PartnerListener.html')" target="_blank" rel="noopener">PartnerListener</a> | 监听并接受被动连接 |
| <a :href="withBase('/rustdoc/rs_snap7/partner/struct.PartnerConfig.html')" target="_blank" rel="noopener">PartnerConfig</a> | 传输、队列与回调设置 |
| <a :href="withBase('/rustdoc/rs_snap7/partner/struct.Transfer.html')" target="_blank" rel="noopener">Transfer</a> | 接收的 R-ID 与数据负载 |

点击接口可查看从当前源码生成的签名、参数、错误约定及源码。

使用教程：[查看相关教程](../guide/optional-products.md)。
