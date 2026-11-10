import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseToml } from '../src/domain/toml.mjs';

test('parses top-level scalars: strings, integers, booleans', () => {
  const doc = parseToml(`
# a comment
entity = "enrollment"   # trailing comment
literal = 'C:\\path'
count = 1_000
negative = -3
on = true
off = false
`);
  assert.deepEqual(doc, { entity: 'enrollment', literal: 'C:\\path', count: 1000, negative: -3, on: true, off: false });
});

test('parses basic-string escapes', () => {
  assert.deepEqual(parseToml('s = "a \\"quoted\\" \\u00e9 tab\\t"'), { s: 'a "quoted" é tab\t' });
});

test('parses multi-line arrays with comments and a trailing comma', () => {
  const doc = parseToml(`states = [
  "draft",     # first
  "submitted",
  # a comment line
  "approved",
]
mixed = [true, false, 3]
nested = [["a", 1], ["b", 2]]
empty = []`);
  assert.deepEqual(doc, {
    states: ['draft', 'submitted', 'approved'],
    mixed: [true, false, 3],
    nested: [['a', 1], ['b', 2]],
    empty: [],
  });
});

test('parses tables, arrays of tables, and inline tables', () => {
  const doc = parseToml(`name = "x"

[guards]
complete = "every field is filled in"

[[rules]]
when = { role = "admin", verified = "-" }
then = { promote = "yes" }

[[rules]]
when = { role = "guest", verified = false }
then = {}`);
  assert.deepEqual(doc, {
    name: 'x',
    guards: { complete: 'every field is filled in' },
    rules: [
      { when: { role: 'admin', verified: '-' }, then: { promote: 'yes' } },
      { when: { role: 'guest', verified: false }, then: {} },
    ],
  });
});

test('accepts quoted keys', () => {
  assert.deepEqual(parseToml('"two words" = 1'), { 'two words': 1 });
});

test('errors name the line', () => {
  assert.throws(() => parseToml('a = 1\na = 2'), /line 2: duplicate key "a"/);
  assert.throws(() => parseToml('a = "open'), /line 1: unterminated string/);
  assert.throws(() => parseToml('a = 1\n[t]\n[t]'), /line 3: table "t" is defined twice/);
  assert.throws(() => parseToml('a = 1979-05-27'), /line 1: unsupported value/);
  assert.throws(() => parseToml('a.b = 1'), /line 1: dotted keys are not supported/);
  assert.throws(() => parseToml('[a.b]'), /line 1: dotted keys are not supported/);
  assert.throws(() => parseToml('a = 1 b = 2'), /line 1: expected end of line/);
  assert.throws(() => parseToml('a = [1, 2'), /line 1: unterminated array/);
  assert.throws(() => parseToml('a = """x"""'), /line 1: multi-line strings are not supported/);
});

test('an array of tables cannot reuse a plain key', () => {
  assert.throws(() => parseToml('rules = 1\n[[rules]]'), /line 2: "rules" is not an array of tables/);
});
