import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseStory, serializeStory, acceptanceCriteria, appendChangelog } from '../src/domain/story.mjs';
import { READY_MD } from './fixtures.mjs';

test('parseStory reads scalar, numeric and array frontmatter', () => {
  const s = parseStory(READY_MD);
  assert.equal(s.id, 'ST-001');
  assert.equal(s.title, 'Reset password via email');
  assert.equal(s.value, 8);
  assert.equal(s.size, 3);
  assert.deepEqual(s.depends_on, []);
});

test('parseStory splits body into H2 sections, preserving order', () => {
  const s = parseStory(READY_MD);
  assert.deepEqual(Object.keys(s.sections), ['Story', 'Acceptance Criteria', 'Notes', 'Out of Scope', 'Changelog']);
  assert.match(s.sections.Story, /^As a registered user/);
});

test('parseStory handles inline arrays and quoted strings', () => {
  const s = parseStory('---\nid: ST-2\ntitle: "Colon: inside"\ndepends_on: [ST-1, ST-3]\n---\n');
  assert.equal(s.title, 'Colon: inside');
  assert.deepEqual(s.depends_on, ['ST-1', 'ST-3']);
});

test('parseStory rejects documents without frontmatter', () => {
  assert.throws(() => parseStory('# just markdown'), /frontmatter/);
});

test('serializeStory round-trips', () => {
  const s = parseStory(READY_MD);
  assert.deepEqual(parseStory(serializeStory(s)), s);
});

test('serializeStory quotes strings that would otherwise be misread', () => {
  const out = serializeStory({ id: 'ST-9', title: 'Fix: login', sections: {} });
  assert.match(out, /title: "Fix: login"/);
});

test('acceptanceCriteria returns one entry per list item', () => {
  const acs = acceptanceCriteria(parseStory(READY_MD));
  assert.equal(acs.length, 2);
  assert.match(acs[0].text, /^Given a registered email/);
  assert.equal(acs[0].done, false);
});

test('appendChangelog adds a dated entry and bumps updated without mutating input', () => {
  const s = parseStory(READY_MD);
  const next = appendChangelog(s, 'split into ST-2, ST-3', '2026-11-04');
  assert.equal(next.updated, '2026-11-04');
  assert.match(next.sections.Changelog, /- 2026-11-04 split into ST-2, ST-3$/);
  assert.equal(s.updated, '2026-11-02');
});
