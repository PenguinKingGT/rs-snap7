# rs-snap7 文档网站

[English](README.md) | 简体中文

独立的 VitePress 中英文使用手册，全部位于 `website/`。
网站不导入 `../docs/` 中的内部设计/迁移文档，也不修改 Rust 库配置。

## 环境要求

- Node.js 22 及以上与 npm。
- 项目所需的 Rust 工具链，生成全特性 API 需要 Rust 1.95 及以上。
- 包含库源码和 Cargo.lock 的完整仓库。

工具链固定使用 VitePress `2.0.0-alpha.21`，即当前 `next` 预览版本线。
升级该预发布工具链时应同步维护锁文件。

## 开发与构建

从仓库根目录执行：

```sh
cd website
npm ci
npm run docs:dev
```

打开 VitePress 输出的地址。默认英文位于 `/`，简体中文位于 `/zh/`。
两种语言的指南路径对应，切换语言会保留当前页面，全文搜索支持中英文。

```sh
npm run docs:build
npm run docs:preview
```

构建先生成全部 Cargo 特性的 Rustdoc，再生成网站。
产物全部保存在 `website/.vitepress/` 和 `website/public/rustdoc/`，已排除 Git 跟踪。
Rustdoc 正文使用源码中的英文注释，网站提供双语 API 使用说明。
最终静态产物位于 `website/.vitepress/dist/`，包含完整 API。
`npm run docs:api` 仅刷新 Rustdoc，不访问 PLC 硬件。

## 测试生产产物

```sh
npx playwright install chromium
npm run docs:build
npm run docs:test
```

浏览器测试覆盖双语页面、同页语言切换、中英文搜索、生成的 API 和手机导航。
Linux CI 可用 `npx playwright install --with-deps chromium` 安装浏览器依赖。
已安装 Google Chrome 时，也可用 `DOCS_BROWSER_CHANNEL=chrome npm run docs:test`。
Rust 单元测试不依赖网站的 Node 环境。

## 部署静态网站

部署到域名根路径时，正常构建后发布 `.vitepress/dist/`。
部署到 GitHub Pages 等子路径时，构建、预览和测试传入相同前缀：

```sh
DOCS_BASE=/rs-snap7/ npm run docs:build
DOCS_BASE=/rs-snap7/ npm run docs:test
DOCS_BASE=/rs-snap7/ npm run docs:preview
```

自定义域名通常使用 `/`。前缀必须以 `/` 开始和结束。
这些命令不会修改托管账号或仓库的 Pages 设置。
托管配置参见 [VitePress 官方部署指南](https://vitepress.dev/guide/deploy)。

## 维护内容

- 英文教程位于 `guide/`，中文教程位于 `zh/guide/`。
- API 索引位于 `api/index.md` 与 `zh/api/index.md`。
- 新增页面时同时维护对应语言页面及 `.vitepress/config.ts` 导航。
- 示例和行为说明须与 Rust API 一致。
- 配置说明与代码注释使用英文。
- 内部链接失效会导致生产构建失败，不要关闭检查。
- 提交 package-lock.json，使用 `npm ci` 保证依赖安装可复现。

## 文档参考

教程与 API 分类参考 [python-snap7 官方文档](https://python-snap7.readthedocs.io/en/latest/)
按任务组织的方式。示例与接口以当前 Rust 实现为准，上游硬件结果与 Python 专属行为
不作为 Rust 版本的验收结论。每个教程/API 页面都有对应的英文路径；
分类参考链接到生成的 Rustdoc，浏览器检查覆盖双语页面一致性与 API 链接目标。
