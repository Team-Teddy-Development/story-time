import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { READY_MD, ENROLLMENT, PROMOTION } from './fixtures.mjs';
import { makeTempDir } from './temp-dir.mjs';

const CLI = join(dirname(fileURLToPath(import.meta.url)), '..', 'story.mjs');
const run = promisify(execFile);

async function story(cwd, ...args) {
  const env = { ...process.env };
  delete env.STORY_TIME_DIR;
  delete env.STORY_TIME_CONFIG;
  try {
    const { stdout, stderr } = await run(process.execPath, [CLI, ...args], { cwd, env });
    return { code: 0, stdout, stderr };
  } catch (e) {
    return { code: e.code, stdout: e.stdout, stderr: e.stderr };
  }
}

async function project(files) {
  const root = await makeTempDir('story-design-');
  for (const [path, text] of Object.entries(files)) {
    await mkdir(dirname(join(root, path)), { recursive: true });
    await writeFile(join(root, path), text);
  }
  return root;
}

test('help lists the machine and table commands', async () => {
  const r = await story(await project({}), 'help');
  assert.match(r.stdout, /machine check\|criteria\|diagram <file>/);
  assert.match(r.stdout, /table\s+check\|criteria <file>/);
});

test('machine check passes a valid machine and exits 0', async () => {
  const root = await project({ 'model/state/enrollment.toml': ENROLLMENT });
  const r = await story(root, 'machine', 'check', 'model/state/enrollment.toml');
  assert.equal(r.code, 0, r.stderr);
  assert.equal(r.stdout.trim(), 'enrollment: OK (4 states, 4 events, 4 transitions)');
});

test('machine check exits 1 naming each defect', async () => {
  const broken = ENROLLMENT.replace('terminal = ["approved", "rejected"]', 'terminal = ["approved"]');
  const root = await project({ 'm.toml': broken });
  const r = await story(root, 'machine', 'check', 'm.toml');
  assert.equal(r.code, 1);
  assert.equal(r.stdout.trim(), 'enrollment: 1 defect\n  dead-end       State "rejected" is not terminal and has no outgoing transition.');
  const json = JSON.parse((await story(root, 'machine', 'check', 'm.toml', '--json')).stdout);
  assert.equal(json.ok, false);
  assert.deepEqual(json.defects.map((d) => d.code), ['dead-end']);
});

test('machine criteria prints one checkbox criterion per line', async () => {
  const root = await project({ 'enrollment.toml': ENROLLMENT });
  const r = await story(root, 'machine', 'criteria', 'enrollment.toml');
  assert.equal(r.code, 0, r.stderr);
  const lines = r.stdout.trim().split('\n');
  assert.equal(lines.length, 17);
  assert.equal(lines[0], '- [ ] Given an enrollment in draft and every required field is filled in, when submit is received, then the enrollment is in submitted.');
  assert.ok(lines.every((l) => /^- \[ \] Given .+, when .+, then .+\.$/.test(l)));
});

test('machine criteria refuses a machine with defects', async () => {
  const root = await project({ 'm.toml': ENROLLMENT.replace('to = "approved"\nevent = "approve"', 'to = "approved"') });
  const r = await story(root, 'machine', 'criteria', 'm.toml');
  assert.equal(r.code, 1);
  assert.match(r.stdout, /missing-event/);
});

test('machine diagram prints Mermaid', async () => {
  const root = await project({ 'enrollment.toml': ENROLLMENT });
  const r = await story(root, 'machine', 'diagram', 'enrollment.toml');
  assert.match(r.stdout, /^stateDiagram-v2\n {4}\[\*\] --> draft/);
});

test('a TOML syntax error names the file and line, and a missing file exits 1', async () => {
  const root = await project({ 'bad.toml': 'states = ["a"\n' });
  const r = await story(root, 'machine', 'check', 'bad.toml');
  assert.equal(r.code, 1);
  assert.match(r.stderr, /bad\.toml: line 1: unterminated array/);
  assert.equal((await story(root, 'table', 'check', 'nope.toml')).code, 1);
  assert.equal((await story(root, 'machine', 'frobnicate', 'x.toml')).code, 2);
  assert.equal((await story(root, 'table', 'check')).code, 2);
});

test('table check passes a complete table and fails listing gaps and overlaps', async () => {
  const gap = PROMOTION.replace(/\[\[rules\]\]\nwhen = \{ role = "guest"[^\n]*\nthen[^\n]*\n/, '');
  const root = await project({ 'promotion.toml': PROMOTION, 'gap.toml': gap });
  const ok = await story(root, 'table', 'check', 'promotion.toml');
  assert.equal(ok.code, 0, ok.stderr);
  assert.equal(ok.stdout.trim(), 'promotion: OK (2 inputs, 6 combinations, 4 rules, hit policy unique)');
  const bad = await story(root, 'table', 'check', 'gap.toml');
  assert.equal(bad.code, 1);
  assert.match(bad.stdout, /No rule covers role=guest, verified=true\./);
  assert.match(bad.stdout, /No rule covers role=guest, verified=false\./);
});

test('table criteria prints the rule table, and --json gives rows for a parameterized test', async () => {
  const root = await project({ 'promotion.toml': PROMOTION });
  const r = await story(root, 'table', 'criteria', 'promotion.toml');
  assert.equal(r.code, 0, r.stderr);
  assert.match(r.stdout, /^- \[ \] Given the inputs in a row of the promotion table, when the rule is evaluated, then the result is the outputs in that row\.\n\n {2}\| rule \| role \| verified \| promote \|/);
  const json = JSON.parse((await story(root, 'table', 'criteria', 'promotion.toml', '--json')).stdout);
  assert.deepEqual(json.rows[0], [1, 'admin', true, 'yes']);
});

test('lint errors on a story whose state_machine file is missing, relative to the repo root', async () => {
  const md = READY_MD.replace('depends_on: []', 'depends_on: []\nstate_machine: model/state/enrollment.toml');
  const root = await project({ 'backlog/ST-001.md': md });
  const r = await story(root, 'lint', '--json');
  assert.equal(r.code, 1);
  assert.ok(JSON.parse(r.stdout)[0].findings.some((f) => f.rule === 'state_machine.exists'));
  await mkdir(join(root, 'model', 'state'), { recursive: true });
  await writeFile(join(root, 'model', 'state', 'enrollment.toml'), ENROLLMENT);
  const ok = await story(root, 'lint');
  assert.equal(ok.code, 0, ok.stdout);
});

test('lint reports an open spike nobody depends on', async () => {
  const spikeMd = `---
id: ST-002
title: "Spike: provider X"
status: refined
type: spike
timebox: 1d
created: 2026-11-01
updated: 2026-11-01
---

## Question
Can provider X do 3DS?

## Exit Criteria
- [ ] Decision recorded
`;
  const root = await project({ 'backlog/ST-001.md': READY_MD, 'backlog/ST-002.md': spikeMd });
  const r = await story(root, 'lint', 'ST-002');
  assert.equal(r.code, 1);
  assert.match(r.stdout, /spike\.unblocks/);
  await writeFile(join(root, 'backlog', 'ST-001.md'), READY_MD.replace('depends_on: []', 'depends_on: [ST-002]'));
  assert.equal((await story(root, 'lint', 'ST-002')).code, 0);
});

test('criteria name the entity and the rule from the file, not its stem', async () => {
  const machine = ENROLLMENT.replace('entity = "enrollment"', 'name = "cutover"');
  const root = await project({ 'bad.toml': machine, 'stem.toml': PROMOTION });
  const m = await story(root, 'machine', 'criteria', 'bad.toml');
  assert.equal(m.code, 0, m.stderr);
  assert.match(m.stdout, /^- \[ \] Given a cutover in draft/);
  const t = await story(root, 'table', 'criteria', 'stem.toml');
  assert.match(t.stdout, /of the promotion table/);
});
