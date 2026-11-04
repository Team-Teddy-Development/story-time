import { test } from 'node:test';
import assert from 'node:assert/strict';
import { wsjf, rankBacklog } from '../src/domain/wsjf.mjs';
import { story } from './fixtures.mjs';

test('wsjf = (value + time criticality + risk reduction) / size', () => {
  assert.deepEqual(wsjf(story({ value: 8, time_criticality: 5, risk_reduction: 3, size: 4 })), { costOfDelay: 16, score: 4 });
});

test('wsjf is null when inputs are missing', () => {
  assert.equal(wsjf(story({ size: undefined })), null);
  assert.equal(wsjf(story({ value: undefined })), null);
});

test('rankBacklog orders ready stories by score, then smaller size, then oldest', () => {
  const a = story({ id: 'A', value: 1, time_criticality: 1, risk_reduction: 1, size: 3 }); // 1
  const b = story({ id: 'B', value: 8, time_criticality: 4, risk_reduction: 0, size: 3 }); // 4
  const c = story({ id: 'C', value: 4, time_criticality: 2, risk_reduction: 0, size: 1.5 }); // 4, smaller
  const d = story({ id: 'D', value: 8, time_criticality: 4, risk_reduction: 0, size: 3, created: '2026-01-01' }); // 4, older than B
  const { ranked } = rankBacklog([a, b, c, d]);
  assert.deepEqual(ranked.map((r) => r.story.id), ['C', 'D', 'B', 'A']);
});

test('rankBacklog excludes non-ready, blocked, and unscored stories with reasons', () => {
  const done = story({ id: 'DONE', status: 'done' });
  const draft = story({ id: 'DRAFT', status: 'draft' });
  const blocked = story({ id: 'BLOCKED', depends_on: ['DRAFT'] });
  const unscored = story({ id: 'UNSCORED', value: undefined });
  const { ranked, excluded } = rankBacklog([done, draft, blocked, unscored]);
  assert.deepEqual(ranked, []);
  const reasons = Object.fromEntries(excluded.map((e) => [e.story.id, e.reason]));
  assert.match(reasons.DRAFT, /status draft/);
  assert.match(reasons.BLOCKED, /blocked by DRAFT/);
  assert.match(reasons.UNSCORED, /missing WSJF/);
  assert.ok(!('DONE' in reasons), 'finished work is silently ignored');
});

test('rankBacklog can include refined stories when asked', () => {
  const refined = story({ id: 'R', status: 'refined' });
  assert.equal(rankBacklog([refined], { statuses: ['ready', 'refined'] }).ranked.length, 1);
});
