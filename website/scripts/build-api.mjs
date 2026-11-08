// Build API documentation into website-owned output; leave the library tree untouched.
import { spawnSync } from 'node:child_process';
import { cp, mkdir, rm } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const website = fileURLToPath(new URL('../', import.meta.url));
const target = path.join(website, '.vitepress/rustdoc');
const destination = path.join(website, 'public/rustdoc');
const result = spawnSync('cargo', [
    'doc',
    '--locked',
    '--manifest-path', path.join(website, '../Cargo.toml'),
    '--target-dir', target,
    '--no-deps',
    '--all-features',
], { cwd: website, stdio: 'inherit' });

if (result.error || result.status !== 0) {
    console.error('Rust API generation failed. Install the project Rust toolchain and retry.');
    process.exit(result.status || 1);
}

// Replace stale pages only after cargo doc succeeds.
await mkdir(path.dirname(destination), { recursive: true });
await rm(destination, { recursive: true, force: true });
await cp(path.join(target, 'doc'), destination, { recursive: true });
console.log('Complete Rust API generated in website/public/rustdoc/.');
