<script setup>
import { withBase } from 'vitepress';
</script>

# 类型与地址 API

使用经过校验的地址构造器，避免手工拼接协议数值字段。长度和单位取决于内存区域与字长。

## 主要接口

| 接口 | 用途 |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/types/struct.Address.html')" target="_blank" rel="noopener">Address</a> | 校验区域、DB、起始位置与数量 |
| <a :href="withBase('/rustdoc/rs_snap7/types/enum.Area.html')" target="_blank" rel="noopener">Area</a> | DB、输入、输出、标记、计时器与计数器区域 |
| <a :href="withBase('/rustdoc/rs_snap7/types/enum.WordLen.html')" target="_blank" rel="noopener">WordLen</a> | 元素表示及字节宽度 |
| <a :href="withBase('/rustdoc/rs_snap7/types/enum.BlockType.html')" target="_blank" rel="noopener">BlockType</a> | S7 块类型标识 |

点击接口可查看从当前源码生成的签名、参数、错误约定及源码。

使用教程：[查看相关教程](../guide/read-write.md)。
