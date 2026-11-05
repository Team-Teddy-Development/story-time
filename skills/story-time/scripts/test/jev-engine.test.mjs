import { test } from 'node:test';
import assert from 'node:assert/strict';
import { JevDecisionEngine } from '../src/adapters/jev/jev-decision-engine.mjs';
import { JevClient } from '../src/adapters/jev/jev-client.mjs';
import { CodeDecisionEngine } from '../src/domain/code-decision-engine.mjs';
import { selectDecisionEngine } from '../src/application/select-decision-engine.mjs';
import { parseStory } from '../src/domain/story.mjs';
import { READY_MD } from './fixtures.mjs';
import { fakeFetch, okBody } from './fake-jev.mjs';

const KEY = 'tsk_live_0123456789abcdef0123456789abcdef';
const ready = () => parseStory(READY_MD);
const engineWith = (fetch) =>
  new JevDecisionEngine({ client: new JevClient({ apiKey: KEY, fetch, sleep: async () => {}, warn: () => {} }), fallback: new CodeDecisionEngine() });

test('with a key, decisions come from Jev in a single batched request', async () => {
  const fetch = fakeFetch();
  const j = await engineWith(fetch).judge(ready());
  assert.equal(j.source, 'jev');
  assert.equal(j.model, 'jev-1.13.0');
  assert.equal(fetch.judgeCalls().length, 1);
  assert.equal(j.decisions.negotiable.value, true);
  assert.equal(j.decisions.size.value, 3);
  assert.deepEqual(j.usage, { inputTokens: 400, outputTokens: 12, costUsd: 0.0000168 });
});

test('the story sent to Jev is only the judged fields, never the changelog or frontmatter', async () => {
  const fetch = fakeFetch();
  await engineWith(fetch).judge(ready());
  const sent = JSON.stringify(fetch.judgeCalls()[0].request.state);
  assert.doesNotMatch(sent, /Changelog|created|EP-1|Uses existing mailer/);
});

test('when Jev is unusable, the code engine decides and the reason is recorded', async () => {
  const j = await engineWith(fakeFetch(() => ({ status: 503 }))).judge(ready());
  assert.equal(j.source, 'fallback');
  assert.equal(j.reason, 'overloaded');
  assert.equal(j.decisions.type.value, 'story', 'still a complete judgement');
});

test('a story containing an email is judged by code without leaving the machine', async () => {
  const fetch = fakeFetch();
  const s = ready();
  s.sections.Story = 'As a registered user, I want resets sent to me@example.com, so that I can get back in.';
  const j = await engineWith(fetch).judge(s);
  assert.deepEqual([j.source, j.reason], ['fallback', 'egress_refused']);
  assert.equal(fetch.judgeCalls().length, 0);
});

test('a malformed Jev answer falls back rather than trusting it', async () => {
  const fetch = fakeFetch((req) => ({ status: 200, body: { ...okBody(req), answers: {} } }));
  const j = await engineWith(fetch).judge(ready());
  assert.deepEqual([j.source, j.reason], ['fallback', 'protocol_error']);
});

// --- selection: no key means code, a key means Jev ---

test('no key selects the code engine', () => {
  const { engine, mode, why } = selectDecisionEngine({ apiKey: undefined, env: {}, config: {} });
  assert.ok(engine instanceof CodeDecisionEngine);
  assert.equal(mode, 'code');
  assert.match(why, /no TypeSafe key/);
});

test('a key selects Jev, with the configured model', () => {
  const { engine, mode, model } = selectDecisionEngine({ apiKey: KEY, env: {}, config: { jev: { model: 'jev-preview' } }, fetch: fakeFetch() });
  assert.ok(engine instanceof JevDecisionEngine);
  assert.deepEqual([mode, model], ['jev', 'jev-preview']);
});

test('STORY_TIME_JEV=off is a kill switch even with a key', () => {
  for (const off of ['off', '0', 'false', 'no']) {
    const { mode, why } = selectDecisionEngine({ apiKey: KEY, env: { STORY_TIME_JEV: off }, config: {} });
    assert.equal(mode, 'code', off);
    assert.match(why, /STORY_TIME_JEV/);
  }
});
