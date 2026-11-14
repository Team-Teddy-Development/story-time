import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdir, readdir, readFile, writeFile, access } from 'node:fs/promises';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { planInstall, AGENTS } from '../scripts/install.mjs';
import { EXPECTED } from './expected-skills.mjs';
import { makeTempDir } from '../skills/story-time/scripts/test/temp-dir.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const INSTALL = join(ROOT, 'scripts', 'install.mjs');
const run = promisify(execFile);
const tmp = () => makeTempDir('story-install-');
const exists = (p) => access(p).then(() => true, () => false);

async function install(home, ...args) {
  try {
    const r = await run(process.execPath, [INSTALL, ...args], { env: { ...process.env, HOME: home, CODEX_HOME: '' } });
    return { code: 0, ...r };
  } catch (e) {
    return { code: e.code, stdout: e.stdout, stderr: e.stderr };
  }
}

test('planInstall resolves user and project targets for each agent', () => {
  const dest = (agent, scope) =>
    planInstall({ agents: [agent], scope, home: '/h', project: '/p', skills: ['spike'] })[0].dest;
  assert.equal(dest('codex', 'user'), '/h/.agents/skills/spike');
  assert.equal(dest('codex', 'project'), '/p/.agents/skills/spike');
  assert.equal(dest('claude', 'user'), '/h/.claude/skills/spike');
  assert.equal(dest('claude', 'project'), '/p/.claude/skills/spike');
});

test('planInstall honours codexHome for the legacy $CODEX_HOME/skills location', () => {
  const [p] = planInstall({ agents: ['codex'], scope: 'user', home: '/h', codexHome: '/ch', skills: ['spike'] });
  assert.equal(p.dest, '/ch/skills/spike');
});

test('planInstall rejects unknown agents and scopes', () => {
  assert.throws(() => planInstall({ agents: ['cursor'], scope: 'user', home: '/h', skills: [] }), /Unknown agent/);
  assert.throws(() => planInstall({ agents: ['codex'], scope: 'global', home: '/h', skills: [] }), /Unknown scope/);
  assert.deepEqual(Object.keys(AGENTS).sort(), ['claude', 'codex']);
});

test('installs every skill for Codex into ~/.agents/skills, with metadata and a runnable CLI', async () => {
  const home = await tmp();
  const r = await install(home, '--agent', 'codex');
  assert.equal(r.code, 0, r.stderr);
  const dir = join(home, '.agents', 'skills');
  assert.deepEqual((await readdir(dir)).sort(), [...EXPECTED].sort());
  assert.ok(await exists(join(dir, 'story-pick', 'agents', 'openai.yaml')));
  assert.ok(!(await exists(join(dir, 'story-time', 'scripts', 'test'))), 'tests are not shipped');
  const { stdout } = await run(process.execPath, [join(dir, 'story-time', 'scripts', 'story.mjs'), 'help']);
  assert.match(stdout, /story — agile backlog toolkit/);
});

test('--agent all installs for both Claude Code and Codex', async () => {
  const home = await tmp();
  assert.equal((await install(home, '--agent', 'all')).code, 0);
  assert.ok(await exists(join(home, '.claude', 'skills', 'story-create', 'SKILL.md')));
  assert.ok(await exists(join(home, '.agents', 'skills', 'story-create', 'SKILL.md')));
});

test('--scope project installs into the given project directory', async () => {
  const home = await tmp();
  const project = await tmp();
  assert.equal((await install(home, '--agent', 'codex', '--scope', 'project', '--project', project)).code, 0);
  assert.ok(await exists(join(project, '.agents', 'skills', 'spike', 'SKILL.md')));
});

test('refuses to overwrite an existing skill without --force, and changes nothing', async () => {
  const home = await tmp();
  const mine = join(home, '.agents', 'skills', 'spike');
  await mkdir(mine, { recursive: true });
  await writeFile(join(mine, 'SKILL.md'), 'my edits');
  const r = await install(home, '--agent', 'codex');
  assert.equal(r.code, 1);
  assert.match(r.stderr, /already exists[\s\S]*--force/);
  assert.deepEqual(await readdir(join(home, '.agents', 'skills')), ['spike'], 'nothing else was installed');
  assert.equal((await install(home, '--agent', 'codex', '--force')).code, 0);
  assert.notEqual(await readFile(join(mine, 'SKILL.md'), 'utf8'), 'my edits');
});

test('--dry-run prints the plan and writes nothing', async () => {
  const home = await tmp();
  const r = await install(home, '--agent', 'codex', '--dry-run');
  assert.equal(r.code, 0);
  assert.match(r.stdout, /\.agents\/skills\/story-time/);
  assert.ok(!(await exists(join(home, '.agents'))));
});

test('--uninstall removes only story-time skills', async () => {
  const home = await tmp();
  await install(home, '--agent', 'codex');
  await mkdir(join(home, '.agents', 'skills', 'someone-elses'), { recursive: true });
  assert.equal((await install(home, '--agent', 'codex', '--uninstall')).code, 0);
  assert.deepEqual(await readdir(join(home, '.agents', 'skills')), ['someone-elses']);
});

test('missing --agent is a usage error', async () => {
  const r = await install(await tmp());
  assert.equal(r.code, 2);
  assert.match(r.stderr, /--agent/);
});
