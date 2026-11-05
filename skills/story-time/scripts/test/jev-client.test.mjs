import { test } from 'node:test';
import assert from 'node:assert/strict';
import { JevClient, JEV_DEFAULT_MODEL, JEV_ENDPOINT } from '../src/adapters/jev/jev-client.mjs';
import { scanPayload, assertVendorEndpoint } from '../src/adapters/jev/jev-egress.mjs';
import { fakeFetch, okBody } from './fake-jev.mjs';

const KEY = 'tsk_live_0123456789abcdef0123456789abcdef';
const STATE = { title: 'Save cart', statement: 'As a shopper, I want to save my cart, so that I can finish later.' };
const QUESTIONS = {
  benefit: { type: 'noul', instructions: 'Read state.statement.', criteria: { true: 'The statement gives a concrete reason the shopper wants this.', false: 'The statement gives no reason the shopper wants this.' } },
  kind: { type: 'choice', instructions: 'Classify state.statement.', criteria: { story: 'new behaviour for a user', bug: 'broken behaviour' } },
};

function client(fetch, opts = {}) {
  const delays = [];
  const warnings = [];
  const c = new JevClient({
    apiKey: KEY, fetch, sleep: async (ms) => delays.push(ms), random: () => 0.5, warn: (m) => warnings.push(m), ...opts,
  });
  return { c, delays, warnings };
}

// --- wire protocol ---

test('POSTs {state, model, questions} to the vendor endpoint with a Bearer key and redirects refused', async () => {
  const fetch = fakeFetch();
  const { c } = client(fetch, { canary: false });
  const r = await c.decide({ state: STATE, questions: QUESTIONS });
  assert.equal(r.ok, true);
  const [call] = fetch.calls;
  assert.equal(call.url, 'https://api.typesafe.ai/v1/systemone');
  assert.equal(call.init.method, 'POST');
  assert.deepEqual(call.init.headers, { authorization: `Bearer ${KEY}`, 'content-type': 'application/json' });
  assert.equal(call.init.redirect, 'error');
  assert.ok(call.init.signal, 'every request has a timeout');
  assert.deepEqual(Object.keys(call.request), ['state', 'model', 'questions']);
  assert.equal(call.request.model, JEV_DEFAULT_MODEL);
  assert.equal(JEV_DEFAULT_MODEL, 'jev-1.13.0', 'pinned, so decisions are reproducible');
  assert.equal(JEV_ENDPOINT, 'https://api.typesafe.ai/v1/systemone');
});

test('returns the answering model, the answers, and token usage', async () => {
  const { c } = client(fakeFetch(), { canary: false });
  const r = await c.decide({ state: STATE, questions: QUESTIONS });
  assert.equal(r.source, 'jev');
  assert.equal(r.model, 'jev-1.13.0');
  assert.equal(r.answers.benefit.noul, 0.9);
  assert.deepEqual(r.usage, { inputTokens: 400, outputTokens: 12 });
});

test('the key never appears in the body, the URL, or a returned reason', async () => {
  const fetch = fakeFetch(() => ({ status: 500, body: { error_type: 'x' } }));
  const { c } = client(fetch, { canary: false });
  const r = await c.decide({ state: STATE, questions: QUESTIONS });
  assert.ok(!fetch.calls.some((call) => call.url.includes(KEY) || call.init.body.includes(KEY)));
  assert.ok(!JSON.stringify(r).includes(KEY));
});

test('no key means no request at all', async () => {
  const fetch = fakeFetch();
  const r = await new JevClient({ apiKey: '  ', fetch }).decide({ state: STATE, questions: QUESTIONS });
  assert.deepEqual([r.ok, r.reason], [false, 'unconfigured']);
  assert.equal(fetch.calls.length, 0);
});

// --- strict response validation ---

const badResponses = {
  'missing model': (req) => ({ ...okBody(req), model: undefined }),
  'missing usage': (req) => ({ ...okBody(req), usage: undefined }),
  'missing answer': (req) => ({ ...okBody(req), answers: { benefit: okBody(req).answers.benefit } }),
  'extra answer': (req) => ({ ...okBody(req), answers: { ...okBody(req).answers, surprise: { type: 'noul', noul: 0.5 } } }),
  'wrong type': (req) => ({ ...okBody(req), answers: { ...okBody(req).answers, benefit: { type: 'choice', choice: 'x' } } }),
  'probability out of range': (req) => ({ ...okBody(req), answers: { ...okBody(req).answers, benefit: { type: 'noul', noul: 1.2 } } }),
  'unknown choice': (req) => ({ ...okBody(req), answers: { ...okBody(req).answers, kind: { type: 'choice', choice: 'epic', probabilities: {}, confidence: 0.9 } } }),
  'negative tokens': (req) => ({ ...okBody(req), usage: { input_tokens: -1, output_tokens: 0 } }),
};

for (const [name, mutate] of Object.entries(badResponses)) {
  test(`a response with ${name} is a protocol error, never defaulted`, async () => {
    const fetch = fakeFetch((req) => ({ status: 200, body: mutate(req) }));
    const { c } = client(fetch, { canary: false });
    const r = await c.decide({ state: STATE, questions: QUESTIONS });
    assert.deepEqual([r.ok, r.reason], [false, 'protocol_error']);
    assert.equal(fetch.calls.length, 1, 'protocol errors are not retried');
  });
}

test('a non-JSON body is a protocol error', async () => {
  const { c } = client(fakeFetch(() => ({ status: 200, body: '<html>' })), { canary: false });
  assert.equal((await c.decide({ state: STATE, questions: QUESTIONS })).reason, 'protocol_error');
});

// --- retries and failure classes ---

test('429 is retried with the server hint, then succeeds', async () => {
  const fetch = fakeFetch((req, n) => (n === 1 ? { status: 429, headers: { 'retry-after-ms': '1200' } } : { status: 200, body: okBody(req) }));
  const { c, delays } = client(fetch, { canary: false });
  const r = await c.decide({ state: STATE, questions: QUESTIONS });
  assert.equal(r.ok, true);
  assert.equal(r.attempts, 2);
  assert.deepEqual(delays, [1200]);
});

test('retry-after in seconds is honoured and clamped to 30s; no header falls back to backoff', async () => {
  const fetch = fakeFetch((req, n) => (n === 1 ? { status: 503, headers: { 'retry-after': '120' } } : n === 2 ? { status: 503 } : { status: 200, body: okBody(req) }));
  const { c, delays } = client(fetch, { canary: false });
  await c.decide({ state: STATE, questions: QUESTIONS });
  assert.equal(delays[0], 30_000);
  assert.ok(delays[1] > 0 && delays[1] <= 5_000, `backoff, not zero: ${delays[1]}`);
});

test('three failed attempts fall back with rate_limited or overloaded', async () => {
  const rl = client(fakeFetch(() => ({ status: 429 })), { canary: false });
  const r1 = await rl.c.decide({ state: STATE, questions: QUESTIONS });
  assert.deepEqual([r1.reason, r1.attempts], ['rate_limited', 3]);
  const ov = client(fakeFetch(() => ({ status: 529 })), { canary: false });
  assert.equal((await ov.c.decide({ state: STATE, questions: QUESTIONS })).reason, 'overloaded');
});

test('timeouts and transport errors (including refused redirects) are retried, then classified', async () => {
  const timeout = client(fakeFetch(() => { throw Object.assign(new Error('timed out'), { name: 'TimeoutError' }); }), { canary: false });
  assert.equal((await timeout.c.decide({ state: STATE, questions: QUESTIONS })).reason, 'timeout');
  const redirect = client(fakeFetch(() => { throw new TypeError('fetch failed: redirect mode is set to error'); }), { canary: false });
  const r = await redirect.c.decide({ state: STATE, questions: QUESTIONS });
  assert.deepEqual([r.reason, r.attempts], ['transport_error', 3]);
});

test('401/403 latch for the run: no retry, no further requests, one warning', async () => {
  const fetch = fakeFetch(() => ({ status: 401 }));
  const { c, warnings } = client(fetch, { canary: false });
  assert.equal((await c.decide({ state: STATE, questions: QUESTIONS })).reason, 'auth_rejected');
  assert.equal((await c.decide({ state: STATE, questions: QUESTIONS })).reason, 'auth_rejected');
  assert.equal(fetch.calls.length, 1);
  assert.equal(warnings.length, 1);
  assert.doesNotMatch(warnings[0], new RegExp(KEY));
});

test('422 is a validation error and is not retried', async () => {
  const fetch = fakeFetch(() => ({ status: 422, body: 'bad question' }));
  const { c } = client(fetch, { canary: false });
  assert.equal((await c.decide({ state: STATE, questions: QUESTIONS })).reason, 'validation_error');
  assert.equal(fetch.calls.length, 1);
});

// --- egress ---

test('only the vendor origin over https is allowed', () => {
  assert.doesNotThrow(() => assertVendorEndpoint('https://api.typesafe.ai/v1/systemone'));
  for (const bad of ['http://api.typesafe.ai/v1/systemone', 'https://api.typesafe.ai.evil.com/x', 'https://gateway.ai.cloudflare.com/x', 'not a url']) {
    assert.throws(() => assertVendorEndpoint(bad), /endpoint/i, bad);
  }
  assert.throws(() => new JevClient({ apiKey: KEY, endpoint: 'https://example.com/v1/systemone' }), /endpoint/i);
});

test('scanPayload finds emails, phone numbers, and secrets anywhere in the payload', () => {
  const findings = scanPayload({
    state: { statement: 'Email jane@acme.com or call 555-123-4567', criteria: ['token ghp_abcdefghijklmnopqrstuvwxyz123456'] },
  });
  assert.deepEqual(findings.map((f) => f.kind).sort(), ['email', 'phone', 'secret']);
  assert.ok(findings.every((f) => f.path && !('value' in f)), 'findings name a path, never echo the value');
  assert.deepEqual(scanPayload({ state: STATE, questions: QUESTIONS }), []);
});

test('a payload with personal data or secrets is never sent', async () => {
  const fetch = fakeFetch();
  const { c } = client(fetch, { canary: false });
  const r = await c.decide({ state: { ...STATE, statement: 'Ping bob@corp.io when done' }, questions: QUESTIONS });
  assert.deepEqual([r.ok, r.reason], [false, 'egress_refused']);
  assert.deepEqual(r.findings, [{ kind: 'email', path: 'state.statement' }]);
  assert.equal(fetch.calls.length, 0);
});

// --- contradiction canary ---

test('the canary runs once, before the first real request', async () => {
  const fetch = fakeFetch();
  const { c } = client(fetch);
  await c.decide({ state: STATE, questions: QUESTIONS });
  await c.decide({ state: STATE, questions: QUESTIONS });
  assert.equal(fetch.calls.length, 3);
  assert.ok(Object.keys(fetch.calls[0].request.questions).every((k) => k.startsWith('canary_')));
});

test('a model answering every question the same way latches canary_failed', async () => {
  const constant = (req) => ({
    status: 200,
    body: { ...okBody(req), answers: Object.fromEntries(Object.keys(req.questions).map((k) => [k, { type: 'noul', noul: 0.62 }])) },
  });
  const fetch = fakeFetch(constant);
  const { c, warnings } = client(fetch);
  assert.equal((await c.decide({ state: STATE, questions: QUESTIONS })).reason, 'canary_failed');
  assert.equal((await c.decide({ state: STATE, questions: QUESTIONS })).reason, 'canary_failed');
  assert.equal(fetch.calls.length, 1, 'no real request after a failed canary');
  assert.equal(warnings.length, 1);
});

test('a canary that could not run (outage) does not latch; it retries next time', async () => {
  let down = true;
  const fetch = fakeFetch((req) => (down ? { status: 503 } : { status: 200, body: okBody(req) }));
  const { c } = client(fetch);
  assert.equal((await c.decide({ state: STATE, questions: QUESTIONS })).reason, 'overloaded');
  down = false;
  assert.equal((await c.decide({ state: STATE, questions: QUESTIONS })).ok, true);
});
