import { test } from 'node:test';
import assert from 'node:assert/strict';
import { splitStory } from '../src/application/split-story.mjs';
import { InMemoryStoryRepository } from '../src/adapters/memory-story-repository.mjs';
import { story } from './fixtures.mjs';

const children = [
  { title: 'Save cart for signed-in shoppers', statement: 'As a shopper, I want my cart saved when signed in, so that I can resume later.', acceptance: ['Given items, when I sign back in, then they are there'], size: 2 },
  { title: 'Merge guest cart on sign-in', statement: 'As a guest, I want my cart kept when I sign in, so that I do not lose items.', acceptance: ['Given a guest cart, when I sign in, then items merge'], size: 3 },
];

async function seeded() {
  const repo = new InMemoryStoryRepository();
  await repo.save(story({ id: 'ST-001', size: 13, epic: 'EP-1', persona: 'shopper' }));
  return repo;
}

test('creates children linked to the parent and marks the parent split', async () => {
  const repo = await seeded();
  const r = await splitStory(repo, 'ST-001', children, { date: '2026-11-04' });
  assert.equal(r.ok, true);
  assert.deepEqual(r.childIds, ['ST-002', 'ST-003']);

  const parent = await repo.get('ST-001');
  assert.equal(parent.status, 'split');
  assert.deepEqual(parent.children, ['ST-002', 'ST-003']);
  assert.match(parent.sections.Changelog, /2026-11-04 split into ST-002, ST-003/);

  const c1 = await repo.get('ST-002');
  assert.equal(c1.parent, 'ST-001');
  assert.equal(c1.epic, 'EP-1', 'inherits epic');
  assert.equal(c1.status, 'draft', 'children need their own refinement');
  assert.equal(c1.size, 2);
  assert.match(c1.sections['Acceptance Criteria'], /^- \[ \] Given items/);
});

test('refuses to split a story that is done or already split', async () => {
  const repo = new InMemoryStoryRepository();
  await repo.save(story({ id: 'ST-001', status: 'done' }));
  const r = await splitStory(repo, 'ST-001', children);
  assert.equal(r.ok, false);
  assert.match(r.error.message, /cannot split .* done/i);
});

test('requires at least two children', async () => {
  const r = await splitStory(await seeded(), 'ST-001', [children[0]]);
  assert.equal(r.ok, false);
  assert.match(r.error.message, /at least two/);
});

test('compensates: a failure mid-split leaves the backlog exactly as it was', async () => {
  const repo = await seeded();
  const before = await repo.list();
  let saves = 0;
  const flaky = Object.create(repo);
  flaky.save = async (s) => {
    if (++saves === 2) throw new Error('disk full');
    return repo.save(s);
  };
  const r = await splitStory(flaky, 'ST-001', children);
  assert.equal(r.ok, false);
  assert.match(r.error.message, /disk full/);
  assert.deepEqual(await repo.list(), before);
});

test('compensates when updating the parent fails after children exist', async () => {
  const repo = await seeded();
  const before = await repo.list();
  const flaky = Object.create(repo);
  flaky.save = async (s) => {
    if (s.id === 'ST-001') throw new Error('parent locked');
    return repo.save(s);
  };
  const r = await splitStory(flaky, 'ST-001', children);
  assert.equal(r.ok, false);
  assert.deepEqual(await repo.list(), before);
});
