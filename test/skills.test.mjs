// Library lint: every skill is a valid Agent Skill and every cross-reference
// resolves. Prose is code here; this keeps it honest.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readdir, readFile, access } from 'node:fs/promises';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SKILLS = join(ROOT, 'skills');

export const EXPECTED = [
  'story-time',
  'principle-invest',
  'story-create',
  'story-edit',
  'story-split',
  'acceptance-criteria',
  'story-estimate',
  'story-ready',
  'backlog-groom',
  'story-pick',
  'story-map',
  'spike',
  'story-kickoff',
  'story-accept',
];

async function load(name) {
  const text = await readFile(join(SKILLS, name, 'SKILL.md'), 'utf8');
  const m = /^---\n([\s\S]*?)\n---\n([\s\S]*)$/.exec(text);
  assert.ok(m, `${name}: SKILL.md needs YAML frontmatter`);
  const fm = Object.fromEntries(
    m[1].split('\n').filter(Boolean).map((l) => {
      const i = l.indexOf(':');
      return [l.slice(0, i).trim(), l.slice(i + 1).trim().replace(/^"(.*)"$/, '$1')];
    }),
  );
  return { fm, body: m[2], text };
}

test('the library ships exactly the expected skills', async () => {
  const dirs = (await readdir(SKILLS, { withFileTypes: true })).filter((d) => d.isDirectory()).map((d) => d.name);
  assert.deepEqual(dirs.sort(), [...EXPECTED].sort());
});

for (const name of EXPECTED) {
  test(`${name}: frontmatter name matches directory and description is usable`, async () => {
    const { fm } = await load(name);
    assert.equal(fm.name, name);
    assert.ok(fm.description?.length >= 40, 'description must say what it does and when to use it');
    assert.ok(fm.description.length <= 1024, 'description must be <= 1024 chars');
    assert.match(fm.description, /\bUse when\b/, 'description must include a "Use when" trigger');
  });

  test(`${name}: relative links resolve`, async () => {
    const { text } = await load(name);
    const links = [...text.matchAll(/\]\((\.{1,2}\/[^)#\s]+)/g)].map((m) => m[1]);
    for (const link of links) {
      await access(join(SKILLS, name, link)).catch(() => assert.fail(`${name}: broken link ${link}`));
    }
  });

  test(`${name}: referenced skills exist`, async () => {
    const { body } = await load(name);
    const refs = [...body.matchAll(/\*\*([a-z][a-z-]+)\*\* skill/g)].map((m) => m[1]);
    for (const ref of refs) assert.ok(EXPECTED.includes(ref), `${name} references unknown skill ${ref}`);
  });

  test(`${name}: stays concise (<= 250 lines)`, async () => {
    const { text } = await load(name);
    assert.ok(text.split('\n').length <= 250);
  });
}

test('README lists every skill', async () => {
  const readme = await readFile(join(ROOT, 'README.md'), 'utf8');
  for (const name of EXPECTED) assert.match(readme, new RegExp(`\`${name}\``), `README missing ${name}`);
});

test('plugin manifest is valid and names the library story-time', async () => {
  const manifest = JSON.parse(await readFile(join(ROOT, '.claude-plugin', 'plugin.json'), 'utf8'));
  assert.equal(manifest.name, 'story-time');
  assert.ok(manifest.version);
});
