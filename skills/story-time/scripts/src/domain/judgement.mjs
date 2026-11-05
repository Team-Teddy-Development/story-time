// Story judgement: the decisions story-time makes about a story, expressed as
// typed questions (for Jev) and read back from typed answers. Pure.
//
// Decision kinds follow Jev's System One types:
//   noul   - yes/no as a probability of yes
//   choice - one of a fixed set of options
//   score  - a probability-weighted position on an ordered scale
import { acceptanceCriteria } from './story.mjs';

export const FIB = [1, 2, 3, 5, 8, 13, 20];

export const SPLIT_PATTERNS = {
  'workflow-steps': 'split by the steps of the user journey, one step per story',
  paths: 'ship the happy path first, then each alternate path as its own story',
  'business-rules': 'one business rule or rule variation per story',
  'data-variations': 'one kind of data, input, or source at a time',
  interfaces: 'one channel, device, or interface at a time',
  'simple-complex': 'the simplest useful version first, then the harder cases',
  operations: 'split create, read, update, and delete into separate stories',
  'defer-quality': 'make it work first, then performance, accessibility, or localisation',
  'relax-rules': 'ship without a constraint first, then add the constraint',
  spike: 'pull the unknown out into a time-boxed research spike',
};

const TYPE_OPTIONS = {
  story: 'new or changed behaviour that gives a user or the business a benefit',
  bug: 'existing behaviour that is broken and should be fixed',
  spike: 'time-boxed research that answers a question before anything is built',
  chore: 'technical upkeep with no direct change a user would notice',
};

const SIZE_LEVELS = [
  '1 point: trivial, one obvious change with a single acceptance criterion',
  '2 points: small, a couple of criteria in one familiar part of the product',
  '3 points: modest, a few criteria, still well understood',
  '5 points: medium, several criteria or a few parts of the product',
  '8 points: large, many criteria or unfamiliar areas; probably worth splitting',
  '13 points: very large, too big to finish in one sprint without splitting',
  '20 points: epic sized, must be broken into several stories',
];

const LABEL_WORDS = new Set(['yes', 'no', 'true', 'false', 'met', 'not met', 'implemented', 'present', 'n/a', 'ok', 'done', 'pass', 'fail']);

const isSpike = (s) => s.type === 'spike';
const round2 = (n) => Math.round(n * 100) / 100;

export function judgeState(story) {
  const criteria = acceptanceCriteria(story).map((ac) => ac.text);
  return {
    title: story.title ?? '',
    persona: story.persona ?? null,
    statement: (isSpike(story) ? story.sections?.Question : story.sections?.Story)?.trim() ?? '',
    criteria,
    count: criteria.length,
  };
}

export function judgeQuestions(story) {
  const q = {};
  if (!isSpike(story)) {
    q.negotiable = {
      type: 'noul',
      instructions: 'Read state.statement, a user story written as "As a <persona>, I want <goal>, so that <benefit>."',
      criteria: {
        true: 'The statement describes what the persona needs and why, without prescribing a specific user interface control, data store, technology, or implementation.',
        false: 'The statement prescribes a specific solution, such as a named user interface control, database table, API endpoint, framework, or other implementation detail.',
      },
    };
    q.valuable = {
      type: 'noul',
      instructions: 'Read the "so that" part of state.statement, which should say why the persona wants the goal.',
      criteria: {
        true: 'The "so that" part names a concrete gain for the persona or the business that is different from the goal itself.',
        false: 'The "so that" part is missing, merely restates the goal, or names no concrete gain for anyone.',
      },
    };
  }
  acceptanceCriteria(story).forEach((ac, i) => {
    q[`criterion_${i + 1}`] = {
      type: 'noul',
      instructions: `Judge this acceptance criterion of the story in state: "${ac.text}"`,
      criteria: {
        true: 'The criterion describes one observable outcome with concrete values, so an automated test could clearly pass or fail it.',
        false: 'The criterion is vague, subjective, bundles several outcomes, or could not clearly pass or fail a test.',
      },
    };
  });
  q.type = { type: 'choice', instructions: 'Classify the work described by state.title and state.statement.', criteria: { ...TYPE_OPTIONS } };
  if (!isSpike(story)) {
    q.size = {
      type: 'score',
      instructions: 'Estimate the relative effort of this story from state.statement and state.criteria, as a team would in planning poker.',
      criteria: [...SIZE_LEVELS],
    };
    q.split_pattern = {
      type: 'choice',
      instructions: 'If this story had to be split into smaller vertical slices, which splitting pattern fits it best?',
      criteria: { ...SPLIT_PATTERNS },
    };
  }
  return q;
}

// Guards against question shapes Jev is known to answer with a constant
// (label-like noul branches) or reject with an unhelpful 400.
export function validateQuestions(questions) {
  for (const [id, q] of Object.entries(questions)) {
    if (!q || typeof q.instructions !== 'string' || !q.instructions.trim()) throw new Error(`Question ${id}: instructions are required.`);
    if (q.type === 'noul') {
      const keys = Object.keys(q.criteria ?? {});
      if (keys.length !== 2 || !keys.includes('true') || !keys.includes('false')) throw new Error(`Question ${id}: noul criteria must have exactly the keys true and false.`);
      for (const branch of Object.values(q.criteria)) {
        if (typeof branch !== 'string' || LABEL_WORDS.has(branch.trim().toLowerCase()) || branch.trim().split(/\s+/).length < 4) {
          throw new Error(`Question ${id}: "${branch}" is a label, not a proposition. Write a full sentence for each branch.`);
        }
      }
    } else if (q.type === 'choice') {
      const n = Object.keys(q.criteria ?? {}).length;
      if (n < 2 || n > 255) throw new Error(`Question ${id}: a choice needs 2 to 255 options.`);
    } else if (q.type === 'score') {
      if (!Array.isArray(q.criteria) || q.criteria.length < 2 || q.criteria.length > 10) throw new Error(`Question ${id}: a score needs 2 to 10 ordered levels.`);
    } else {
      throw new Error(`Question ${id}: unknown type "${q.type}". Use noul, choice, or score.`);
    }
  }
  return questions;
}

export function needsSplit(decisions, criteriaCount) {
  return (decisions.size?.value ?? 0) >= 8 || criteriaCount > 8;
}

export function interpretAnswers(story, answers, { threshold = 0.5 } = {}) {
  const noul = (a) => ({ value: a.noul >= threshold, confidence: round2(Math.abs(2 * a.noul - 1)) });
  const decisions = {};
  if (answers.negotiable) decisions.negotiable = noul(answers.negotiable);
  if (answers.valuable) decisions.valuable = noul(answers.valuable);
  decisions.criteria = acceptanceCriteria(story).map((ac, i) => {
    const d = noul(answers[`criterion_${i + 1}`]);
    return { index: i + 1, text: ac.text, testable: d.value, confidence: d.confidence };
  });
  decisions.type = { value: answers.type.choice, confidence: round2(answers.type.confidence) };
  if (answers.size) {
    const idx = Math.min(FIB.length - 1, Math.max(0, Math.round(answers.size.score)));
    decisions.size = { value: FIB[idx], confidence: round2(answers.size.confidence ?? 0) };
  }
  if (answers.split_pattern && needsSplit(decisions, decisions.criteria.length)) {
    decisions.splitPattern = { value: answers.split_pattern.choice, confidence: round2(answers.split_pattern.confidence) };
  }
  return { decisions };
}
