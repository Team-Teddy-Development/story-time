import { test } from 'node:test';
import assert from 'node:assert/strict';
import { runTrackerGatewayContract } from './tracker-gateway.contract.mjs';
import { fakeGh, fakeAz, newState } from './fake-trackers.mjs';
import { MemoryTrackerGateway } from '../src/adapters/memory-tracker-gateway.mjs';
import { GitHubIssuesGateway } from '../src/adapters/github-issues-gateway.mjs';
import { AdoBoardsGateway } from '../src/adapters/ado-boards-gateway.mjs';
import { assertTrackerGateway } from '../src/ports/tracker-gateway.mjs';
import { parseStory } from '../src/domain/story.mjs';
import { READY_MD, story } from './fixtures.mjs';

const ADO = { organization: 'https://dev.azure.com/acme', project: 'Shop', process: 'agile' };

function github(opts = {}) {
  const state = newState();
  const gateway = new GitHubIssuesGateway({ repo: 'acme/shop', exec: fakeGh(state, opts), env: opts.env });
  return { state, gateway, isActive: async (id) => state.issues[id]?.state === 'OPEN' };
}

function ado(process = 'agile', opts = {}) {
  const state = newState();
  const gateway = new AdoBoardsGateway({ ado: { ...ADO, process }, exec: fakeAz(state, opts), env: opts.env });
  return { state, gateway, isActive: async (id) => Boolean(state.items[id]) };
}

runTrackerGatewayContract('MemoryTrackerGateway', async () => {
  const gateway = new MemoryTrackerGateway();
  return { gateway, isActive: async (id) => gateway.items.get(String(id))?.open === true };
});
runTrackerGatewayContract('GitHubIssuesGateway', async () => github());
runTrackerGatewayContract('AdoBoardsGateway', async () => ado());

test('assertTrackerGateway rejects objects missing port methods', () => {
  assert.throws(() => assertTrackerGateway({ create() {} }), /missing: update, remove/);
});

// --- GitHub ---

test('github create ensures labels exist, sends the body on stdin, and parses the issue number', async () => {
  const { state, gateway } = github();
  const r = await gateway.create(parseStory(READY_MD));
  assert.deepEqual(r, { remoteId: '1', url: 'https://github.com/acme/shop/issues/1' });
  assert.deepEqual(state.labels.sort(), ['epic:EP-1', 'size:3', 'story', 'value:8']);
  assert.match(state.issues[1].body, /### Acceptance Criteria\n- \[ \] Given a registered email/);
  const create = state.calls.find((c) => c[1] === 'issue' && c[2] === 'create');
  assert.ok(create.includes('--body-file') && create[create.indexOf('--body-file') + 1] === '-', 'body never goes in argv');
});

test('github update replaces stale story-time labels but keeps the team\'s own labels', async () => {
  const { state, gateway } = github();
  const { remoteId } = await gateway.create(story({ id: 'ST-1', size: 3 }));
  state.issues[remoteId].labels.push('needs-design');
  await gateway.update(story({ id: 'ST-1', size: 5, remote: remoteId }));
  assert.deepEqual(state.issues[remoteId].labels.sort(), ['needs-design', 'size:5', 'story', 'value:5'].sort());
});

test('github status sync: done closes as completed, archived as not planned, reopening when active again', async () => {
  const { state, gateway } = github();
  const { remoteId } = await gateway.create(story({ id: 'ST-1' }));
  await gateway.update(story({ id: 'ST-1', status: 'done', remote: remoteId }));
  assert.deepEqual([state.issues[remoteId].state, state.issues[remoteId].reason], ['CLOSED', 'completed']);
  await gateway.update(story({ id: 'ST-1', status: 'in-progress', remote: remoteId }));
  assert.equal(state.issues[remoteId].state, 'OPEN');
  await gateway.update(story({ id: 'ST-1', status: 'archived', remote: remoteId }));
  assert.equal(state.issues[remoteId].reason, 'not planned');
});

test('github remove closes as not planned (issues are never deleted)', async () => {
  const { state, gateway } = github();
  const { remoteId } = await gateway.create(story({ id: 'ST-1' }));
  await gateway.remove(remoteId);
  assert.deepEqual([state.issues[remoteId].state, state.issues[remoteId].reason], ['CLOSED', 'not planned']);
});

test('github passes the token to gh as GH_TOKEN in the environment, never in argv', async () => {
  const seen = [];
  const state = newState();
  const inner = fakeGh(state);
  const exec = (cmd, args, opts) => {
    seen.push(opts.env);
    return inner(cmd, args, opts);
  };
  const gateway = new GitHubIssuesGateway({ repo: 'acme/shop', exec, env: { GH_TOKEN: 'ghp_secret123' } });
  await gateway.create(story({ id: 'ST-1' }));
  assert.ok(seen.every((e) => e?.GH_TOKEN === 'ghp_secret123'));
  assert.ok(!JSON.stringify(state.calls).includes('ghp_secret123'));
});

// --- Azure DevOps ---

test('ado create maps fields by reference name and returns the work item url', async () => {
  const { state, gateway } = ado();
  const r = await gateway.create(parseStory(READY_MD));
  assert.deepEqual(r, { remoteId: '1', url: 'https://dev.azure.com/acme/Shop/_workitems/edit/1' });
  const item = state.items[1];
  assert.equal(item.type, 'User Story');
  assert.equal(item.title, 'Reset password via email');
  assert.match(item.description, /^<p>As a registered user/);
  assert.match(item.fields['Microsoft.VSTS.Common.AcceptanceCriteria'], /^<ul><li>Given a registered email/);
  assert.equal(item.fields['Microsoft.VSTS.Scheduling.StoryPoints'], '3');
  assert.equal(item.fields['System.Tags'], 'story-time; ST-001; epic:EP-1');
  assert.ok(state.calls[0].includes('--output') && state.calls[0].includes('json'));
});

test('ado scrum uses Product Backlog Item and Effort', async () => {
  const { state, gateway } = ado('scrum');
  await gateway.create(parseStory(READY_MD));
  assert.equal(state.items[1].type, 'Product Backlog Item');
  assert.equal(state.items[1].fields['Microsoft.VSTS.Scheduling.Effort'], '3');
});

test('ado status sync maps done and archived to the process template states', async () => {
  const cases = [['agile', 'Closed', 'Removed'], ['scrum', 'Done', 'Removed'], ['basic', 'Done', 'Done']];
  for (const [process, done, archived] of cases) {
    const { state, gateway } = ado(process);
    const { remoteId } = await gateway.create(story({ id: 'ST-1' }));
    await gateway.update(story({ id: 'ST-1', status: 'done', remote: remoteId }));
    assert.equal(state.items[remoteId].state, done, `${process} done`);
    await gateway.update(story({ id: 'ST-1', status: 'archived', remote: remoteId }));
    assert.equal(state.items[remoteId].state, archived, `${process} archived`);
  }
});

test('ado leaves workflow states alone for active stories', async () => {
  const { state, gateway } = ado();
  const { remoteId } = await gateway.create(story({ id: 'ST-1', status: 'in-progress' }));
  await gateway.update(story({ id: 'ST-1', status: 'ready', remote: remoteId }));
  assert.equal(state.items[remoteId].state, 'New');
  assert.ok(!state.calls.some((c) => c.includes('--state')));
});

test('ado remove sends the work item to the recycle bin', async () => {
  const { state, gateway } = ado();
  const { remoteId } = await gateway.create(story({ id: 'ST-1' }));
  await gateway.remove(remoteId);
  const del = state.calls.find((c) => c[3] === 'delete');
  assert.ok(del.includes('--yes'));
  assert.equal(state.items[remoteId], undefined);
});

test('ado passes the PAT as AZURE_DEVOPS_EXT_PAT in the environment', async () => {
  let env;
  const state = newState();
  const inner = fakeAz(state);
  const gateway = new AdoBoardsGateway({ ado: ADO, exec: (c, a, o) => ((env = o.env), inner(c, a, o)), env: { AZURE_DEVOPS_EXT_PAT: 'pat-xyz' } });
  await gateway.create(story({ id: 'ST-1' }));
  assert.equal(env.AZURE_DEVOPS_EXT_PAT, 'pat-xyz');
  assert.ok(!JSON.stringify(state.calls).includes('pat-xyz'));
});
