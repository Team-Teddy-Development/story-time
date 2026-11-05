// Strategy selection: no key means code-driven decisions; a key means Jev,
// with code heuristics as the per-call fallback. STORY_TIME_JEV=off overrides.
import { CodeDecisionEngine } from '../domain/code-decision-engine.mjs';
import { JevClient, JEV_DEFAULT_MODEL } from '../adapters/jev/jev-client.mjs';
import { JevDecisionEngine } from '../adapters/jev/jev-decision-engine.mjs';

const OFF = new Set(['off', '0', 'false', 'no']);

export function selectDecisionEngine({ apiKey, env = process.env, config = {}, fetch, warn } = {}) {
  const code = new CodeDecisionEngine();
  if (OFF.has(String(env.STORY_TIME_JEV ?? '').trim().toLowerCase())) {
    return { engine: code, mode: 'code', why: 'disabled by STORY_TIME_JEV' };
  }
  if (!String(apiKey ?? '').trim()) {
    return { engine: code, mode: 'code', why: 'no TypeSafe key; run story secret set jev to use Jev' };
  }
  const model = config.jev?.model ?? JEV_DEFAULT_MODEL;
  const client = new JevClient({ apiKey, model, ...(fetch ? { fetch } : {}), ...(warn ? { warn } : {}) });
  return { engine: new JevDecisionEngine({ client, fallback: code }), mode: 'jev', model, why: 'TypeSafe key configured' };
}
