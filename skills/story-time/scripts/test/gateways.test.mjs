import { test } from 'node:test';
import assert from 'node:assert/strict';
import { runTrackerGatewayContract } from './tracker-gateway.contract.mjs';
import { fakeGh, fakeAz, newState } from './fake-trackers.mjs';
import { MemoryTrackerGateway } from '../src/adapters/memory-tracker-gateway.mjs';
import { GitHubIssuesGateway } from '../src/adapters/github-issues-gateway.mjs';
import { AdoBoardsGateway } from '../src/adapters/ado-boards-gateway.mjs';
import { GiteaIssuesGateway } from '../src/adapters/gitea-issues-gateway.mjs';
import { fakeGiteaFetch, newGiteaState, issueLabels } from './fake-gitea.mjs';
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

const GITEA = { baseUrl: 'https://gitea.example.com/', owner: 'acme', repo: 'shop' };

function gitea(opts = {}) {
  const state = newGiteaState();
  const fetch = fakeGiteaFetch(state, { token: 'gt_secret123', ...opts });
  const gateway = new GiteaIssuesGateway({ gitea: GITEA, token: opts.gatewayToken ?? 'gt_secret123', fetch });
  return { state, gateway, isActive: async (id) => state.issues[id]?.state === 'open' };
}

runTrackerGatewayContract('MemoryTrackerGateway', async () => {
  const gateway = new MemoryTrackerGateway();
  return { gateway, isActive: async (id) => gateway.items.get(String(id))?.open === true };
});
runTrackerGatewayContract('GitHubIssuesGateway', async () => github());
runTrackerGatewayContract('AdoBoardsGateway', async () => ado());
runTrackerGatewayContract('GiteaIssuesGateway', async () => gitea());

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

// --- Gitea ---

test('gitea create makes missing labels, sends label ids, and returns the issue url', async () => {
  const { state, gateway } = gitea();
  const r = await gateway.create(parseStory(READY_MD));
  assert.deepEqual(r, { remoteId: '1', url: 'https://gitea.example.com/acme/shop/issues/1' });
  assert.deepEqual(state.labels.map((l) => l.name).sort(), ['epic:EP-1', 'size:3', 'story', 'value:8']);
  assert.deepEqual(issueLabels(state, 1).sort(), ['epic:EP-1', 'size:3', 'story', 'value:8']);
  assert.equal(state.issues[1].title, 'Reset password via email');
  assert.match(state.issues[1].body, /### Acceptance Criteria\n- \[ \] Given a registered email/);
  const create = state.calls.find((c) => c.method === 'POST' && c.url.endsWith('/api/v1/repos/acme/shop/issues'));
  assert.ok(create.body.labels.every((id) => typeof id === 'number'), 'Gitea takes label ids, not names');
});

test('gitea reuses labels that already exist, across every page of the label list', async () => {
  const { state, gateway } = gitea();
  for (const name of ['bug', 'needs-design', 'story', 'size:3', 'value:8']) state.labels.push({ id: ++state.nextLabel, name, color: '#000000' });
  await gateway.create(parseStory(READY_MD));
  assert.equal(state.labels.filter((l) => l.name === 'story').length, 1);
  assert.equal(state.labels.filter((l) => l.name === 'value:8').length, 1, 'value:8 is on page 3 of the label list');
  assert.deepEqual(state.labels.map((l) => l.name).slice(5), ['epic:EP-1']);
});

test('gitea update replaces stale story-time labels but keeps the team\'s own labels', async () => {
  const { state, gateway } = gitea();
  const { remoteId } = await gateway.create(story({ id: 'ST-1', size: 3 }));
  state.labels.push({ id: ++state.nextLabel, name: 'needs-design', color: '#ff0000' });
  state.issues[remoteId].labelIds.push(state.nextLabel);
  await gateway.update(story({ id: 'ST-1', title: 'Renamed', size: 5, remote: remoteId }));
  assert.deepEqual(issueLabels(state, remoteId).sort(), ['needs-design', 'size:5', 'story', 'value:5'].sort());
  assert.equal(state.issues[remoteId].title, 'Renamed');
});

test('gitea status sync: done closes, archived closes with not-planned, active reopens and drops not-planned', async () => {
  const { state, gateway } = gitea();
  const { remoteId } = await gateway.create(story({ id: 'ST-1' }));
  await gateway.update(story({ id: 'ST-1', status: 'done', remote: remoteId }));
  assert.equal(state.issues[remoteId].state, 'closed');
  assert.ok(!issueLabels(state, remoteId).includes('not-planned'));
  await gateway.update(story({ id: 'ST-1', status: 'in-progress', remote: remoteId }));
  assert.equal(state.issues[remoteId].state, 'open');
  await gateway.update(story({ id: 'ST-1', status: 'archived', remote: remoteId }));
  assert.equal(state.issues[remoteId].state, 'closed');
  assert.ok(issueLabels(state, remoteId).includes('not-planned'));
  await gateway.update(story({ id: 'ST-1', status: 'ready', remote: remoteId }));
  assert.equal(state.issues[remoteId].state, 'open');
  assert.ok(!issueLabels(state, remoteId).includes('not-planned'));
});

test('gitea create of a done story closes the new issue', async () => {
  const { state, gateway } = gitea();
  const { remoteId } = await gateway.create(story({ id: 'ST-1', status: 'split' }));
  assert.equal(state.issues[remoteId].state, 'closed');
  assert.ok(issueLabels(state, remoteId).includes('not-planned'));
});

test('gitea remove closes the issue with a not-planned label and keeps other labels (issues are never deleted)', async () => {
  const { state, gateway } = gitea();
  const { remoteId } = await gateway.create(story({ id: 'ST-1' }));
  await gateway.remove(remoteId);
  assert.equal(state.issues[remoteId].state, 'closed');
  assert.ok(issueLabels(state, remoteId).includes('not-planned'));
  assert.ok(issueLabels(state, remoteId).includes('story'));
});

test('gitea sends the token in the Authorization header only, never in a URL', async () => {
  const { state, gateway } = gitea();
  const { remoteId } = await gateway.create(story({ id: 'ST-1' }));
  await gateway.update(story({ id: 'ST-1', status: 'done', remote: remoteId }));
  assert.ok(state.calls.length > 0);
  assert.ok(state.calls.every((c) => c.auth === 'token gt_secret123'));
  assert.ok(!state.calls.some((c) => c.url.includes('gt_secret123')));
});

test('gitea reports HTTP errors with the status and the server message, without the token', async () => {
  const { gateway } = gitea({ gatewayToken: 'gt_wrong999' });
  await assert.rejects(gateway.create(story({ id: 'ST-1' })), (e) => {
    assert.match(e.message, /Gitea .*HTTP 401.*token is required/);
    assert.doesNotMatch(e.message, /gt_wrong999/);
    return true;
  });
});

test('gitea refuses to start without a token', () => {
  assert.throws(() => new GiteaIssuesGateway({ gitea: GITEA, token: '', fetch: async () => {} }), /story secret set gitea/);
});
