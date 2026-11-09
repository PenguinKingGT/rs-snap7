<script setup>
import { withBase } from 'vitepress';
</script>

# 设备发现 API

PROFINET DCP 通过以太网识别设备，独立于 S7 TCP 会话。内置原始以太网支持 Linux/macOS，应用也可以提供 FrameIo。

**Cargo 特性：** `discovery`。

## 主要接口

| 接口 | 用途 |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/discovery/index.html')" target="_blank" rel="noopener">discovery</a> | 发现全部设备或识别指定 MAC |
| <a :href="withBase('/rustdoc/rs_snap7/discovery/struct.Device.html')" target="_blank" rel="noopener">Device</a> | 设备发布的名称、地址与厂商元数据 |
| <a :href="withBase('/rustdoc/rs_snap7/discovery/struct.DiscoveryConfig.html')" target="_blank" rel="noopener">DiscoveryConfig</a> | 限制收集时间、帧数与设备数 |
| <a :href="withBase('/rustdoc/rs_snap7/discovery/trait.FrameIo.html')" target="_blank" rel="noopener">FrameIo</a> | 由应用提供的以太网适配接口 |

点击接口可查看从当前源码生成的签名、参数、错误约定及源码。

使用教程：[查看相关教程](../guide/optional-products.md)。
