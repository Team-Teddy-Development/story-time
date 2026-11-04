// Contract every StoryRepository adapter must satisfy (Liskov: any adapter
// is substitutable). Adapters call runRepositoryContract with a factory.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { story } from './fixtures.mjs';

export function runRepositoryContract(name, makeRepo) {
  test(`${name}: save then get returns an equal story`, async () => {
    const repo = await makeRepo();
    const s = story({ id: 'ST-1' });
    await repo.save(s);
    assert.deepEqual(await repo.get('ST-1'), s);
  });

  test(`${name}: get of unknown id returns null`, async () => {
    const repo = await makeRepo();
    assert.equal(await repo.get('ST-404'), null);
  });

  test(`${name}: save overwrites an existing story`, async () => {
    const repo = await makeRepo();
    await repo.save(story({ id: 'ST-1', title: 'old' }));
    await repo.save(story({ id: 'ST-1', title: 'new' }));
    assert.equal((await repo.get('ST-1')).title, 'new');
    assert.equal((await repo.list()).length, 1);
  });

  test(`${name}: list filters by status and sorts by id`, async () => {
    const repo = await makeRepo();
    await repo.save(story({ id: 'ST-2', status: 'draft' }));
    await repo.save(story({ id: 'ST-1', status: 'ready' }));
    await repo.save(story({ id: 'ST-3', status: 'ready' }));
    assert.deepEqual((await repo.list()).map((s) => s.id), ['ST-1', 'ST-2', 'ST-3']);
    assert.deepEqual((await repo.list({ status: 'ready' })).map((s) => s.id), ['ST-1', 'ST-3']);
  });

  test(`${name}: remove deletes and reports whether anything was removed`, async () => {
    const repo = await makeRepo();
    await repo.save(story({ id: 'ST-1' }));
    assert.equal(await repo.remove('ST-1'), true);
    assert.equal(await repo.remove('ST-1'), false);
    assert.equal(await repo.get('ST-1'), null);
  });

  test(`${name}: nextId increments past the highest existing id for the prefix`, async () => {
    const repo = await makeRepo();
    assert.equal(await repo.nextId('ST'), 'ST-001');
    await repo.save(story({ id: 'ST-009' }));
    await repo.save(story({ id: 'EP-050' }));
    assert.equal(await repo.nextId('ST'), 'ST-010');
    assert.equal(await repo.nextId('EP'), 'EP-051');
  });

  test(`${name}: returned stories are copies, not shared references`, async () => {
    const repo = await makeRepo();
    await repo.save(story({ id: 'ST-1' }));
    const a = await repo.get('ST-1');
    a.title = 'mutated';
    assert.notEqual((await repo.get('ST-1')).title, 'mutated');
  });
}
