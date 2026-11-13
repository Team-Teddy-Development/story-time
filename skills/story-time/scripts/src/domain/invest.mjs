// INVEST + Definition of Ready lint. Each rule is a small object with a
// single responsibility; lintStory composes them (open/closed: add rules,
// don't edit the linter). Rules are pure: (story, context) -> findings.
import { acceptanceCriteria } from './story.mjs';

export const STATEMENT = /^\s*As (?:an?\s+|the\s+)?(.+?),\s*I (?:want|need|can)\s+(.+?)(?:,?\s*so that\s+(.+?))?\.?\s*$/is;
const GENERIC_PERSONA = /^(?:end[-\s]?)?(?:user|someone|anyone|somebody|person|people|customer|visitor)s?$/i;
export const GHERKIN = /^Given\b.+\bwhen\b.+\bthen\b/is;
const VAGUE = [
  'fast', 'quickly', 'easy', 'easily', 'simple', 'intuitive', 'user-friendly', 'seamless',
  'robust', 'properly', 'correctly', 'appropriate', 'etc', 'and so on', 'as needed', 'should work',
  'efficient', 'flexible', 'nice',
];
export const VAGUE_RE = new RegExp(`\\b(${VAGUE.map((w) => w.replace(/[-\s]/g, '[-\\s]')).join('|')})\\b`, 'i');

const isSpike = (s) => s.type === 'spike';
const isOneCriterionChore = (s) => s.type === 'chore' && acceptanceCriteria(s).length === 1;
const CLOSED = new Set(['done', 'split', 'archived']);
const needsStatement = (s) => !s.type || s.type === 'story';
const isNum = (v) => typeof v === 'number' && Number.isFinite(v);

export const SIZE_WARN = 8;
export const SIZE_MAX = 13;
export const AC_MAX = 8;

const rule = (id, invest, check) => ({ id, invest, check });

export const DEFAULT_RULES = [
  rule('title.present', 'N', (s) => (s.title?.trim() ? [] : [err('Story has no title.')])),

  rule('statement.format', 'V', (s) => {
    if (!needsStatement(s)) return [];
    const text = s.sections?.Story ?? '';
    return STATEMENT.test(text) ? [] : [err('"## Story" must read "As a <persona>, I want <goal>, so that <benefit>."')];
  }),

  rule('statement.benefit', 'V', (s) => {
    if (!needsStatement(s)) return [];
    const m = STATEMENT.exec(s.sections?.Story ?? '');
    return m && !m[3] ? [err('Statement has no "so that <benefit>". Without the why, value cannot be judged.')] : [];
  }),

  rule('statement.persona', 'V', (s) => {
    if (!needsStatement(s)) return [];
    const m = STATEMENT.exec(s.sections?.Story ?? '');
    return m && GENERIC_PERSONA.test(m[1].trim())
      ? [warn(`Generic persona "${m[1].trim()}". Name who benefits (e.g. "billing admin", "first-time buyer").`)]
      : [];
  }),

  rule('ac.present', 'T', (s) => {
    if (isSpike(s)) return [];
    return acceptanceCriteria(s).length ? [] : [err('No acceptance criteria. A story nobody can test cannot be done.')];
  }),

  rule('ac.gherkin', 'T', (s) => {
    if (isSpike(s)) return [];
    const bad = acceptanceCriteria(s).filter((ac) => !GHERKIN.test(ac.text));
    return bad.map((ac) => warn(`Criterion is not Given/When/Then: "${ac.text}"`));
  }),

  rule('ac.vague', 'T', (s) =>
    acceptanceCriteria(s).flatMap((ac) => {
      const m = VAGUE_RE.exec(ac.text);
      return m ? [warn(`Vague word "${m[1]}" in "${ac.text}". Replace it with a measurable threshold.`)] : [];
    }),
  ),

  rule('ac.count', 'S', (s) => {
    const n = acceptanceCriteria(s).length;
    return n > AC_MAX ? [warn(`${n} acceptance criteria (> ${AC_MAX}). The story is probably several stories; consider story-split.`)] : [];
  }),

  rule('size.estimated', 'E', (s) =>
    isSpike(s) || isOneCriterionChore(s) || isNum(s.size) ? [] : [err('Not estimated. Run story-estimate.')],
  ),

  rule('size.small', 'S', (s) => {
    if (!isNum(s.size)) return [];
    if (s.size >= SIZE_MAX) return [err(`Size ${s.size} >= ${SIZE_MAX}: too big to finish in a sprint. Split it.`)];
    if (s.size >= SIZE_WARN) return [warn(`Size ${s.size} is large. Look for a split before committing.`)];
    return [];
  }),

  rule('value.scored', 'V', (s) => {
    if (isSpike(s)) return [];
    const missing = ['value', 'time_criticality', 'risk_reduction'].filter((k) => !isNum(s[k]));
    return missing.length ? [warn(`Missing WSJF inputs: ${missing.join(', ')}. story-pick cannot rank it.`)] : [];
  }),

  rule('independent.deps', 'I', (s, ctx) => {
    if (!ctx.stories || !s.depends_on?.length) return [];
    const byId = new Map(ctx.stories.map((x) => [x.id, x]));
    return s.depends_on.flatMap((id) => {
      const dep = byId.get(id);
      if (!dep) return [err(`Depends on ${id}, which does not exist.`)];
      return dep.status === 'done' ? [] : [warn(`Depends on ${id} (${dep.status}). Not independent until it is done.`)];
    });
  }),

  rule('spike.timebox', 'E', (s) => (isSpike(s) && !s.timebox ? [err('Spike has no timebox (e.g. timebox: 2d).')] : [])),

  rule('spike.question', 'T', (s) =>
    isSpike(s) && !s.sections?.Question?.trim() ? [err('Spike has no "## Question" to answer.')] : [],
  ),

  rule('spike.unblocks', 'V', (s, ctx) => {
    if (!isSpike(s) || CLOSED.has(s.status) || !ctx.stories) return [];
    return ctx.stories.some((x) => x.depends_on?.includes(s.id))
      ? []
      : [err(`No story lists ${s.id} in depends_on. A spike must unblock a named story: link it, or archive the spike.`)];
  }),

  rule('state_machine.exists', 'T', (s, ctx) =>
    s.state_machine && ctx.exists && !ctx.exists(s.state_machine)
      ? [err(`state_machine: ${s.state_machine} does not exist (paths are relative to the repo root). Write it with the state-machine skill.`)]
      : [],
  ),
];

// ctx.stories is the whole backlog; ctx.exists(path) answers for a repo-root-relative path.
// Rules that need either skip when it is absent.
export function lintStory(story, { rules = DEFAULT_RULES, stories, exists } = {}) {
  const ctx = { stories, exists };
  const findings = rules.flatMap((r) =>
    r.check(story, ctx).map((f) => ({ rule: r.id, invest: r.invest, ...f })),
  );
  return { id: story.id, findings, ready: !findings.some((f) => f.severity === 'error') };
}

function err(message) {
  return { severity: 'error', message };
}
function warn(message) {
  return { severity: 'warning', message };
}
