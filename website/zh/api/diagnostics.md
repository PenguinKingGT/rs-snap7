<script setup>
import { withBase } from 'vitepress';
</script>

# 诊断与块模型 API

客户端服务方法返回这些结构化模型。各 SZL、控制服务及块传输是否可用，应在目标控制器上确认。

## 主要接口

| 接口 | 用途 |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/szl/index.html')" target="_blank" rel="noopener">szl</a> | CPU、CP、订货号、保护与诊断解析 |
| <a :href="withBase('/rustdoc/rs_snap7/types/enum.CpuState.html')" target="_blank" rel="noopener">CpuState</a> | 解码后的 CPU 状态 |
| <a :href="withBase('/rustdoc/rs_snap7/blocks/struct.BlockInfo.html')" target="_blank" rel="noopener">BlockInfo</a> | 校验后的远端块元信息 |
| <a :href="withBase('/rustdoc/rs_snap7/blocks/struct.CompactBlockInfo.html')" target="_blank" rel="noopener">CompactBlockInfo</a> | 离线紧凑头解析 |

点击接口可查看从当前源码生成的签名、参数、错误约定及源码。

使用教程：[查看相关教程](../guide/diagnostics.md)。
