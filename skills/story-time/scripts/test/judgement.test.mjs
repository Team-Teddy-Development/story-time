import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  FIB, SPLIT_PATTERNS, judgeState, judgeQuestions, validateQuestions, interpretAnswers, needsSplit,
} from '../src/domain/judgement.mjs';
import { CodeDecisionEngine } from '../src/domain/code-decision-engine.mjs';
import { assertDecisionEngine } from '../src/ports/decision-engine.mjs';
import { parseStory } from '../src/domain/story.mjs';
import { READY_MD, story } from './fixtures.mjs';

const ready = () => parseStory(READY_MD);

// --- the questions Jev is asked ---

test('judgeState sends only the story fields a decision needs', () => {
  assert.deepEqual(Object.keys(judgeState(ready())).sort(), ['count', 'criteria', 'persona', 'statement', 'title']);
  assert.equal(judgeState(ready()).count, 2);
});

test('judgeQuestions batches every decision about a story into one request', () => {
  const q = judgeQuestions(ready());
  assert.deepEqual(Object.keys(q), ['negotiable', 'valuable', 'criterion_1', 'criterion_2', 'type', 'size', 'split_pattern']);
  assert.equal(q.negotiable.type, 'noul');
  assert.equal(q.type.type, 'choice');
  assert.deepEqual(Object.keys(q.type.criteria), ['story', 'bug', 'spike', 'chore']);
  assert.equal(q.size.type, 'score');
  assert.equal(q.size.criteria.length, FIB.length, 'one level per Fibonacci size, lowest first');
  assert.deepEqual(Object.keys(q.split_pattern.criteria), Object.keys(SPLIT_PATTERNS));
});

test('each criterion question names its own criterion, because question ids never reach the model', () => {
  const q = judgeQuestions(ready());
  assert.match(q.criterion_2.instructions, /expired link/);
});

test('noul branches are concrete propositions under exactly true and false', () => {
  for (const [id, q] of Object.entries(judgeQuestions(ready()))) {
    if (q.type !== 'noul') continue;
    assert.deepEqual(Object.keys(q.criteria), ['true', 'false'], id);
    for (const branch of Object.values(q.criteria)) assert.ok(branch.split(' ').length >= 6, `${id}: "${branch}" is a label, not a proposition`);
  }
});

test('spikes are not judged as user statements', () => {
  const spike = story({ type: 'spike', timebox: '1d', sections: { Question: 'Can X do 3DS?', 'Exit Criteria': '- [ ] Decision recorded' } });
  const q = judgeQuestions(spike);
  assert.ok(!('negotiable' in q) && !('valuable' in q));
  assert.ok('criterion_1' in q && 'type' in q && !('size' in q));
});

test('validateQuestions rejects shapes Jev answers with a constant or a 400', () => {
  assert.throws(() => validateQuestions({ a: { type: 'noul', instructions: 'x', criteria: { true: 'yes', false: 'no' } } }), /label/);
  assert.throws(() => validateQuestions({ a: { type: 'noul', instructions: 'x', criteria: { yes: 'a b c d e f', no: 'a b c d e f' } } }), /true.*false/);
  assert.throws(() => validateQuestions({ a: { type: 'choice', instructions: 'x', criteria: { only: 'one' } } }), /2 to 255/);
  assert.throws(() => validateQuestions({ a: { type: 'score', instructions: 'x', criteria: ['one'] } }), /2 to 10/);
  assert.throws(() => validateQuestions({ a: { type: 'essay', instructions: 'x', criteria: {} } }), /type/);
  assert.doesNotThrow(() => validateQuestions(judgeQuestions(ready())));
});

// --- reading Jev's answers back ---

const answers = (over = {}) => ({
  negotiable: { type: 'noul', noul: 0.9 },
  valuable: { type: 'noul', noul: 0.2 },
  criterion_1: { type: 'noul', noul: 0.8 },
  criterion_2: { type: 'noul', noul: 0.4 },
  type: { type: 'choice', choice: 'story', probabilities: { story: 0.85, bug: 0.1, spike: 0.03, chore: 0.02 }, confidence: 0.85 },
  size: { type: 'score', score: 2.4, probabilities: {}, legend: {}, confidence: 0.6 },
  split_pattern: { type: 'choice', choice: 'paths', probabilities: { paths: 0.7 }, confidence: 0.7 },
  ...over,
});

test('interpretAnswers turns probabilities into decisions at the 0.5 threshold', () => {
  const d = interpretAnswers(ready(), answers()).decisions;
  assert.deepEqual(d.negotiable, { value: true, confidence: 0.8 });
  assert.deepEqual(d.valuable, { value: false, confidence: 0.6 });
  assert.deepEqual(d.criteria.map((c) => c.testable), [true, false]);
  assert.equal(d.criteria[1].text.startsWith('Given an expired link'), true);
  assert.deepEqual(d.type, { value: 'story', confidence: 0.85 });
});

test('a probability-weighted score maps to the nearest Fibonacci size', () => {
  assert.equal(interpretAnswers(ready(), answers()).decisions.size.value, 3); // 2.4 -> index 2 -> 3
  assert.equal(interpretAnswers(ready(), answers({ size: { type: 'score', score: 4.6, confidence: 0.5 } })).decisions.size.value, 13);
});

test('the split pattern is reported only when the story needs splitting', () => {
  assert.equal(interpretAnswers(ready(), answers()).decisions.splitPattern, undefined);
  const big = interpretAnswers(ready(), answers({ size: { type: 'score', score: 4, confidence: 0.6 } })).decisions;
  assert.deepEqual(big.splitPattern, { value: 'paths', confidence: 0.7 });
  assert.equal(needsSplit({ size: { value: 5 } }, 9), true, 'more than 8 criteria also needs a split');
});

// --- the code engine (no key) ---

test('CodeDecisionEngine implements the port and marks every decision as code', async () => {
  const engine = new CodeDecisionEngine();
  assertDecisionEngine(engine);
  const j = await engine.judge(ready());
  assert.equal(j.source, 'code');
  assert.equal(j.id, 'ST-001');
  assert.equal(j.decisions.negotiable.value, true);
  assert.equal(j.decisions.valuable.value, true);
  assert.deepEqual(j.decisions.criteria.map((c) => c.testable), [true, true]);
  assert.equal(j.decisions.type.value, 'story');
});

test('code engine flags solution words in the statement as not negotiable', async () => {
  const s = story({ sections: { ...story().sections, Story: 'As a shopper, I want a dropdown backed by a Redis table, so that I can pick a size.' } });
  const d = (await new CodeDecisionEngine().judge(s)).decisions;
  assert.equal(d.negotiable.value, false);
  assert.match(d.negotiable.reason, /dropdown/);
});

test('code engine flags a circular or missing benefit', async () => {
  const circular = story({ sections: { ...story().sections, Story: 'As a shopper, I want to save my cart, so that I can save my cart.' } });
  assert.equal((await new CodeDecisionEngine().judge(circular)).decisions.valuable.value, false);
  const vague = story({ sections: { ...story().sections, Story: 'As a shopper, I want saved carts, so that I can use the feature.' } });
  assert.equal((await new CodeDecisionEngine().judge(vague)).decisions.valuable.value, false);
});

test('code engine marks vague or non-Gherkin criteria untestable', async () => {
  const s = story({ sections: { ...story().sections, 'Acceptance Criteria': '- [ ] Given a cart, when saved, then it loads fast\n- [ ] works well' } });
  assert.deepEqual((await new CodeDecisionEngine().judge(s)).decisions.criteria.map((c) => c.testable), [false, false]);
});

test('code engine classifies type from wording', async () => {
  const judge = async (title, statement) =>
    (await new CodeDecisionEngine().judge(story({ title, type: undefined, sections: { Story: statement } }))).decisions.type.value;
  assert.equal(await judge('Checkout crashes on submit', 'As a shopper, I want checkout to stop throwing an error, so that I can pay.'), 'bug');
  assert.equal(await judge('Investigate payment providers', 'As a team, we need to evaluate whether provider X supports 3DS, so that we can choose.'), 'spike');
  assert.equal(await judge('Upgrade Node to 22', 'As a maintainer, I want to bump the runtime dependency, so that we stay supported.'), 'chore');
  assert.equal(await judge('Save cart', story().sections.Story), 'story');
});

test('code engine sizes by criteria count, with low confidence', async () => {
  const many = Array.from({ length: 9 }, (_, i) => `- [ ] Given ${i}, when x, then y`).join('\n');
  const d = (await new CodeDecisionEngine().judge(story({ sections: { ...story().sections, 'Acceptance Criteria': many } }))).decisions;
  assert.equal(d.size.value, 13);
  assert.ok(d.size.confidence <= 0.4);
  assert.ok(d.splitPattern, 'a 13 needs a split pattern');
});

test('assertDecisionEngine rejects objects without judge()', () => {
  assert.throws(() => assertDecisionEngine({}), /missing: judge/);
});
