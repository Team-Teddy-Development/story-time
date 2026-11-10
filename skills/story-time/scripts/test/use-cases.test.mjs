import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createStory } from '../src/application/create-story.mjs';
import { updateStory } from '../src/application/update-story.mjs';
import { backlogStats } from '../src/application/backlog-stats.mjs';
import { InMemoryStoryRepository } from '../src/adapters/memory-story-repository.mjs';
import { story } from './fixtures.mjs';

test('createStory allocates the next id, defaults to draft, and records creation', async () => {
  const repo = new InMemoryStoryRepository([story({ id: 'ST-004' })]);
  const s = await createStory(repo, {
    title: 'Export invoices',
    persona: 'accountant',
    statement: 'As an accountant, I want to export invoices as CSV, so that I can reconcile in my spreadsheet.',
    acceptance: ['Given invoices this month, when I export, then I get one CSV row per invoice'],
    epic: 'EP-2',
  }, { date: '2026-11-04' });
  assert.equal(s.id, 'ST-005');
  assert.equal(s.status, 'draft');
  assert.equal(s.created, '2026-11-04');
  assert.match(s.sections['Acceptance Criteria'], /^- \[ \] Given invoices/);
  assert.match(s.sections.Changelog, /2026-11-04 created/);
  assert.deepEqual(await repo.get('ST-005'), s);
});

test('createStory without a statement leaves a TODO that fails lint', async () => {
  const s = await createStory(new InMemoryStoryRepository(), { title: 'Something' });
  assert.match(s.sections.Story, /^TODO/);
});

test('createStory requires a title and a known type', async () => {
  const repo = new InMemoryStoryRepository();
  await assert.rejects(createStory(repo, {}), /title/);
  await assert.rejects(createStory(repo, { title: 'x', type: 'saga' }), /type/);
});

test('createStory honours a custom id prefix', async () => {
  const s = await createStory(new InMemoryStoryRepository(), { title: 'x' }, { prefix: 'PAY' });
  assert.equal(s.id, 'PAY-001');
});

test('updateStory sets fields, coerces types, and writes a changelog entry', async () => {
  const repo = new InMemoryStoryRepository([story({ id: 'ST-1', status: 'refined', size: undefined })]);
  const s = await updateStory(repo, 'ST-1', { size: '5', depends_on: 'ST-2,ST-3' }, { note: 'estimated in refinement', date: '2026-11-04' });
  assert.equal(s.size, 5);
  assert.deepEqual(s.depends_on, ['ST-2', 'ST-3']);
  assert.match(s.sections.Changelog, /2026-11-04 set size=5, depends_on=ST-2,ST-3 — estimated in refinement/);
});

test('updateStory rejects unknown statuses and non-numeric scores', async () => {
  const repo = new InMemoryStoryRepository([story({ id: 'ST-1' })]);
  await assert.rejects(updateStory(repo, 'ST-1', { status: 'wip' }), /status/);
  await assert.rejects(updateStory(repo, 'ST-1', { value: 'high' }), /value must be a number/);
  await assert.rejects(updateStory(repo, 'ST-404', { size: 1 }), /not found/);
});

test('updateStory guards the ready transition with the INVEST lint', async () => {
  const repo = new InMemoryStoryRepository([story({ id: 'ST-1', status: 'refined', size: undefined })]);
  await assert.rejects(updateStory(repo, 'ST-1', { status: 'ready' }), /not ready.*size\.estimated/s);
  const forced = await updateStory(repo, 'ST-1', { status: 'ready' }, { force: true });
  assert.equal(forced.status, 'ready');
});

test('updateStory refuses to edit id', async () => {
  const repo = new InMemoryStoryRepository([story({ id: 'ST-1' })]);
  await assert.rejects(updateStory(repo, 'ST-1', { id: 'ST-2' }), /id/);
});

test('backlogStats summarizes counts, points, readiness gaps, and stale stories', async () => {
  const repo = new InMemoryStoryRepository([
    story({ id: 'ST-1', status: 'ready', size: 3 }),
    story({ id: 'ST-2', status: 'ready', size: 5 }),
    story({ id: 'ST-3', status: 'draft', size: undefined, updated: '2026-08-01' }),
    story({ id: 'ST-4', status: 'done', size: 8 }),
  ]);
  const st = await backlogStats(repo, { today: '2026-11-04', staleDays: 30 });
  assert.deepEqual(st.byStatus, { draft: 1, ready: 2, done: 1 });
  assert.equal(st.readyPoints, 8);
  assert.deepEqual(st.unestimated, ['ST-3']);
  assert.deepEqual(st.stale, ['ST-3']);
  assert.equal(st.open, 3);
});

test('noteStory appends a changelog entry without changing fields', async () => {
  const { noteStory } = await import('../src/application/note-story.mjs');
  const repo = new InMemoryStoryRepository([story({ id: 'ST-1', status: 'ready' })]);
  const s = await noteStory(repo, 'ST-1', 'added lockout criterion', { date: '2026-11-04' });
  assert.equal(s.status, 'ready');
  assert.equal(s.updated, '2026-11-04');
  assert.match(s.sections.Changelog, /- 2026-11-04 added lockout criterion$/);
  await assert.rejects(noteStory(repo, 'ST-1', '  '), /note/);
  await assert.rejects(noteStory(repo, 'ST-404', 'x'), /not found/);
});

test('updateStory gates ready on a missing state_machine file when given a file check', async () => {
  const repo = new InMemoryStoryRepository([story({ id: 'ST-010', status: 'refined', state_machine: 'model/state/x.toml' })]);
  await assert.rejects(updateStory(repo, 'ST-010', { status: 'ready' }, { exists: () => false }), /state_machine\.exists/);
  const s = await updateStory(repo, 'ST-010', { status: 'ready' }, { exists: () => true });
  assert.equal(s.status, 'ready');
});
