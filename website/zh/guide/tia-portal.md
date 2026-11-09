# TIA Portal 配置

S7-1200/1500 的经典 S7 访问需要适当的 CPU 权限和具有绝对偏移的 DB。
请结合[上游配置指南](https://python-snap7.readthedocs.io/en/latest/tia-portal-config.html)
及对应 CPU/固件手册操作，不同 TIA Portal 版本的菜单名称可能不同。

## 准备专用 DB

1. 选择专门供应用访问的 DB。
2. 在属性中配置标准访问，使字段具有固定字节偏移。
3. 记录 DB 编号、字段类型和偏移，供 Rust 应用使用。

更改块访问方式可能重新初始化 DB 数据，应提前安排变更并保留所需数据。
转换步骤见 Siemens 的[块访问设置](https://docs.tia.siemens.cloud/r/en-us/v20/programming-basics/blocks-in-the-user-program/blocks-with-optimized-access/setting-up-block-access)。

## 启用并应用通信设置

在 CPU 属性的保护/安全连接设置中，按支持情况开启远端 PUT/GET。
编译并下载经过确认的 CPU 与 DB 变更。
先从已知测试区域读取少量数据，核对内容后再增加写入。
流程及访问影响见[上游配置步骤](https://python-snap7.readthedocs.io/en/latest/tia-portal-config.html)。

PUT/GET 会开放内存访问，应符合部署环境的访问策略。
符号访问与 S7CommPlus 不在本库的支持范围内。
