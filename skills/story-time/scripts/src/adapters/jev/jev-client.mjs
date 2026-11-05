// Jev (TypeSafe AI, System One) client. Zero dependencies; Node 20+ fetch.
//
// Contract follows the integration already proven in production at
// Team-Teddy-Development/varsity-engine (lib/pdca/jev-client.ts):
//   POST https://api.typesafe.ai/v1/systemone  { state, model, questions }
//   Authorization: Bearer <key>; redirects refused; strict response decoding;
//   408/429/5xx/timeout/transport retried (3 attempts); 401/403 latch for the run;
//   422 never retried; a contradiction canary before the first real call.
//
// decide() never throws for runtime failures. It returns
//   { ok: true, source: 'jev', model, answers, usage, attempts }
//   { ok: false, reason, attempts?, findings? }
// so callers can always fall back to code heuristics.
import { assertVendorEndpoint, scanPayload } from './jev-egress.mjs';

export const JEV_ENDPOINT = 'https://api.typesafe.ai/v1/systemone';
export const JEV_DEFAULT_MODEL = 'jev-1.13.0';
export const JEV_INPUT_USD_PER_MTOK = 0.042;

export const JEV_DEFAULT_RETRY = {
  maxAttempts: 3,
  baseDelayMs: 500,
  maxDelayMs: 5_000,
  jitterRatio: 0.25,
  maxRetryAfterMs: 30_000,
  requestTimeoutMs: 10_000,
};

const CANARY_STATE = { statement: 'As a shopper, I want to save my cart, so that I can finish checkout later.' };
const CANARY_QUESTIONS = {
  canary_true: {
    type: 'noul',
    instructions: 'Read state.statement.',
    criteria: { true: 'The statement says why the shopper wants to save the cart.', false: 'The statement gives no reason for wanting to save the cart.' },
  },
  canary_false_database: {
    type: 'noul',
    instructions: 'Read state.statement.',
    criteria: { true: 'The statement names a specific database or storage technology.', false: 'The statement does not name any database or storage technology.' },
  },
  canary_false_bicycle: {
    type: 'noul',
    instructions: 'Read state.statement.',
    criteria: { true: 'The statement is about repairing or riding a bicycle.', false: 'The statement has nothing to do with bicycles.' },
  },
};
const CANARY_MIN_SPREAD = 0.3;

const sleepFor = (ms) => new Promise((r) => setTimeout(r, ms));
const isProb = (n) => typeof n === 'number' && Number.isFinite(n) && n >= 0 && n <= 1;
const isCount = (n) => Number.isSafeInteger(n) && n >= 0;

export class JevClient {
  constructor({
    apiKey, model = JEV_DEFAULT_MODEL, endpoint = JEV_ENDPOINT, fetch = globalThis.fetch,
    retry = JEV_DEFAULT_RETRY, sleep = sleepFor, random = Math.random,
    warn = (m) => process.emitWarning(m, 'Jev'), canary = true,
  }) {
    this.endpoint = assertVendorEndpoint(endpoint);
    this.apiKey = String(apiKey ?? '').trim();
    this.model = model;
    this.fetch = fetch;
    this.retry = { ...JEV_DEFAULT_RETRY, ...retry };
    this.sleep = sleep;
    this.random = random;
    this.warn = warn;
    this.canaryState = canary ? 'pending' : 'passed';
    this.authRejected = false;
    this.warned = new Set();
  }

  async decide({ state, questions }) {
    if (!this.apiKey) return { ok: false, reason: 'unconfigured' };
    if (this.authRejected) return { ok: false, reason: 'auth_rejected' };
    if (this.canaryState === 'failed') return { ok: false, reason: 'canary_failed' };
    if (this.canaryState === 'pending') {
      const blocked = await this.runCanary();
      if (blocked) return blocked;
    }
    return this.send(state, questions);
  }

  async runCanary() {
    const r = await this.send(CANARY_STATE, CANARY_QUESTIONS);
    // An outage is not evidence about the model: try the canary again next call.
    if (!r.ok) return r;
    const p = Object.fromEntries(Object.entries(r.answers).map(([k, a]) => [k, a.noul]));
    const values = Object.values(p);
    const spread = Math.max(...values) - Math.min(...values);
    const correct = p.canary_true >= 0.5 && p.canary_false_database < 0.5 && p.canary_false_bicycle < 0.5;
    if (correct && spread >= CANARY_MIN_SPREAD) {
      this.canaryState = 'passed';
      return null;
    }
    this.canaryState = 'failed';
    this.warnOnce('canary', 'Jev failed its contradiction check (it answered unrelated questions alike). Jev is off for this run; code heuristics decide.');
    return { ok: false, reason: 'canary_failed' };
  }

  async send(state, questions) {
    // Snapshot first, check the snapshot, send exactly what was checked.
    const payload = JSON.parse(JSON.stringify({ state, model: this.model, questions }));
    const findings = scanPayload({ state: payload.state, questions: payload.questions });
    if (findings.length) return { ok: false, reason: 'egress_refused', findings };
    const body = JSON.stringify(payload);

    const { maxAttempts } = this.retry;
    for (let attempt = 1; attempt <= maxAttempts; attempt++) {
      let res;
      try {
        res = await this.fetch(this.endpoint, {
          method: 'POST',
          headers: { authorization: `Bearer ${this.apiKey}`, 'content-type': 'application/json' },
          body,
          redirect: 'error',
          signal: AbortSignal.timeout(this.retry.requestTimeoutMs),
        });
      } catch (e) {
        const reason = e?.name === 'TimeoutError' || e?.name === 'AbortError' ? 'timeout' : 'transport_error';
        if (attempt < maxAttempts) {
          await this.sleep(this.backoff(attempt));
          continue;
        }
        return { ok: false, reason, attempts: attempt };
      }

      if (res.status >= 200 && res.status < 300) {
        let json;
        try {
          json = JSON.parse(await res.text());
        } catch {
          return { ok: false, reason: 'protocol_error', attempts: attempt };
        }
        const decoded = decodeResponse(questions, json);
        if (!decoded) return { ok: false, reason: 'protocol_error', attempts: attempt };
        return { ok: true, source: 'jev', ...decoded, attempts: attempt };
      }
      if (res.status === 401 || res.status === 403) {
        this.authRejected = true;
        this.warnOnce('auth', `TypeSafe rejected the API key (HTTP ${res.status}). Jev is off for this run; code heuristics decide. Check it with: story secret status`);
        return { ok: false, reason: 'auth_rejected', attempts: attempt };
      }
      if (res.status === 422) return { ok: false, reason: 'validation_error', attempts: attempt };
      if (res.status === 408 || res.status === 429 || res.status >= 500) {
        const reason = res.status === 429 ? 'rate_limited' : 'overloaded';
        if (attempt < maxAttempts) {
          await this.sleep(this.retryHint(res.headers) ?? this.backoff(attempt));
          continue;
        }
        return { ok: false, reason, attempts: attempt };
      }
      return { ok: false, reason: 'protocol_error', attempts: attempt };
    }
    return { ok: false, reason: 'transport_error', attempts: maxAttempts };
  }

  backoff(attempt) {
    const { baseDelayMs, maxDelayMs, jitterRatio } = this.retry;
    const delay = Math.min(maxDelayMs, baseDelayMs * 2 ** (attempt - 1));
    return Math.max(1, Math.round(delay + (this.random() * 2 - 1) * jitterRatio * delay));
  }

  // retry-after-ms first, then retry-after in seconds; both clamped. An absent
  // or unparseable header returns undefined so the caller backs off, never 0.
  retryHint(headers) {
    const clamp = (ms) => Math.min(this.retry.maxRetryAfterMs, Math.max(0, ms));
    const ms = Number(headers?.get?.('retry-after-ms'));
    if (headers?.get?.('retry-after-ms') != null && Number.isFinite(ms)) return clamp(ms);
    const s = Number(headers?.get?.('retry-after'));
    if (headers?.get?.('retry-after') != null && Number.isFinite(s)) return clamp(s * 1000);
    return undefined;
  }

  warnOnce(key, message) {
    if (this.warned.has(key)) return;
    this.warned.add(key);
    this.warn(message);
  }
}

// Strict: any missing, extra, mistyped, or out-of-range value rejects the whole
// response. Nothing is defaulted or clamped.
export function decodeResponse(questions, json) {
  if (!json || typeof json !== 'object') return null;
  if (typeof json.model !== 'string' || !json.model) return null;
  const u = json.usage;
  if (!u || !isCount(u.input_tokens) || !isCount(u.output_tokens)) return null;
  const a = json.answers;
  if (!a || typeof a !== 'object') return null;
  const want = Object.keys(questions).sort();
  const got = Object.keys(a).sort();
  if (want.length !== got.length || want.some((k, i) => k !== got[i])) return null;
  for (const [id, q] of Object.entries(questions)) {
    const ans = a[id];
    if (!ans || ans.type !== q.type) return null;
    if (q.type === 'noul' && !isProb(ans.noul)) return null;
    if (q.type === 'choice') {
      if (!Object.keys(q.criteria).includes(ans.choice) || !isProb(ans.confidence)) return null;
      if (!ans.probabilities || typeof ans.probabilities !== 'object' || !Object.values(ans.probabilities).every(isProb)) return null;
    }
    if (q.type === 'score') {
      if (typeof ans.score !== 'number' || !Number.isFinite(ans.score)) return null;
      if (ans.confidence !== undefined && !isProb(ans.confidence)) return null;
    }
  }
  return { model: json.model, answers: a, usage: { inputTokens: u.input_tokens, outputTokens: u.output_tokens } };
}
