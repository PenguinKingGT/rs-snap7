<script setup>
import { withBase } from 'vitepress';
</script>

# LOGO! API

LOGO! 将经过校验的 VM 地址转换为 DB1 操作。TSAP 应以控制器配置为准，VW/VD 为有符号数值。

## 主要接口

| 接口 | 用途 |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/logo/struct.Logo.html')" target="_blank" rel="noopener">Logo</a> | 异步 VM 读写与显式 TSAP 连接 |
| <a :href="withBase('/rustdoc/rs_snap7/logo/struct.VmAddress.html')" target="_blank" rel="noopener">VmAddress</a> | 解析 V10、VW20、VD30 与 V10.3 |
| <a :href="withBase('/rustdoc/rs_snap7/blocking/struct.Logo.html')" target="_blank" rel="noopener">blocking::Logo</a> | 可选的同步 VM 操作 |

点击接口可查看从当前源码生成的签名、参数、错误约定及源码。

使用教程：[查看相关教程](../guide/optional-products.md)。
