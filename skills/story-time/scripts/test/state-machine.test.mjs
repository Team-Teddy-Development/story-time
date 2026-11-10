import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ENROLLMENT } from './fixtures.mjs';
import { parseToml } from '../src/domain/toml.mjs';
import { checkMachine, machineCriteria, machineDiagram } from '../src/domain/state-machine.mjs';

const check = (toml, opts) => checkMachine(parseToml(toml), opts);
const codes = (r) => r.defects.map((d) => d.code);

test('a valid machine has no defects and a normalized model', () => {
  const { machine, defects } = check(ENROLLMENT);
  assert.deepEqual(defects, []);
  assert.equal(machine.entity, 'enrollment');
  assert.deepEqual(machine.guards, { complete: 'every required field is filled in' });
  assert.deepEqual(machine.transitions[0], { from: 'draft', to: 'submitted', event: 'submit', guard: 'complete' });
  assert.deepEqual(machine.invariants, ['An approved enrollment has a coverage start date.']);
});

test('the entity defaults to the name passed in (the file name)', () => {
  const { machine } = check(ENROLLMENT.replace('entity = "enrollment"\n', ''), { name: 'employment' });
  assert.equal(machine.entity, 'employment');
});

test('an unreachable state is a defect naming the state', () => {
  const r = check(ENROLLMENT.replace('"rejected"]\ninitial', '"rejected", "orphan"]\ninitial').replace('terminal = ["approved", "rejected"]', 'terminal = ["approved", "rejected", "orphan"]'));
  assert.deepEqual(r.defects, [{ code: 'unreachable', message: 'State "orphan" cannot be reached from the initial state "draft".' }]);
});

test('a non-terminal state with no way out is a dead end', () => {
  const r = check(ENROLLMENT.replace('terminal = ["approved", "rejected"]', 'terminal = ["approved"]'));
  assert.deepEqual(r.defects, [{ code: 'dead-end', message: 'State "rejected" is not terminal and has no outgoing transition.' }]);
});

test('a transition without an event is a defect', () => {
  const r = check(ENROLLMENT.replace('to = "approved"\nevent = "approve"', 'to = "approved"'));
  assert.deepEqual(r.defects, [{ code: 'missing-event', message: 'Transition 2 (submitted -> approved) has no event.' }]);
});

test('unknown states, events and guards are defects', () => {
  const r = check(ENROLLMENT.replace('to = "rejected"', 'to = "rejeced"').replace('event = "withdraw"', 'event = "retract"').replace('guard = "complete"', 'guard = "done"'));
  assert.deepEqual(codes(r).sort(), ['unknown-event', 'unknown-guard', 'unknown-state']);
  assert.ok(r.defects.some((d) => d.message === 'Transition 3 goes to unknown state "rejeced".'));
  assert.ok(r.defects.some((d) => d.message === 'Transition 4 uses unknown event "retract".'));
  assert.ok(r.defects.some((d) => d.message === 'Transition 1 uses unknown guard "done".'));
});

test('an unknown initial or terminal state is a defect', () => {
  const r = check(ENROLLMENT.replace('initial = "draft"', 'initial = "new"'));
  assert.ok(r.defects.some((d) => d.code === 'unknown-state' && d.message === 'Initial state "new" is not in states.'));
  const t = check(ENROLLMENT.replace('terminal = ["approved", "rejected"]', 'terminal = ["approved", "rejected", "gone"]'));
  assert.ok(t.defects.some((d) => d.code === 'unknown-state' && d.message === 'Terminal state "gone" is not in states.'));
});

test('a terminal state with an outgoing transition is a defect', () => {
  const r = check(`${ENROLLMENT}
[[transitions]]
from = "approved"
to = "draft"
event = "withdraw"
`);
  assert.deepEqual(r.defects, [{ code: 'terminal-exit', message: 'Terminal state "approved" has an outgoing transition on "withdraw".' }]);
});

test('two transitions from one state on one event with the same guard are ambiguous', () => {
  const r = check(`${ENROLLMENT}
[[transitions]]
from = "submitted"
to = "rejected"
event = "approve"
`);
  assert.deepEqual(r.defects, [{ code: 'ambiguous', message: 'State "submitted" has 2 transitions on "approve" with the same guard.' }]);
});

test('missing required fields are schema defects', () => {
  const r = checkMachine({});
  assert.deepEqual(r.defects.map((d) => d.message), [
    'states must be a non-empty list of names.',
    'initial must name a state.',
    'events must be a non-empty list of names.',
    'transitions must be one or more [[transitions]] tables.',
  ]);
  assert.ok(r.defects.every((d) => d.code === 'schema'));
});

test('criteria: one per transition, one per refused event, one per failing guard', () => {
  const { machine } = check(ENROLLMENT);
  const criteria = machineCriteria(machine);
  const count = (kind) => criteria.filter((c) => c.kind === kind).length;
  assert.equal(count('transition'), 4);
  assert.equal(count('refused'), 12, 'draft refuses 3 events, submitted 1, each terminal state all 4');
  assert.equal(count('guard'), 1);
  assert.deepEqual(criteria.slice(0, 2).map((c) => c.text), [
    'Given an enrollment in draft and every required field is filled in, when submit is received, then the enrollment is in submitted.',
    'Given an enrollment in submitted, when approve is received, then the enrollment is in approved.',
  ]);
  assert.ok(criteria.some((c) => c.text === 'Given an enrollment in draft and guard complete fails (every required field is filled in), when submit is received, then submit is refused and the enrollment stays in draft.'));
  assert.ok(criteria.some((c) => c.text === 'Given an enrollment in approved, when withdraw is received, then withdraw is refused and the enrollment stays in approved.'));
  assert.ok(!criteria.some((c) => c.kind === 'refused' && c.from === 'submitted' && c.event === 'approve'));
});

test('an unguarded alternative means a failing guard is not a refusal', () => {
  const { machine } = check(`${ENROLLMENT}
[[transitions]]
from = "draft"
to = "draft"
event = "submit"
`);
  assert.equal(machineCriteria(machine).filter((c) => c.kind === 'guard').length, 0);
});

test('criteria use "a" before a consonant', () => {
  const { machine } = check(ENROLLMENT.replace('entity = "enrollment"', 'entity = "cutover stage"'));
  assert.match(machineCriteria(machine)[0].text, /^Given a cutover stage in draft/);
});

test('diagram renders a Mermaid stateDiagram with guards', () => {
  const { machine } = check(ENROLLMENT);
  assert.equal(
    machineDiagram(machine),
    [
      'stateDiagram-v2',
      '    [*] --> draft',
      '    draft --> submitted: submit [complete]',
      '    submitted --> approved: approve',
      '    submitted --> rejected: reject',
      '    submitted --> draft: withdraw',
      '    approved --> [*]',
      '    rejected --> [*]',
    ].join('\n'),
  );
});
