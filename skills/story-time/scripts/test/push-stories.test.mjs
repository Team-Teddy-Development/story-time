import { test } from 'node:test';
import assert from 'node:assert/strict';
import { pushStories, planPush } from '../src/application/push-stories.mjs';
import { InMemoryStoryRepository } from '../src/adapters/memory-story-repository.mjs';
import { MemoryTrackerGateway } from '../src/adapters/memory-tracker-gateway.mjs';
import { story } from './fixtures.mjs';

const seeded = (...stories) => new InMemoryStoryRepository(stories);

test('planPush: create without a remote, update with one, skip closed stories never pushed', async () => {
  const repo = seeded(
    story({ id: 'ST-1' }),
    story({ id: 'ST-2', remote: '7' }),
    story({ id: 'ST-3', status: 'done' }),
    story({ id: 'ST-4', status: 'done', remote: '9' }),
  );
  const plan = await planPush(repo, ['ST-1', 'ST-2', 'ST-3', 'ST-4']);
  assert.deepEqual(plan.map((p) => [p.story.id, p.action]), [['ST-1', 'create'], ['ST-2', 'update'], ['ST-3', 'skip'], ['ST-4', 'update']]);
});

test('planPush rejects unknown ids before anything is pushed', async () => {
  await assert.rejects(planPush(seeded(), ['ST-404']), /ST-404 not found/);
});

test('pushStories creates remote items and links them back with a changelog line', async () => {
  const repo = seeded(story({ id: 'ST-1' }), story({ id: 'ST-2' }));
  const gateway = new MemoryTrackerGateway();
  const r = await pushStories(repo, gateway, ['ST-1', 'ST-2'], { date: '2026-11-04' });
  assert.equal(r.ok, true);
  assert.deepEqual(r.pushed.map((p) => [p.id, p.action, p.remoteId]), [['ST-1', 'create', '1'], ['ST-2', 'create', '2']]);
  const s1 = await repo.get('ST-1');
  assert.equal(s1.remote, '1');
  assert.equal(s1.remote_url, 'https://tracker.example/1');
  assert.match(s1.sections.Changelog, /2026-11-04 pushed to tracker as 1/);
});

test('pushing again updates the same remote item instead of creating a duplicate', async () => {
  const repo = seeded(story({ id: 'ST-1' }));
  const gateway = new MemoryTrackerGateway();
  await pushStories(repo, gateway, ['ST-1']);
  await repo.save({ ...(await repo.get('ST-1')), title: 'Renamed' });
  const r = await pushStories(repo, gateway, ['ST-1']);
  assert.equal(r.pushed[0].action, 'update');
  assert.equal(gateway.items.size, 1);
  assert.equal(gateway.items.get('1').title, 'Renamed');
});

test('saga: a remote failure removes the items already created and leaves local stories untouched', async () => {
  const repo = seeded(story({ id: 'ST-1' }), story({ id: 'ST-2' }), story({ id: 'ST-3' }));
  const before = await repo.list();
  const gateway = new MemoryTrackerGateway({ failOn: (op, s) => op === 'create' && s.id === 'ST-3' });
  const r = await pushStories(repo, gateway, ['ST-1', 'ST-2', 'ST-3']);
  assert.equal(r.ok, false);
  assert.match(r.error.message, /ST-3/);
  assert.deepEqual([...gateway.items.values()].map((i) => i.open), [false, false], 'created items are rolled back');
  assert.deepEqual(await repo.list(), before);
});

test('saga: a local save failure rolls back remote creates and restores local files', async () => {
  const repo = seeded(story({ id: 'ST-1' }), story({ id: 'ST-2' }));
  const before = await repo.list();
  const gateway = new MemoryTrackerGateway();
  let saves = 0;
  const flaky = Object.create(repo);
  flaky.save = async (s) => {
    if (++saves === 2) throw new Error('disk full');
    return repo.save(s);
  };
  const r = await pushStories(flaky, gateway, ['ST-1', 'ST-2']);
  assert.equal(r.ok, false);
  assert.deepEqual(await repo.list(), before);
  assert.ok([...gateway.items.values()].every((i) => !i.open));
});

test('dryRun returns the plan and calls nothing', async () => {
  const repo = seeded(story({ id: 'ST-1' }));
  const gateway = new MemoryTrackerGateway();
  const r = await pushStories(repo, gateway, ['ST-1'], { dryRun: true });
  assert.deepEqual(r.plan.map((p) => [p.id, p.action]), [['ST-1', 'create']]);
  assert.equal(gateway.items.size, 0);
});
