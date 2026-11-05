// DecisionEngine port: makes the judgement calls about a story that the
// INVEST regex lint cannot (need vs solution, real benefit, testable criteria,
// type, size, split pattern). Strategies: code heuristics, or Jev.
//
// interface DecisionEngine {
//   judge(story): Promise<Judgement>
// }
// Judgement = { id, source: 'code' | 'jev' | 'fallback', reason?, model?, usage?,
//               decisions: { negotiable?, valuable?, criteria[], type, size?, splitPattern? } }
// Every decision carries { value, confidence }, confidence in 0..1.

export function assertDecisionEngine(engine) {
  const missing = ['judge'].filter((m) => typeof engine?.[m] !== 'function');
  if (missing.length) throw new TypeError(`Not a DecisionEngine; missing: ${missing.join(', ')}`);
  return engine;
}
