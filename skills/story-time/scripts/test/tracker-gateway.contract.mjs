// Contract every TrackerGateway adapter must satisfy (Liskov: push logic
// works against any of them). Adapters call runTrackerGatewayContract.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { story } from './fixtures.mjs';

export function runTrackerGatewayContract(name, makeGateway) {
  test(`${name}: create returns a remote id and url`, async () => {
    const { gateway } = await makeGateway();
    const r = await gateway.create(story({ id: 'ST-1', title: 'Save cart' }));
    assert.ok(r.remoteId, 'remoteId');
    assert.match(r.url, /^https:\/\//);
  });

  test(`${name}: update of a created item keeps its remote id`, async () => {
    const { gateway } = await makeGateway();
    const created = await gateway.create(story({ id: 'ST-1', title: 'Save cart' }));
    const updated = await gateway.update(story({ id: 'ST-1', title: 'Save cart v2', remote: created.remoteId }));
    assert.equal(String(updated.remoteId), String(created.remoteId));
    assert.equal(updated.url, created.url);
  });

  test(`${name}: update without a remote id is rejected`, async () => {
    const { gateway } = await makeGateway();
    await assert.rejects(gateway.update(story({ id: 'ST-1' })), /no remote/i);
  });

  test(`${name}: update of an item that does not exist rejects`, async () => {
    const { gateway } = await makeGateway();
    await assert.rejects(gateway.update(story({ id: 'ST-1', remote: '999' })));
  });

  test(`${name}: remove takes a created item out of the active backlog`, async () => {
    const { gateway, isActive } = await makeGateway();
    const { remoteId } = await gateway.create(story({ id: 'ST-1' }));
    assert.equal(await isActive(remoteId), true);
    await gateway.remove(remoteId);
    assert.equal(await isActive(remoteId), false);
  });
}
