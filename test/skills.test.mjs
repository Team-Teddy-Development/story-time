// Library lint: every skill is a valid Agent Skill and every cross-reference
// resolves. Prose is code here; this keeps it honest.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readdir, readFile, access } from 'node:fs/promises';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SKILLS = join(ROOT, 'skills');

import { EXPECTED } from './expected-skills.mjs';

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

// --- Codex ---

function parseOpenAiYaml(text) {
  // Two-level YAML subset used by agents/openai.yaml: section headers and quoted scalars.
  const out = {};
  let section = null;
  for (const line of text.split('\n')) {
    if (!line.trim() || line.trim().startsWith('#')) continue;
    const top = /^([a-z_]+):\s*$/.exec(line);
    if (top) {
      section = out[top[1]] = {};
      continue;
    }
    const kv = /^\s{2}([a-z_]+):\s*(.+)$/.exec(line);
    assert.ok(kv && section, `unsupported openai.yaml line: ${line}`);
    section[kv[1]] = kv[2] === 'true' ? true : kv[2] === 'false' ? false : JSON.parse(kv[2]);
  }
  return out;
}

for (const name of EXPECTED) {
  test(`${name}: has Codex agents/openai.yaml interface metadata`, async () => {
    const y = parseOpenAiYaml(await readFile(join(SKILLS, name, 'agents', 'openai.yaml'), 'utf8'));
    assert.ok(y.interface?.display_name, 'display_name required');
    const sd = y.interface.short_description;
    assert.ok(sd && sd.length >= 25 && sd.length <= 64, `short_description must be 25-64 chars, got ${sd?.length}`);
    assert.match(y.interface.default_prompt, new RegExp(`\\$${name}\\b`), 'default_prompt must mention $<skill>');
  });
}

test('principle-invest is explicit-only in Codex, matching disable-model-invocation', async () => {
  const y = parseOpenAiYaml(await readFile(join(SKILLS, 'principle-invest', 'agents', 'openai.yaml'), 'utf8'));
  assert.equal(y.policy?.allow_implicit_invocation, false);
});

test('Codex plugin manifest exists and matches the Claude manifest version', async () => {
  const codex = JSON.parse(await readFile(join(ROOT, '.codex-plugin', 'plugin.json'), 'utf8'));
  const claude = JSON.parse(await readFile(join(ROOT, '.claude-plugin', 'plugin.json'), 'utf8'));
  assert.equal(codex.name, 'story-time');
  assert.equal(codex.version, claude.version);
  assert.ok(codex.description);
});

test('README documents the Codex install', async () => {
  const readme = await readFile(join(ROOT, 'README.md'), 'utf8');
  assert.match(readme, /## Install[\s\S]*Codex/);
  assert.match(readme, /codex plugin marketplace add team-teddy-development\/story-time/);
  assert.match(readme, /--agent codex/);
});
