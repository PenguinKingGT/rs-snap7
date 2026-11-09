<script setup>
import { withBase } from 'vitepress';
</script>

# 错误 API

匹配结构化错误以保留 PLC 错误码与已确认写入进度。Error 是非穷尽枚举，匹配时需要兜底分支。

## 主要接口

| 接口 | 用途 |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/error/enum.Error.html')" target="_blank" rel="noopener">Error</a> | 校验、通信、PLC 与不确定结果错误 |
| <a :href="withBase('/rustdoc/rs_snap7/error/type.Result.html')" target="_blank" rel="noopener">Result</a> | 库的结果类型别名 |
| <a :href="withBase('/rustdoc/rs_snap7/error/fn.item_error_text.html')" target="_blank" rel="noopener">item_error_text</a> | 解释 PLC 项目返回码 |

点击接口可查看从当前源码生成的签名、参数、错误约定及源码。

使用教程：[查看相关教程](../guide/errors-and-limits.md)。
