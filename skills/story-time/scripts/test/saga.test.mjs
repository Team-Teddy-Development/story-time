import { test } from 'node:test';
import assert from 'node:assert/strict';
import { runSaga } from '../src/saga/saga.mjs';

const step = (name, log, { fail = false, failCompensate = false } = {}) => ({
  name,
  async execute(ctx) {
    log.push(`do:${name}`);
    if (fail) throw new Error(`${name} failed`);
    ctx[name] = true;
    return `${name}-result`;
  },
  async compensate(ctx, result) {
    log.push(`undo:${name}:${result}`);
    if (failCompensate) throw new Error(`undo ${name} failed`);
  },
});

test('runs every step in order and returns their results', async () => {
  const log = [];
  const r = await runSaga([step('a', log), step('b', log)], {});
  assert.equal(r.ok, true);
  assert.deepEqual(r.results, { a: 'a-result', b: 'b-result' });
  assert.deepEqual(log, ['do:a', 'do:b']);
});

test('on failure, compensates completed steps in reverse order', async () => {
  const log = [];
  const r = await runSaga([step('a', log), step('b', log), step('c', log, { fail: true }), step('d', log)], {});
  assert.equal(r.ok, false);
  assert.equal(r.failedStep, 'c');
  assert.match(r.error.message, /c failed/);
  assert.deepEqual(log, ['do:a', 'do:b', 'do:c', 'undo:b:b-result', 'undo:a:a-result']);
  assert.deepEqual(r.compensated, ['b', 'a']);
});

test('keeps compensating when a compensation fails, and reports it', async () => {
  const log = [];
  const r = await runSaga([step('a', log), step('b', log, { failCompensate: true }), step('c', log, { fail: true })], {});
  assert.deepEqual(r.compensated, ['a']);
  assert.equal(r.compensationFailures.length, 1);
  assert.equal(r.compensationFailures[0].step, 'b');
  assert.ok(log.includes('undo:a:a-result'));
});

test('steps without compensate are allowed (read-only steps)', async () => {
  const log = [];
  const readOnly = { name: 'load', execute: async () => 'x' };
  const r = await runSaga([readOnly, step('b', log, { fail: true })], {});
  assert.equal(r.ok, false);
  assert.deepEqual(r.compensated, []);
});
