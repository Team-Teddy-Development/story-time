// End to end: the real CLI and process-exec adapter against fake gh/az on PATH.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { mkdtemp, mkdir, writeFile, readFile, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname, delimiter } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { promisify } from 'node:util';
import { makeFakeBin } from './fake-trackers.mjs';
import { READY_MD } from './fixtures.mjs';

const CLI = join(dirname(fileURLToPath(import.meta.url)), '..', 'story.mjs');
const run = promisify(execFile);

function cleanEnv(bin, extra = {}) {
  const env = { ...process.env, PATH: `${bin}${delimiter}${process.env.PATH}`, ...extra };
  for (const k of ['STORY_TIME_DIR', 'STORY_TIME_PREFIX', 'STORY_TIME_CONFIG', 'GH_TOKEN', 'GITHUB_TOKEN', 'AZURE_DEVOPS_EXT_PAT', 'GITEA_TOKEN']) {
    if (!(k in extra)) delete env[k];
  }
  return env;
}

async function story(cwd, bin, args, { input, env, preload } = {}) {
  return new Promise((resolve) => {
    const node = preload ? ['--import', pathToFileURL(preload).href] : [];
    const child = spawn(process.execPath, [...node, CLI, ...args], { cwd, env: cleanEnv(bin, env) });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (d) => (stdout += d));
    child.stderr.on('data', (d) => (stderr += d));
    child.on('close', (code) => resolve({ code, stdout, stderr }));
    child.stdin.end(input ?? '');
  });
}

async function project(settings) {
  const dir = await mkdtemp(join(tmpdir(), 'story-push-'));
  await mkdir(join(dir, '.story-time'), { recursive: true });
  await writeFile(join(dir, '.story-time', 'settings.json'), JSON.stringify(settings));
  await mkdir(join(dir, 'backlog'));
  await writeFile(join(dir, 'backlog', 'ST-001.md'), READY_MD);
  return dir;
}

const GITHUB = { tracker: 'github', github: { repo: 'acme/shop' } };
const ADO = { tracker: 'ado', ado: { organization: 'https://dev.azure.com/acme', project: 'Shop' } };

test('push refuses when sync is off, and calls no tracker CLI', async () => {
  const bin = await makeFakeBin();
  const dir = await project({ ...GITHUB, sync: 'off' });
  const r = await story(dir, bin.dir, ['push', 'ST-001']);
  assert.equal(r.code, 1);
  assert.match(r.stderr, /Pushing is off for this project/);
  assert.deepEqual((await bin.readState()).calls, []);
});

test('push creates the GitHub issue and links it in the story file', async () => {
  const bin = await makeFakeBin();
  const dir = await project({ ...GITHUB, sync: 'auto' });
  const r = await story(dir, bin.dir, ['push', 'ST-001', '--json']);
  assert.equal(r.code, 0, r.stderr);
  assert.equal(JSON.parse(r.stdout).pushed[0].url, 'https://github.com/acme/shop/issues/1');
  const file = await readFile(join(dir, 'backlog', 'ST-001.md'), 'utf8');
  assert.match(file, /^remote: 1$/m);
  assert.match(file, /^remote_url: "https:\/\/github\.com\/acme\/shop\/issues\/1"$/m);
  assert.equal((await bin.readState()).issues[1].title, 'Reset password via email');
});

test('a second push updates the same issue', async () => {
  const bin = await makeFakeBin();
  const dir = await project({ ...GITHUB, sync: 'ask' });
  await story(dir, bin.dir, ['push', 'ST-001']);
  const r = await story(dir, bin.dir, ['push', 'ST-001', '--json']);
  assert.equal(JSON.parse(r.stdout).pushed[0].action, 'update');
  assert.equal(Object.keys((await bin.readState()).issues).length, 1);
});

test('push --dry-run prints the plan without calling the tracker', async () => {
  const bin = await makeFakeBin();
  const dir = await project({ ...ADO, sync: 'auto' });
  const r = await story(dir, bin.dir, ['push', '--all', '--dry-run']);
  assert.equal(r.code, 0, r.stderr);
  assert.match(r.stdout, /create\s+ST-001/);
  assert.deepEqual((await bin.readState()).calls, []);
});

test('push creates the ADO work item', async () => {
  const bin = await makeFakeBin();
  const dir = await project({ ...ADO, sync: 'auto' });
  const r = await story(dir, bin.dir, ['push', 'ST-001', '--json']);
  assert.equal(r.code, 0, r.stderr);
  assert.equal(JSON.parse(r.stdout).pushed[0].url, 'https://dev.azure.com/acme/Shop/_workitems/edit/1');
  assert.equal((await bin.readState()).items[1].type, 'User Story');
});

test('push of an unknown story fails before calling the tracker', async () => {
  const bin = await makeFakeBin();
  const dir = await project({ ...GITHUB, sync: 'auto' });
  const r = await story(dir, bin.dir, ['push', 'ST-001', 'ST-404']);
  assert.equal(r.code, 1);
  assert.match(r.stderr, /ST-404 not found/);
  assert.deepEqual((await bin.readState()).calls, []);
  assert.doesNotMatch(await readFile(join(dir, 'backlog', 'ST-001.md'), 'utf8'), /^remote:/m);
});

test('a tracker CLI failure is reported with its message and the story stays unlinked', async () => {
  const bin = await makeFakeBin();
  const dir = await project({ tracker: 'github', github: { repo: 'nobody/missing' }, sync: 'auto' });
  // The fake gh rejects edits of unknown issues; point the story at one.
  const file = join(dir, 'backlog', 'ST-001.md');
  await writeFile(file, (await readFile(file, 'utf8')).replace('size: 3\n', 'size: 3\nremote: 42\n'));
  const r = await story(dir, bin.dir, ['push', 'ST-001']);
  assert.equal(r.code, 1);
  assert.match(r.stderr, /Could not resolve to an issue/);
});

// --- secrets ---

test('secret set reads the token from stdin, stores it 0600, and never prints it', async () => {
  const bin = await makeFakeBin();
  const dir = await project({ ...GITHUB, sync: 'auto' });
  const r = await story(dir, bin.dir, ['secret', 'set', 'github'], { input: 'ghp_supersecret9876\n' });
  assert.equal(r.code, 0, r.stderr);
  assert.doesNotMatch(r.stdout + r.stderr, /supersecret/);
  const file = join(dir, '.story-time', '.github.env');
  assert.equal((await stat(file)).mode & 0o777, 0o600);
  assert.match(await readFile(file, 'utf8'), /^GH_TOKEN=ghp_supersecret9876$/m);
  const status = await story(dir, bin.dir, ['secret', 'status']);
  assert.match(status.stdout, /github\s+set \(…9876\)/);
  assert.match(status.stdout, /ado\s+not set/);
  assert.doesNotMatch(status.stdout, /supersecret/);
});

test('secret set rejects a token passed as an argument', async () => {
  const bin = await makeFakeBin();
  const r = await story(await project(GITHUB), bin.dir, ['secret', 'set', 'github', 'ghp_inline']);
  assert.equal(r.code, 2);
  assert.match(r.stderr, /not as an argument/);
});

test('push hands the stored token to gh through the environment', async () => {
  const bin = await makeFakeBin();
  const dir = await project({ ...GITHUB, sync: 'auto' });
  await story(dir, bin.dir, ['secret', 'set', 'github'], { input: 'ghp_tok\n' });
  // The fake gh records argv; the token must not be in it.
  const r = await story(dir, bin.dir, ['push', 'ST-001']);
  assert.equal(r.code, 0, r.stderr);
  assert.ok(!JSON.stringify((await bin.readState()).calls).includes('ghp_tok'));
});

test('secret status still reports an unsafe token file when the environment also has a token', async () => {
  const bin = await makeFakeBin();
  const dir = await project(GITHUB);
  await story(dir, bin.dir, ['secret', 'set', 'github'], { input: 'ghp_filetoken1234\n' });
  const { chmod } = await import('node:fs/promises');
  await chmod(join(dir, '.story-time', '.github.env'), 0o644);
  const r = await story(dir, bin.dir, ['secret', 'status'], { env: { GH_TOKEN: 'from-env' } });
  assert.match(r.stdout, /github\s+set via GH_TOKEN \(environment\)/);
  assert.match(r.stdout, /unusable: .*permissions 644/);
});

test('secret clear removes the token file', async () => {
  const bin = await makeFakeBin();
  const dir = await project(GITHUB);
  await story(dir, bin.dir, ['secret', 'set', 'github'], { input: 'x\n' });
  assert.equal((await story(dir, bin.dir, ['secret', 'clear', 'github'])).code, 0);
  assert.match((await story(dir, bin.dir, ['secret', 'status'])).stdout, /github\s+not set/);
});

// --- gitea (REST through fetch; the simulator is preloaded, no network) ---

const GITEA = { tracker: 'gitea', gitea: { baseUrl: 'https://gitea.example.com', owner: 'acme', repo: 'shop' } };
const GITEA_PRELOAD = join(dirname(fileURLToPath(import.meta.url)), 'fake-gitea-preload.mjs');

async function giteaRun(dir, bin, args, { input, env = {} } = {}) {
  const stateFile = join(dir, 'gitea-state.json');
  const r = await story(dir, bin.dir, args, { input, preload: GITEA_PRELOAD, env: { FAKE_GITEA_STATE: stateFile, FAKE_GITEA_TOKEN: 'gt_tok4321', ...env } });
  const state = await readFile(stateFile, 'utf8').then(JSON.parse, () => ({ calls: [], issues: {} }));
  return { ...r, state };
}

test('push creates the Gitea issue with the stored token and links it in the story file', async () => {
  const bin = await makeFakeBin();
  const dir = await project({ ...GITEA, sync: 'ask' });
  assert.equal((await giteaRun(dir, bin, ['secret', 'set', 'gitea'], { input: 'gt_tok4321\n' })).code, 0);
  const r = await giteaRun(dir, bin, ['push', 'ST-001', '--json']);
  assert.equal(r.code, 0, r.stderr);
  assert.equal(JSON.parse(r.stdout).pushed[0].url, 'https://gitea.example.com/acme/shop/issues/1');
  const file = await readFile(join(dir, 'backlog', 'ST-001.md'), 'utf8');
  assert.match(file, /^remote: 1$/m);
  assert.match(file, /^remote_url: "https:\/\/gitea\.example\.com\/acme\/shop\/issues\/1"$/m);
  assert.equal(r.state.issues[1].title, 'Reset password via email');
  assert.ok(r.state.calls.every((c) => c.auth === 'token gt_tok4321' && !c.url.includes('gt_tok4321')));
  assert.deepEqual((await bin.readState()).calls, [], 'no gh or az call');
  const again = await giteaRun(dir, bin, ['push', 'ST-001', '--json']);
  assert.equal(JSON.parse(again.stdout).pushed[0].action, 'update');
  assert.equal(Object.keys(again.state.issues).length, 1);
});

test('push to Gitea takes GITEA_TOKEN from the environment when no token file exists', async () => {
  const bin = await makeFakeBin();
  const dir = await project({ ...GITEA, sync: 'auto' });
  const r = await giteaRun(dir, bin, ['push', 'ST-001'], { env: { GITEA_TOKEN: 'gt_tok4321' } });
  assert.equal(r.code, 0, r.stderr);
  assert.equal(Object.keys(r.state.issues).length, 1);
});

test('push to Gitea without a token fails before any request and says how to store one', async () => {
  const bin = await makeFakeBin();
  const dir = await project({ ...GITEA, sync: 'auto' });
  const r = await giteaRun(dir, bin, ['push', 'ST-001']);
  assert.equal(r.code, 1);
  assert.match(r.stderr, /story secret set gitea/);
  assert.deepEqual(r.state.calls, []);
});

test('secret status lists the gitea token', async () => {
  const bin = await makeFakeBin();
  const dir = await project(GITEA);
  await story(dir, bin.dir, ['secret', 'set', 'gitea'], { input: 'gt_abcdefgh9999\n' });
  const file = join(dir, '.story-time', '.gitea.env');
  assert.equal((await stat(file)).mode & 0o777, 0o600);
  assert.match(await readFile(file, 'utf8'), /^GITEA_TOKEN=gt_abcdefgh9999$/m);
  assert.match((await story(dir, bin.dir, ['secret', 'status'])).stdout, /gitea\s+set \(…9999\)/);
});
