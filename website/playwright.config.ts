// Exercise the built static site, including deployments beneath a repository subpath.
import { defineConfig, devices } from '@playwright/test';

const base = process.env.DOCS_BASE || '/';
const address = `http://127.0.0.1:4173${base}`;

export default defineConfig({
    testDir: './tests',
    fullyParallel: true,
    forbidOnly: Boolean(process.env.CI),
    retries: process.env.CI ? 1 : 0,
    reporter: 'list',
    use: {
        ...devices['Desktop Chrome'],
        viewport: { width: 1440, height: 900 },
        baseURL: address,
        channel: process.env.DOCS_BROWSER_CHANNEL,
        trace: 'retain-on-failure',
    },
    webServer: {
        command: 'npm run docs:preview -- --host 127.0.0.1 --port 4173 --strictPort',
        url: address,
        reuseExistingServer: false,
        timeout: 30_000,
    },
});
