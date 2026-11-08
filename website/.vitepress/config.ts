// Configure the bilingual site while keeping the documentation site isolated in website/.
import { defineConfig } from 'vitepress';

const repository = 'https://github.com/PenguinKingGT/rs-snap7';
const base = process.env.DOCS_BASE || '/';

if (!base.startsWith('/') || !base.endsWith('/')) {
    throw new Error('DOCS_BASE must start and end with /, for example /rs-snap7/.');
}

/** Build matching navigation for each locale without sharing untranslated labels. */
function navigation(chinese: boolean) {
    const prefix = chinese ? '/zh' : '';
    const link = (page: string) => `${prefix}/${page}`;
    return {
        nav: [
            { text: chinese ? '使用指南' : 'Guide', link: link('guide/getting-started') },
            { text: chinese ? 'API 参考' : 'API Reference', link: link('api/') },
        ],
        sidebar: [
            {
                text: chinese ? '使用指南' : 'User Guide',
                items: [
                    { text: chinese ? '快速开始' : 'Getting Started', link: link('guide/getting-started') },
                    { text: chinese ? '连接与配置' : 'Connection and Configuration', link: link('guide/configuration') },
                    { text: chinese ? '异步与同步' : 'Async and Blocking', link: link('guide/async-and-blocking') },
                    { text: chinese ? '读取与写入' : 'Reading and Writing', link: link('guide/read-write') },
                    { text: chinese ? '标签与 DB 快照' : 'Tags and DB Snapshots', link: link('guide/data-tools') },
                    { text: chinese ? '诊断与块操作' : 'Diagnostics and Blocks', link: link('guide/diagnostics') },
                    { text: chinese ? '可选功能与 CLI' : 'Optional Features and CLI', link: link('guide/optional-products') },
                    { text: chinese ? '错误处理与限制' : 'Errors and Limits', link: link('guide/errors-and-limits') },
                ],
            },
            {
                text: chinese ? 'API 参考' : 'API Reference',
                items: [
                    { text: chinese ? '接口索引' : 'API Overview', link: link('api/') },
                    { text: chinese ? '完整 Rust API（英文）' : 'Complete Rust API', link: '/rustdoc/rs_snap7/index.html', target: '_blank' },
                ],
            },
        ],
    };
}

export default defineConfig({
    title: 'rs-snap7',
    description: 'Async-first Siemens S7 communication in Rust.',
    base,
    lastUpdated: true,
    srcExclude: ['README*.md'],
    // Keep .html links compatible with ordinary static file hosting and Pages.
    cleanUrls: false,
    locales: {
        root: {
            label: 'English',
            lang: 'en-US',
            themeConfig: {
                ...navigation(false),
                editLink: {
                    pattern: `${repository}/edit/main/website/:path`,
                    text: 'Edit this page on GitHub',
                },
                footer: {
                    message: 'Released under the MIT License.',
                    copyright: 'rs-snap7 contributors',
                },
            },
        },
        zh: {
            label: '简体中文',
            lang: 'zh-CN',
            description: '以异步为核心的 Rust Siemens S7 通信库。',
            themeConfig: {
                ...navigation(true),
                outline: { label: '本页目录' },
                docFooter: { prev: '上一页', next: '下一页' },
                lastUpdated: { text: '最后更新于' },
                darkModeSwitchLabel: '外观',
                lightModeSwitchTitle: '切换到浅色模式',
                darkModeSwitchTitle: '切换到深色模式',
                sidebarMenuLabel: '菜单',
                returnToTopLabel: '返回顶部',
                langMenuLabel: '切换语言',
                editLink: {
                    pattern: `${repository}/edit/main/website/:path`,
                    text: '在 GitHub 上编辑此页',
                },
                footer: {
                    message: '基于 MIT 许可证发布。',
                    copyright: 'rs-snap7 贡献者',
                },
            },
        },
    },
    themeConfig: {
        logo: '/logo.svg',
        outline: [2, 3],
        socialLinks: [{ icon: 'github', link: repository }],
        search: {
            provider: 'local',
            options: {
                // Segment Chinese words as well as English terms in both index and query.
                miniSearch: {
                    options: {
                        tokenize: (text: string) => Array.from(
                            new Intl.Segmenter('zh', { granularity: 'word' }).segment(text),
                        ).filter((part) => part.isWordLike).map((part) => part.segment),
                    },
                },
                locales: {
                    zh: {
                        translations: {
                            button: { buttonText: '搜索文档', buttonAriaLabel: '搜索文档' },
                            modal: {
                                noResultsText: '没有找到相关结果',
                                resetButtonTitle: '清除搜索',
                                backButtonTitle: '关闭搜索',
                                displayDetails: '显示详细内容',
                                footer: { selectText: '选择', navigateText: '切换', closeText: '关闭' },
                            },
                        },
                    },
                },
            },
        },
    },
});
