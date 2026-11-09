<script setup>
import { withBase } from 'vitepress';
</script>

# DB 快照 API

自有快照维护可编辑字节与已确认基线。行/字段操作更新同一快照，并逐区间记录成功写入的进度。

## 主要接口

| 接口 | 用途 |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/db/struct.Db.html')" target="_blank" rel="noopener">Db</a> | 行、字段、导出与变更跟踪 I/O |
| <a :href="withBase('/rustdoc/rs_snap7/db/struct.DbConfig.html')" target="_blank" rel="noopener">DbConfig</a> | 物理行位置、间隔与可写前缀 |
| <a :href="withBase('/rustdoc/rs_snap7/db/struct.Layout.html')" target="_blank" rel="noopener">Layout</a> | 解析精确的字节/位布局规格 |

点击接口可查看从当前源码生成的签名、参数、错误约定及源码。

使用教程：[查看相关教程](../guide/data-tools.md)。
