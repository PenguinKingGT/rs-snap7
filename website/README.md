# rs-snap7 Documentation Website

English | [简体中文](README.zh-CN.md)

A standalone VitePress user manual in English and Simplified Chinese. The site
lives entirely in `website/`; it does not import the internal design/migration
documents in `../docs/` or change the Rust library's configuration.

## Requirements

- Node.js 22 or newer and npm.
- The repository's Rust toolchain (Rust 1.95+ for all-feature API generation).
- A checkout containing the library source and Cargo.lock.

The toolchain pins VitePress `2.0.0-alpha.21`, the current `next` preview line.
Keep the lockfile when updating this prerelease toolchain.

## Develop and build

From the repository root:

```sh
cd website
npm ci
npm run docs:dev
```

Open the URL printed by VitePress. The default locale is English at `/`;
Simplified Chinese is at `/zh/`. Guides have matching paths so the language
switch keeps the current page. Local full-text search supports both languages.

```sh
npm run docs:build
npm run docs:preview
```

The build generates complete Rustdoc for all Cargo features before building the
site. All generated output stays in `website/.vitepress/` and
`website/public/rustdoc/`, which are ignored by Git. Rustdoc content follows the
English source comments; the website provides bilingual API guidance.
The final static site is `website/.vitepress/dist/` and includes the complete API.
`npm run docs:api` refreshes only Rustdoc; it does not contact PLC hardware.

## Test the production site

```sh
npx playwright install chromium
npm run docs:build
npm run docs:test
```

Browser tests cover both languages, matching-page language switching, Chinese
and English search, generated API pages, and mobile navigation. On Linux CI,
use `npx playwright install --with-deps chromium` to install browser prerequisites.
Use `DOCS_BROWSER_CHANNEL=chrome npm run docs:test` to test with an installed
Google Chrome instead of the downloaded Chromium. Rust unit tests are independent
of this site's Node dependencies.

## Deploy to static hosting

For hosting at a domain root, deploy `.vitepress/dist/` after a normal build.
For a repository subpath such as GitHub Pages, pass the same base to build and
preview/tests:

```sh
DOCS_BASE=/rs-snap7/ npm run docs:build
DOCS_BASE=/rs-snap7/ npm run docs:test
DOCS_BASE=/rs-snap7/ npm run docs:preview
```

A custom domain usually uses `/`. The base must start and end with `/`.
No hosting account or repository Pages settings are changed by these commands.
See the official [VitePress deployment guide](https://vitepress.dev/guide/deploy)
for hosting configuration.

## Maintain content

- English tutorials: `guide/`; Chinese tutorials: `zh/guide/`.
- API overview: `api/index.md` and `zh/api/index.md`.
- Add matching locale pages and localized navigation in `.vitepress/config.ts`.
- Keep runnable snippets and described behavior consistent with the Rust API.
- Comments and configuration explanations stay in English.
- Broken internal links fail production builds; do not disable that check.
- Commit package-lock.json and use `npm ci` for reproducible installs.
