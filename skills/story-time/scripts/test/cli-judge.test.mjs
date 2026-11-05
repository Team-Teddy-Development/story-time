// End to end through the real CLI. The Jev path runs with a fetch stub
// preloaded via --import, so no test ever reaches the network.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { READY_MD } from './fixtures.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const CLI = join(HERE, '..', 'story.mjs');
const STUB = pathToFileURL(join(HERE, 'fetch-stub.mjs')).href;

function story(cwd, args, { env = {}, input = '', stub = false } = {}) {
  const base = { ...process.env };
  for (const k of ['TYPESAFE_API_KEY', 'STORY_TIME_JEV', 'STORY_TIME_DIR', 'STORY_TIME_CONFIG']) delete base[k];
  const argv = stub ? ['--import', STUB, CLI, ...args] : [CLI, ...args];
  return new Promise((resolve) => {
    const child = spawn(process.execPath, argv, { cwd, env: { ...base, ...env } });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (d) => (stdout += d));
    child.stderr.on('data', (d) => (stderr += d));
    child.on('close', (code) => resolve({ code, stdout, stderr }));
    child.stdin.end(input);
  });
}

async function project() {
  const dir = await mkdtemp(join(tmpdir(), 'story-judge-'));
  await mkdir(join(dir, 'backlog'));
  await writeFile(join(dir, 'backlog', 'ST-001.md'), READY_MD);
  return dir;
}

test('without a key, judge is code driven and says so', async () => {
  const r = await story(await project(), ['judge', 'ST-001']);
  assert.equal(r.code, 0, r.stderr);
  assert.match(r.stdout, /engine: code \(no TypeSafe key/);
  assert.match(r.stdout, /need, not solution\s+yes/);
});

test('judge --json returns structured decisions with their source', async () => {
  const r = await story(await project(), ['judge', 'ST-001', '--json']);
  const [j] = JSON.parse(r.stdout).judgements;
  assert.equal(j.source, 'code');
  assert.equal(j.decisions.type.value, 'story');
});

test('with a stored key, judge is Jev driven', async () => {
  const dir = await project();
  const set = await story(dir, ['secret', 'set', 'jev'], { input: 'tsk_live_0123456789abcdef0123456789abcdef\n' });
  assert.equal(set.code, 0, set.stderr);
  const r = await story(dir, ['judge', 'ST-001', '--json'], { stub: true });
  assert.equal(r.code, 0, r.stderr);
  const out = JSON.parse(r.stdout);
  assert.deepEqual([out.engine.mode, out.engine.model], ['jev', 'jev-1.13.0']);
  assert.equal(out.judgements[0].source, 'jev');
});

test('TYPESAFE_API_KEY in the environment also selects Jev, and STORY_TIME_JEV=off overrides it', async () => {
  const dir = await project();
  const env = { TYPESAFE_API_KEY: 'tsk_live_0123456789abcdef0123456789abcdef' };
  assert.match((await story(dir, ['judge', 'ST-001'], { env, stub: true })).stdout, /engine: jev \(jev-1\.13\.0\)/);
  assert.match((await story(dir, ['judge', 'ST-001'], { env: { ...env, STORY_TIME_JEV: 'off' } })).stdout, /engine: code \(disabled by STORY_TIME_JEV/);
});

test('an outage is reported per story and the code result is still shown', async () => {
  const r = await story(await project(), ['judge', 'ST-001'], {
    env: { TYPESAFE_API_KEY: 'tsk_live_0123456789abcdef0123456789abcdef', FAKE_JEV_STATUS: '503' },
    stub: true,
  });
  assert.equal(r.code, 0, r.stderr);
  assert.match(r.stdout, /Jev unavailable \(overloaded\); code heuristics used/);
});

test('secret status lists the Jev key without revealing it', async () => {
  const dir = await project();
  await story(dir, ['secret', 'set', 'jev'], { input: 'tsk_live_0123456789abcdef0123456789abcdef\n' });
  const r = await story(dir, ['secret', 'status']);
  assert.match(r.stdout, /jev\s+set \(…cdef\)/);
  assert.doesNotMatch(r.stdout, /0123456789abcdef0123/);
});
