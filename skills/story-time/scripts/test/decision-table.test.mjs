import { test } from 'node:test';
import assert from 'node:assert/strict';
import { PROMOTION } from './fixtures.mjs';
import { parseToml } from '../src/domain/toml.mjs';
import { checkTable, tableCriteria } from '../src/domain/decision-table.mjs';

const EXTRA_ADMIN_RULE = `
[[rules]]
when = { role = "admin", verified = true }
then = { promote = "no" }
`;

const check = (toml, opts) => checkTable(parseToml(toml), opts);

test('a complete unique table has no defects', () => {
  const { table, defects } = check(PROMOTION);
  assert.deepEqual(defects, []);
  assert.equal(table.name, 'promotion');
  assert.equal(table.hitPolicy, 'unique');
  assert.deepEqual(table.inputs, [
    { name: 'role', domain: ['admin', 'member', 'guest'] },
    { name: 'verified', domain: [true, false] },
  ]);
  assert.deepEqual(table.outputs, ['promote']);
  assert.equal(table.rules.length, 4);
});

test('the name defaults to the one passed in (the file name)', () => {
  assert.equal(check(PROMOTION.replace('name = "promotion"\n', ''), { name: 'team-scope' }).table.name, 'team-scope');
});

test('an uncovered input combination is listed', () => {
  const r = check(PROMOTION.replace(/\[\[rules\]\]\nwhen = \{ role = "guest"[^\n]*\nthen[^\n]*\n/, ''));
  assert.deepEqual(r.defects, [
    { code: 'uncovered', message: 'No rule covers role=guest, verified=true.', combination: { role: 'guest', verified: true } },
    { code: 'uncovered', message: 'No rule covers role=guest, verified=false.', combination: { role: 'guest', verified: false } },
  ]);
});

test('overlapping rules in a unique table are named', () => {
  const r = check(PROMOTION + EXTRA_ADMIN_RULE);
  assert.deepEqual(r.defects, [
    { code: 'overlap', message: 'Rules 1 and 5 both match role=admin, verified=true (hit policy unique).', rules: [1, 5] },
  ]);
});

test('a first-hit table may overlap, but a rule earlier rules always win over never fires', () => {
  const r = check(PROMOTION.replace('"unique"', '"first"') + EXTRA_ADMIN_RULE);
  assert.deepEqual(r.defects, [
    { code: 'unreachable', message: 'Rule 5 never fires: earlier rules match every combination it covers (hit policy first).', rules: [5] },
  ]);
});

test('malformed rules and settings are schema defects', () => {
  const r = check(`hit_policy = "any"
outputs = ["promote"]

[inputs]
role = ["admin"]
empty = []

[[rules]]
when = { role = "owner" }
then = { promote = "yes", extra = 1 }
`);
  assert.deepEqual(r.defects.map((d) => d.message), [
    'hit_policy must be "unique" or "first", got "any".',
    'Input "empty" needs a non-empty list of values.',
    'Rule 1: role = "owner" is not in its domain (admin).',
    'Rule 1: no condition for input "empty"; write "-" for any value.',
    'Rule 1: unknown output "extra".',
  ]);
  assert.ok(r.defects.every((d) => d.code === 'schema'));
});

test('a table with too many combinations to enumerate is refused', () => {
  const values = JSON.stringify(Array.from({ length: 20 }, (_, i) => `v${i}`));
  const r = check(`hit_policy = "first"\noutputs = ["o"]\n[inputs]\na = ${values}\nb = ${values}\nc = ${values}\n[[rules]]\nwhen = { a = "-", b = "-", c = "-" }\nthen = { o = 1 }\n`);
  assert.deepEqual(r.defects, [{ code: 'schema', message: '8000 input combinations is more than 4096. Split the table.' }]);
});

test('criteria: one Given/When/Then over a row per input combination', () => {
  const { table } = check(PROMOTION);
  const c = tableCriteria(table);
  assert.equal(c.criterion, 'Given the inputs in a row of the promotion table, when the rule is evaluated, then the result is the outputs in that row.');
  assert.deepEqual(c.columns, ['rule', 'role', 'verified', 'promote']);
  assert.deepEqual(c.rows, [
    [1, 'admin', true, 'yes'],
    [1, 'admin', false, 'yes'],
    [2, 'member', true, 'yes'],
    [3, 'member', false, 'no'],
    [4, 'guest', true, 'no'],
    [4, 'guest', false, 'no'],
  ]);
  assert.equal(
    c.markdown,
    [
      '- [ ] Given the inputs in a row of the promotion table, when the rule is evaluated, then the result is the outputs in that row.',
      '',
      '  | rule | role | verified | promote |',
      '  |---|---|---|---|',
      '  | 1 | admin | true | yes |',
      '  | 1 | admin | false | yes |',
      '  | 2 | member | true | yes |',
      '  | 3 | member | false | no |',
      '  | 4 | guest | true | no |',
      '  | 4 | guest | false | no |',
    ].join('\n'),
  );
});

test('first-hit criteria take the first matching rule', () => {
  const { table } = check(PROMOTION.replace('"unique"', '"first"').replace('when = { role = "member", verified = true }', 'when = { role = "member", verified = "-" }'));
  assert.deepEqual(tableCriteria(table).rows.slice(2, 4), [
    [2, 'member', true, 'yes'],
    [2, 'member', false, 'yes'],
  ]);
});
