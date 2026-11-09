// Check navigation, locale-specific search, generated API access, and mobile reading.
import { expect, test } from '@playwright/test';
import { readdirSync } from 'node:fs';

const base = process.env.DOCS_BASE || '/';
// Discover published Markdown pages so new tutorial/API categories cannot skip checks.
const sections = ['guide', 'api'];
const pages = sections.flatMap((section) => readdirSync(new URL(`../${section}/`, import.meta.url))
    .filter((file) => file.endsWith('.md'))
    .map((file) => `${section}/${file.replace(/\.md$/, '.html')}`));

test('language switch retains the current tutorial and localizes navigation', async ({ page }) => {
    await page.goto('guide/getting-started.html');
    await expect(page.getByRole('heading', { level: 1, name: /^Getting Started/ })).toBeVisible();
    await page.getByRole('button', { name: 'Change language' }).click();
    await page.locator('.VPNavBarTranslations').getByRole('link', { name: '简体中文' }).click();
    await expect(page).toHaveURL(new URL(`${base}zh/guide/getting-started.html`, page.url()).href);
    await expect(page.locator('html')).toHaveAttribute('lang', 'zh-CN');
    await expect(page.getByRole('heading', { level: 1, name: /^快速开始/ })).toBeVisible();
    await expect(page.locator('.VPSidebar').getByRole('link', { name: '读取与写入' })).toBeVisible();
    await page.getByRole('button', { name: '切换语言' }).click();
    await page.locator('.VPNavBarTranslations').getByRole('link', { name: 'English' }).click();
    await expect(page.locator('html')).toHaveAttribute('lang', 'en-US');
});

for (const locale of ['', 'zh/']) {
    test(`all ${locale ? 'Chinese' : 'English'} tutorials and API overview render`, async ({ page }) => {
        const errors: string[] = [];
        page.on('pageerror', (error) => errors.push(error.message));
        for (const route of ['index.html', ...pages]) {
            const response = await page.goto(`${locale}${route}`);
            expect(response?.status()).toBe(200);
            await expect(page.locator('h1').first()).toBeVisible();
            await expect(page.locator('html')).toHaveAttribute('lang', locale ? 'zh-CN' : 'en-US');
            await expect(page.locator('.VPNavBarTitle img')).toBeVisible();
        }
        expect(errors).toEqual([]);
    });
}

test('English search finds an error-handling tutorial', async ({ page }) => {
    await page.goto('index.html');
    await page.getByRole('button', { name: 'Search', exact: true }).click();
    await page.locator('#localsearch-input').fill('AmbiguousWrite');
    const result = page.locator('#localsearch-list a').filter({ hasText: 'Errors and Limits' }).first();
    await expect(result).toBeVisible();
    await result.click();
    await expect(page).toHaveURL(/guide\/errors-and-limits\.html/);
});

test('Chinese search segments words and stays in the Chinese locale', async ({ page }) => {
    await page.goto('zh/index.html');
    await page.getByRole('button', { name: '搜索文档', exact: true }).click();
    await page.locator('#localsearch-input').fill('心跳');
    const result = page.locator('#localsearch-list a').first();
    await expect(result).toBeVisible();
    await expect(result).toHaveAttribute('href', new RegExp(`${base}zh/`));
    await result.click();
    await expect(page.locator('html')).toHaveAttribute('lang', 'zh-CN');
});

test('the API overview opens generated Rustdoc with real method signatures', async ({ page, context }) => {
    await page.goto('api/index.html');
    const popup = context.waitForEvent('page');
    await page.getByRole('link', { name: 'Async Client', exact: true }).click();
    const api = await popup;
    await api.waitForLoadState();
    await expect(api).toHaveURL(new RegExp(`${base}rustdoc/rs_snap7/client/struct.Client.html`));
    await expect(api.locator('[id="method.db_read"]')).toBeVisible();
    await expect(api.locator('[id="method.db_read"] .code-header')).toContainText('size: usize');
    const response = await api.goto(`${api.url().split('/rustdoc/')[0]}/rustdoc/rs_snap7/blocking/struct.Client.html`);
    expect(response?.status()).toBe(200);
    await expect(api.locator('[id="method.connect"] .code-header')).not.toContainText('async');
});

test('mobile navigation opens and long code blocks do not widen the viewport', async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto('zh/guide/read-write.html');
    await page.getByRole('button', { name: '菜单', exact: true }).click();
    await expect(page.locator('.VPSidebar').getByRole('link', { name: '标签与 DB 快照' })).toBeVisible();
    await page.locator('.VPSidebar').getByRole('link', { name: '标签与 DB 快照' }).click();
    await expect(page.getByRole('heading', { level: 1, name: /^标签与 DB 快照/ })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});


test('new tutorial and API categories exist in both languages', async () => {
    for (const section of sections) {
        const english = readdirSync(new URL(`../${section}/`, import.meta.url)).filter((file) => file.endsWith('.md')).sort();
        const chinese = readdirSync(new URL(`../zh/${section}/`, import.meta.url)).filter((file) => file.endsWith('.md')).sort();
        expect(chinese).toEqual(english);
    }
});

test('categorized API links resolve to generated documentation', async ({ page, request }) => {
    const targets = new Set<string>();
    for (const route of pages.filter((route) => route.startsWith('api/'))) {
        await page.goto(route);
        const links = await page.locator('.vp-doc a[href*="/rustdoc/"]').evaluateAll((anchors) =>
            anchors.map((anchor) => (anchor as HTMLAnchorElement).href));
        expect(links.length).toBeGreaterThan(0);
        for (const link of links) targets.add(link);
    }
    for (const target of targets) {
        const response = await request.get(target);
        expect(response.status(), target).toBe(200);
        expect(await response.text(), target).toContain('rustdoc');
    }
});


test('favicon resolves from the deployment base on both locale routes', async ({ page, request }) => {
    for (const route of ['index.html', 'zh/api/index.html']) {
        await page.goto(route);
        const icon = page.locator('head link[rel="icon"]');
        await expect(icon).toHaveCount(1);
        await expect(icon).toHaveAttribute('type', 'image/svg+xml');
        await expect(icon).toHaveAttribute('href', `${base}logo.svg`);
        const href = await icon.getAttribute('href');
        const response = await request.get(new URL(href!, page.url()).href);
        expect(response.status()).toBe(200);
        expect(response.headers()['content-type']).toContain('image/svg+xml');
        expect(await response.text()).toContain('<svg');
    }
});
