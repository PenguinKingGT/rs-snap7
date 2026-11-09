<script setup>
import { withBase } from 'vitepress';
</script>

# 标签 API

标签组合地址语法、数据类型与数组长度，动态值保留符号及位宽。经典 S7 I/O 仍需要绝对地址。

## 主要接口

| 接口 | 用途 |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/tags/struct.Tag.html')" target="_blank" rel="noopener">Tag</a> | 解析地址并编解码经过校验的数值 |
| <a :href="withBase('/rustdoc/rs_snap7/tags/enum.Value.html')" target="_blank" rel="noopener">Value</a> | 类型化动态值与数组 |
| <a :href="withBase('/rustdoc/rs_snap7/tags/enum.DataType.html')" target="_blank" rel="noopener">DataType</a> | 标签支持的数据类型 |
| <a :href="withBase('/rustdoc/rs_snap7/tags/import/index.html')" target="_blank" rel="noopener">import</a> | CSV/JSON/TIA XML 文本导入，需要 tag-import |

点击接口可查看从当前源码生成的签名、参数、错误约定及源码。

使用教程：[查看相关教程](../guide/data-tools.md)。
