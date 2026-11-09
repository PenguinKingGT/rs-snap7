<script setup>
import { withBase } from 'vitepress';
</script>

# PPI API

实验性 S7-200 串口访问使用可变的单主客户端和按元素寻址的区域操作，与以太网机架/槽号连接相互独立。

**Cargo 特性：** `ppi`。

## 主要接口

| 接口 | 用途 |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/ppi/struct.PpiClient.html')" target="_blank" rel="noopener">PpiClient</a> | 打开/连接、区域与 V 内存读写 |
| <a :href="withBase('/rustdoc/rs_snap7/ppi/struct.PpiConfig.html')" target="_blank" rel="noopener">PpiConfig</a> | 站号、超时与重试设置 |
| <a :href="withBase('/rustdoc/rs_snap7/ppi/struct.PpiTransport.html')" target="_blank" rel="noopener">PpiTransport</a> | 通过可注入端口执行校验后的串行交换 |
| <a :href="withBase('/rustdoc/rs_snap7/ppi/enum.PpiArea.html')" target="_blank" rel="noopener">PpiArea</a> | 支持的串行内存区域 |

点击接口可查看从当前源码生成的签名、参数、错误约定及源码。

使用教程：[查看相关教程](../guide/optional-products.md)。
