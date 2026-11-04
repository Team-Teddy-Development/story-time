#!/usr/bin/env node
// Runs every *.test.mjs under the test roots with `node --test`.
// Files are listed explicitly because `node --test <glob>` only expands
// globs on Node 21+, and package.json promises Node 20.
import { spawnSync } from 'node:child_process';
import { readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const TEST_ROOTS = ['skills/story-time/scripts/test', 'test'];

const files = TEST_ROOTS.flatMap((dir) =>
  readdirSync(join(ROOT, dir))
    .filter((f) => f.endsWith('.test.mjs'))
    .sort()
    .map((f) => join(ROOT, dir, f)),
);

if (!files.length) {
  console.error('No test files found.');
  process.exit(1);
}

const { status } = spawnSync(process.execPath, ['--test', ...files, ...process.argv.slice(2)], { stdio: 'inherit' });
process.exit(status ?? 1);
