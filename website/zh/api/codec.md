<script setup>
import { withBase } from 'vitepress';
</script>

# 数值编解码 API

无需连接 PLC 即可解码本地字节缓冲区。偏移与长度经过校验，多字节数值按 PLC 字节序处理。

## 主要接口

| 接口 | 用途 |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/codec/index.html')" target="_blank" rel="noopener">codec</a> | 整数、浮点、位、字符与字符串转换 |
| <a :href="withBase('/rustdoc/rs_snap7/codec/trait.Scalar.html')" target="_blank" rel="noopener">Scalar</a> | 定宽标量校验转换 |
| <a :href="withBase('/rustdoc/rs_snap7/codec/fn.read.html')" target="_blank" rel="noopener">read / write</a> | 对字节切片进行泛型类型化访问 |
| <a :href="withBase('/rustdoc/rs_snap7/codec/datetime/index.html')" target="_blank" rel="noopener">datetime</a> | S7 日期时间模型与转换 |

点击接口可查看从当前源码生成的签名、参数、错误约定及源码。

使用教程：[查看相关教程](../guide/read-write.md)。
