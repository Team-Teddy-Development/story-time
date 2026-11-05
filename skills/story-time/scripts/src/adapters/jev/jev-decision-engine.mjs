// Jev-driven judgement. One batched request per story. Whenever Jev cannot
// give a trustworthy answer (no key, auth, outage, canary, egress, protocol),
// the fallback engine decides and the judgement records why.
import { interpretAnswers, judgeQuestions, judgeState, validateQuestions } from '../../domain/judgement.mjs';
import { assertDecisionEngine } from '../../ports/decision-engine.mjs';
import { JEV_INPUT_USD_PER_MTOK } from './jev-client.mjs';

export class JevDecisionEngine {
  constructor({ client, fallback }) {
    this.name = 'jev';
    this.client = client;
    this.fallback = assertDecisionEngine(fallback);
  }

  async judge(story) {
    const questions = validateQuestions(judgeQuestions(story));
    const r = await this.client.decide({ state: judgeState(story), questions });
    if (!r.ok) {
      const j = await this.fallback.judge(story);
      return { ...j, source: 'fallback', reason: r.reason };
    }
    const costUsd = Math.round((r.usage.inputTokens / 1e6) * JEV_INPUT_USD_PER_MTOK * 1e10) / 1e10;
    return {
      id: story.id,
      source: 'jev',
      model: r.model,
      usage: { ...r.usage, costUsd },
      decisions: interpretAnswers(story, r.answers).decisions,
    };
  }
}
