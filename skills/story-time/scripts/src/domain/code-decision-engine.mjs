// Code-driven judgement: deterministic heuristics, used when no TypeSafe key
// is configured, and as the fallback whenever Jev cannot answer. Confidence
// values are deliberately modest: these are rules of thumb, not a model.
import { acceptanceCriteria } from './story.mjs';
import { GHERKIN, STATEMENT, VAGUE_RE } from './invest.mjs';
import { FIB, needsSplit } from './judgement.mjs';

const SOLUTION = /\b(button|dropdown|drop-down|checkbox|radio button|modal|pop-?up|tooltip|database|db|table|column|schema|endpoint|api|microservice|redis|postgres|mysql|sql|mongo(?:db)?|react|angular|vue|kafka|lambda|cron(?:job)?|webhook|json)\b/i;
const CIRCULAR = /\b(use|have|access) (the|this|that) (feature|functionality|app|application|system|tool)\b/i;
const STOP = new Set(['i', 'can', 'to', 'my', 'a', 'an', 'the', 'be', 'able', 'so', 'that', 'we', 'our', 'it', 'is', 'and', 'of', 'for', 'me', 'will', 'get', 'want', 'need']);
const BUG = /\b(bug|crash(?:es|ed|ing)?|broken|error|errors|fails?|failing|failed|regression|incorrect|wrong|throws?|throwing)\b/i;
const SPIKE = /\b(investigate|research|spike|evaluate|explore|prototype|feasib\w*|whether)\b/i;
const CHORE = /\b(upgrade|bump|refactor|migrate|ci|lint|dependency|dependencies|cleanup|clean up)\b/i;

const words = (s) => (s.toLowerCase().match(/[a-z0-9]+/g) ?? []).filter((w) => !STOP.has(w));

function sizeFor(n) {
  if (n <= 1) return 1;
  if (n === 2) return 2;
  if (n === 3) return 3;
  if (n <= 5) return 5;
  if (n <= 8) return 8;
  if (n <= 12) return 13;
  return FIB[FIB.length - 1];
}

export class CodeDecisionEngine {
  constructor() {
    this.name = 'code';
  }

  async judge(story) {
    const spike = story.type === 'spike';
    const statement = (spike ? story.sections?.Question : story.sections?.Story) ?? '';
    const acs = acceptanceCriteria(story);
    const decisions = {};

    if (!spike) {
      const m = STATEMENT.exec(statement);
      const goal = m?.[2] ?? statement;
      const solution = SOLUTION.exec(goal);
      decisions.negotiable = solution
        ? { value: false, confidence: 0.6, reason: `names a solution: "${solution[1]}"` }
        : { value: true, confidence: 0.5 };

      const benefit = m?.[3];
      if (!benefit) decisions.valuable = { value: false, confidence: 0.7, reason: 'no "so that" benefit' };
      else if (CIRCULAR.test(benefit)) decisions.valuable = { value: false, confidence: 0.6, reason: 'the benefit is "using the feature"' };
      else {
        const goalWords = new Set(words(goal));
        const benefitWords = words(benefit);
        const restated = benefitWords.length > 0 && benefitWords.every((w) => goalWords.has(w));
        decisions.valuable = restated
          ? { value: false, confidence: 0.6, reason: 'the benefit restates the goal' }
          : { value: true, confidence: 0.5 };
      }
    }

    decisions.criteria = acs.map((ac, i) => {
      const testable = GHERKIN.test(ac.text) && !VAGUE_RE.test(ac.text);
      return { index: i + 1, text: ac.text, testable, confidence: 0.5 };
    });

    const text = `${story.title ?? ''} ${statement}`;
    const type = BUG.test(text) ? 'bug' : SPIKE.test(text) ? 'spike' : CHORE.test(text) ? 'chore' : 'story';
    decisions.type = { value: type, confidence: 0.55 };

    if (!spike) {
      decisions.size = { value: sizeFor(acs.length), confidence: 0.3, reason: `${acs.length} acceptance criteria` };
      if (needsSplit(decisions, acs.length)) {
        const pattern = / and /i.test(STATEMENT.exec(statement)?.[2] ?? '') ? 'workflow-steps' : acs.length > 8 ? 'paths' : 'simple-complex';
        decisions.splitPattern = { value: pattern, confidence: 0.3 };
      }
    }

    return { id: story.id, source: 'code', decisions };
  }
}
