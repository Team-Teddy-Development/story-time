import { test } from 'node:test';
import assert from 'node:assert/strict';
import { lintStory, DEFAULT_RULES } from '../src/domain/invest.mjs';
import { story } from './fixtures.mjs';

const ids = (r) => r.findings.map((f) => f.rule);

test('a well-formed, estimated, scored story is ready with no findings', () => {
  const r = lintStory(story());
  assert.deepEqual(r.findings, []);
  assert.equal(r.ready, true);
});

test('missing "so that" is a Valuable error', () => {
  const r = lintStory(story({ sections: { ...story().sections, Story: 'As a shopper, I want to save my cart.' } }));
  const f = r.findings.find((x) => x.rule === 'statement.benefit');
  assert.equal(f.severity, 'error');
  assert.equal(f.invest, 'V');
  assert.equal(r.ready, false);
});

test('statement not in role/goal/benefit form is an error', () => {
  const r = lintStory(story({ sections: { ...story().sections, Story: 'Save the cart to the DB.' } }));
  assert.ok(ids(r).includes('statement.format'));
});

test('no acceptance criteria is a Testable error', () => {
  const r = lintStory(story({ sections: { Story: story().sections.Story } }));
  const f = r.findings.find((x) => x.rule === 'ac.present');
  assert.equal(f.severity, 'error');
  assert.equal(f.invest, 'T');
});

test('acceptance criteria not in Given/When/Then form are warnings', () => {
  const r = lintStory(story({ sections: { ...story().sections, 'Acceptance Criteria': '- [ ] cart is saved' } }));
  const f = r.findings.find((x) => x.rule === 'ac.gherkin');
  assert.equal(f.severity, 'warning');
  assert.equal(r.ready, true, 'warnings do not block readiness');
});

test('vague words in acceptance criteria are flagged', () => {
  const r = lintStory(
    story({ sections: { ...story().sections, 'Acceptance Criteria': '- [ ] Given a cart, when saved, then it loads fast' } }),
  );
  const f = r.findings.find((x) => x.rule === 'ac.vague');
  assert.match(f.message, /fast/);
});

test('unestimated story is not ready', () => {
  const r = lintStory(story({ size: undefined }));
  assert.ok(ids(r).includes('size.estimated'));
  assert.equal(r.ready, false);
});

test('size of 13 or more is a Small error; 8 is a warning', () => {
  assert.equal(lintStory(story({ size: 13 })).findings.find((f) => f.rule === 'size.small').severity, 'error');
  assert.equal(lintStory(story({ size: 8 })).findings.find((f) => f.rule === 'size.small').severity, 'warning');
});

test('too many acceptance criteria suggests a split', () => {
  const many = Array.from({ length: 9 }, (_, i) => `- [ ] Given ${i}, when x, then y`).join('\n');
  const r = lintStory(story({ sections: { ...story().sections, 'Acceptance Criteria': many } }));
  assert.ok(ids(r).includes('ac.count'));
});

test('missing value scores warn because the story cannot be ranked', () => {
  const r = lintStory(story({ value: undefined }));
  assert.equal(r.findings.find((f) => f.rule === 'value.scored').severity, 'warning');
});

test('dependency on an unfinished story is an Independent warning when context is provided', () => {
  const dep = story({ id: 'ST-1', status: 'in-progress' });
  const r = lintStory(story({ depends_on: ['ST-1'] }), { stories: [dep] });
  const f = r.findings.find((x) => x.rule === 'independent.deps');
  assert.equal(f.invest, 'I');
  assert.match(f.message, /ST-1/);
});

test('dependency on a missing story is an error', () => {
  const r = lintStory(story({ depends_on: ['ST-404'] }), { stories: [] });
  assert.equal(r.findings.find((x) => x.rule === 'independent.deps').severity, 'error');
});

test('spikes need a question and a timebox instead of a user statement', () => {
  const spike = story({ type: 'spike', size: undefined, sections: { Question: 'Can we use provider X?' } });
  const r = lintStory(spike);
  assert.ok(ids(r).includes('spike.timebox'));
  assert.ok(!ids(r).includes('statement.format'));
  assert.ok(!ids(r).includes('size.estimated'));
  const ok = lintStory({ ...spike, timebox: '2d', sections: { Question: 'Can we use X?', 'Exit Criteria': '- [ ] Decision recorded' } });
  assert.equal(ok.ready, true);
});

test('rules are open for extension: custom rules run alongside defaults', () => {
  const noTodo = { id: 'custom.todo', invest: 'N', check: (s) => (/TODO/.test(s.title) ? [{ severity: 'error', message: 'TODO in title' }] : []) };
  const r = lintStory(story({ title: 'TODO later' }), { rules: [...DEFAULT_RULES, noTodo] });
  assert.ok(ids(r).includes('custom.todo'));
});

test('a generic persona ("a user", "someone") is a Valuable warning', () => {
  for (const who of ['a user', 'an end user', 'someone', 'the customer']) {
    const r = lintStory(story({ sections: { ...story().sections, Story: `As ${who}, I want to save my cart, so that I can finish later.` } }));
    const f = r.findings.find((x) => x.rule === 'statement.persona');
    assert.ok(f, `expected a persona warning for "${who}"`);
    assert.equal(f.severity, 'warning');
  }
  assert.ok(!lintStory(story()).findings.some((x) => x.rule === 'statement.persona'));
});

const spike = (overrides = {}) =>
  story({ id: 'ST-200', type: 'spike', size: undefined, timebox: '1d', sections: { Question: 'Can X do 3DS?', 'Exit Criteria': '- [ ] Decision recorded' }, ...overrides });

test('an open spike no story depends on is a spike.unblocks error', () => {
  const s = spike();
  const r = lintStory(s, { stories: [s, story({ id: 'ST-201' })] });
  const f = r.findings.find((x) => x.rule === 'spike.unblocks');
  assert.equal(f.severity, 'error');
  assert.equal(f.message, 'No story lists ST-200 in depends_on. A spike must unblock a named story: link it, or archive the spike.');
  assert.equal(r.ready, false);
});

test('spike.unblocks does not fire when at least one story depends on the spike', () => {
  const s = spike();
  const r = lintStory(s, { stories: [s, story({ id: 'ST-201', depends_on: ['ST-200'] })] });
  assert.ok(!ids(r).includes('spike.unblocks'));
  assert.equal(r.ready, true);
});

test('spike.unblocks ignores closed spikes and runs only with the backlog as context', () => {
  for (const status of ['done', 'archived', 'split']) {
    const s = spike({ status });
    assert.ok(!ids(lintStory(s, { stories: [s] })).includes('spike.unblocks'), status);
  }
  assert.ok(!ids(lintStory(spike())).includes('spike.unblocks'));
  assert.ok(!ids(lintStory(story(), { stories: [story()] })).includes('spike.unblocks'), 'only spikes');
});

test('a state_machine path that does not exist is an error', () => {
  const s = story({ state_machine: 'model/state/enrollment.toml' });
  const seen = [];
  const r = lintStory(s, { exists: (p) => (seen.push(p), false) });
  assert.deepEqual(seen, ['model/state/enrollment.toml']);
  const f = r.findings.find((x) => x.rule === 'state_machine.exists');
  assert.equal(f.severity, 'error');
  assert.equal(f.message, 'state_machine: model/state/enrollment.toml does not exist (paths are relative to the repo root). Write it with the state-machine skill.');
});

test('state_machine.exists passes when the file exists, and is skipped without a field or a file check', () => {
  const s = story({ state_machine: 'model/state/enrollment.toml' });
  assert.ok(!ids(lintStory(s, { exists: () => true })).includes('state_machine.exists'));
  assert.ok(!ids(lintStory(story(), { exists: () => false })).includes('state_machine.exists'));
  assert.ok(!ids(lintStory(s)).includes('state_machine.exists'));
});
