import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdtemp, mkdir, writeFile, readFile, access } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { READY_MD } from './fixtures.mjs';

const CLI = join(dirname(fileURLToPath(import.meta.url)), '..', 'story.mjs');
const run = promisify(execFile);
const tmp = () => mkdtemp(join(tmpdir(), 'story-cfg-'));
const exists = (p) => access(p).then(() => true, () => false);

// Runs the CLI from a project directory, the way an agent would.
async function story(cwd, ...args) {
  const env = { ...process.env };
  delete env.STORY_TIME_DIR;
  delete env.STORY_TIME_PREFIX;
  delete env.STORY_TIME_CONFIG;
  try {
    const r = await run(process.execPath, [CLI, ...args], { cwd, env });
    return { code: 0, ...r };
  } catch (e) {
    return { code: e.code, stdout: e.stdout, stderr: e.stderr };
  }
}

async function project(config) {
  const dir = await tmp();
  if (config) await writeFile(join(dir, '.story-time.json'), JSON.stringify(config));
  await mkdir(join(dir, config?.backlogDir ?? 'backlog'), { recursive: true });
  await writeFile(join(dir, config?.backlogDir ?? 'backlog', 'ST-001.md'), READY_MD);
  return dir;
}

test('export with no config prints a tracker-neutral markdown card', async () => {
  const r = await story(await project(), 'export', 'ST-001');
  assert.equal(r.code, 0, r.stderr);
  assert.match(r.stdout, /^## Reset password via email/);
});

test('export defaults to the configured tracker format', async () => {
  const r = await story(await project({ tracker: 'github', github: { repo: 'acme/shop' } }), 'export', 'ST-001');
  assert.match(r.stdout, /^\*\*Title:\*\* Reset password via email/);
});

test('export --format overrides the default; --command prints the create command', async () => {
  const dir = await project({ tracker: 'ado', ado: { organization: 'https://dev.azure.com/acme', project: 'Shop' } });
  assert.match((await story(dir, 'export', 'ST-001', '--format', 'github')).stdout, /^\*\*Title:\*\*/);
  assert.match((await story(dir, 'export', 'ST-001', '--command')).stdout, /^az boards work-item create/);
});

test('export --json returns the structured card', async () => {
  const r = await story(await project({ tracker: 'github', github: { repo: 'acme/shop' } }), 'export', 'ST-001', '--json');
  const card = JSON.parse(r.stdout);
  assert.equal(card.title, 'Reset password via email');
  assert.ok(Array.isArray(card.labels));
});

test('config backlogDir and prefix are honoured', async () => {
  const dir = await project({ backlogDir: 'docs/stories', prefix: 'PAY' });
  const r = await story(dir, 'new', '--title', 'Refunds', '--json');
  assert.equal(JSON.parse(r.stdout).id, 'PAY-001');
  assert.ok(await exists(join(dir, 'docs', 'stories', 'PAY-001.md')));
});

test('config prints the effective config and exits 1 on an invalid file', async () => {
  const ok = await story(await project({ tracker: 'github', github: { repo: 'acme/shop' } }), 'config', '--json');
  assert.equal(ok.code, 0, ok.stderr);
  assert.equal(JSON.parse(ok.stdout).github.repo, 'acme/shop');
  const bad = await story(await project({ tracker: 'trello' }), 'config');
  assert.equal(bad.code, 1);
  assert.match(bad.stderr, /\.story-time\.json.*Unknown tracker "trello"/s);
});

test('init writes a validated config from an answers file and refuses to overwrite', async () => {
  const dir = await tmp();
  const answers = join(dir, 'answers.json');
  await writeFile(answers, JSON.stringify({ tracker: 'github', github: { repo: 'acme/shop' }, team: { velocity: 21 } }));
  const r = await story(dir, 'init', '--from', answers);
  assert.equal(r.code, 0, r.stderr);
  const written = JSON.parse(await readFile(join(dir, '.story-time.json'), 'utf8'));
  assert.equal(written.tracker, 'github');
  assert.equal(written.team.velocity, 21);
  assert.equal(written.team.wipLimit, 1, 'defaults are written out so the file documents itself');
  assert.equal((await story(dir, 'init', '--from', answers)).code, 1);
  assert.equal((await story(dir, 'init', '--from', answers, '--force')).code, 0);
});

test('init rejects invalid answers without writing anything', async () => {
  const dir = await tmp();
  const answers = join(dir, 'answers.json');
  await writeFile(answers, JSON.stringify({ tracker: 'github' }));
  const r = await story(dir, 'init', '--from', answers);
  assert.equal(r.code, 1);
  assert.match(r.stderr, /github\.repo/);
  assert.ok(!(await exists(join(dir, '.story-time.json'))));
});

test('help lists export, init, and config', async () => {
  const r = await story(await tmp(), 'help');
  for (const cmd of ['export', 'init', 'config']) assert.match(r.stdout, new RegExp(`^  ${cmd}\\b`, 'm'));
});
