<script setup>
import { withBase } from 'vitepress';
</script>

# 主机指标 Demo API

将主机指标发布到只读 DB1，并在 DB2 提供可写控制区。该开发工具需要 Rust 1.95 及以上。

**Cargo 特性：** `demo`。

## 主要接口

| 接口 | 用途 |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/demo/struct.Demo.html')" target="_blank" rel="noopener">Demo</a> | 启动、采样与访问本地服务 |

点击接口可查看从当前源码生成的签名、参数、错误约定及源码。

使用教程：[查看相关教程](../guide/optional-products.md)。
