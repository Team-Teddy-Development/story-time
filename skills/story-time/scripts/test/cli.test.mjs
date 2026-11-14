import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { writeFile, readdir } from 'node:fs/promises';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { READY_MD } from './fixtures.mjs';
import { makeTempDir } from './temp-dir.mjs';

const CLI = join(dirname(fileURLToPath(import.meta.url)), '..', 'story.mjs');
const run = promisify(execFile);

async function story(dir, ...args) {
  try {
    const { stdout, stderr } = await run(process.execPath, [CLI, ...args], { env: { ...process.env, STORY_TIME_DIR: dir } });
    return { code: 0, stdout, stderr };
  } catch (e) {
    return { code: e.code, stdout: e.stdout, stderr: e.stderr };
  }
}

const tempDir = () => makeTempDir('story-cli-');

test('help lists every command', async () => {
  const r = await story(await tempDir(), 'help');
  assert.equal(r.code, 0);
  for (const cmd of ['new', 'list', 'show', 'lint', 'rank', 'set', 'note', 'split', 'stats']) assert.match(r.stdout, new RegExp(`\\b${cmd}\\b`));
});

test('unknown command exits 2 with usage', async () => {
  const r = await story(await tempDir(), 'frobnicate');
  assert.equal(r.code, 2);
  assert.match(r.stderr, /Unknown command/);
});

test('new creates a story file and prints its id', async () => {
  const dir = await tempDir();
  const r = await story(dir, 'new', '--title', 'Export invoices', '--statement', 'As an accountant, I want CSV export, so that I can reconcile.', '--ac', 'Given invoices, when I export, then I get CSV', '--json');
  assert.equal(r.code, 0, r.stderr);
  assert.equal(JSON.parse(r.stdout).id, 'ST-001');
  assert.deepEqual(await readdir(dir), ['ST-001.md']);
});

test('lint exits 1 when any story has errors and 0 when all are ready', async () => {
  const dir = await tempDir();
  await writeFile(join(dir, 'ST-001.md'), READY_MD);
  assert.equal((await story(dir, 'lint')).code, 0);
  await story(dir, 'new', '--title', 'Half-baked');
  const r = await story(dir, 'lint', '--json');
  assert.equal(r.code, 1);
  const results = JSON.parse(r.stdout);
  assert.equal(results.find((x) => x.id === 'ST-002').ready, false);
});

test('rank prints ready stories by WSJF and explains exclusions', async () => {
  const dir = await tempDir();
  await writeFile(join(dir, 'ST-001.md'), READY_MD);
  await story(dir, 'new', '--title', 'Draft one');
  const r = await story(dir, 'rank');
  assert.equal(r.code, 0, r.stderr);
  assert.match(r.stdout, /1\s+ST-001\s+5\.33/);
  assert.match(r.stdout, /ST-002.*status draft/);
});

test('set updates fields and refuses an unready story moving to ready', async () => {
  const dir = await tempDir();
  await story(dir, 'new', '--title', 'Draft one');
  const bad = await story(dir, 'set', 'ST-001', 'status=ready');
  assert.equal(bad.code, 1);
  assert.match(bad.stderr, /not ready/);
  const ok = await story(dir, 'set', 'ST-001', 'size=3', '--note', 'poker', '--json');
  assert.equal(ok.code, 0, ok.stderr);
  assert.equal(JSON.parse(ok.stdout).size, 3);
});

test('split reads children from a JSON file and runs the saga', async () => {
  const dir = await tempDir();
  await writeFile(join(dir, 'ST-001.md'), READY_MD);
  const spec = join(await tempDir(), 'children.json');
  await writeFile(spec, JSON.stringify([{ title: 'a', statement: 'As a u, I want a, so that b.' }, { title: 'b', statement: 'As a u, I want b, so that c.' }]));
  const r = await story(dir, 'split', 'ST-001', '--from', spec, '--json');
  assert.equal(r.code, 0, r.stderr);
  assert.deepEqual(JSON.parse(r.stdout).childIds, ['ST-002', 'ST-003']);
  assert.match((await story(dir, 'show', 'ST-001')).stdout, /status: split/);
});

test('show of an unknown story exits 1', async () => {
  const r = await story(await tempDir(), 'show', 'ST-999');
  assert.equal(r.code, 1);
  assert.match(r.stderr, /not found/);
});

test('stats --json reports counts', async () => {
  const dir = await tempDir();
  await writeFile(join(dir, 'ST-001.md'), READY_MD);
  const r = await story(dir, 'stats', '--json');
  assert.equal(JSON.parse(r.stdout).readyPoints, 3);
});

test('note appends a changelog line', async () => {
  const dir = await tempDir();
  await writeFile(join(dir, 'ST-001.md'), READY_MD);
  const r = await story(dir, 'note', 'ST-001', 'reworded AC2 for clarity');
  assert.equal(r.code, 0, r.stderr);
  assert.match((await story(dir, 'show', 'ST-001')).stdout, /reworded AC2 for clarity/);
  assert.equal((await story(dir, 'note', 'ST-001')).code, 2);
});
