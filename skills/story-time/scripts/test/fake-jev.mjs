// A scriptable stand-in for fetch() against the Jev API. By default it answers
// every question plausibly, and the contradiction canary correctly. Override
// per call with respond(request) returning { status, body, headers } or throwing.

export function answerFor(id, q) {
  if (q.type === 'noul') {
    if (id === 'canary_true') return { type: 'noul', noul: 0.95 };
    if (id.startsWith('canary_false')) return { type: 'noul', noul: 0.04 };
    return { type: 'noul', noul: 0.9 };
  }
  if (q.type === 'choice') {
    const [first, ...rest] = Object.keys(q.criteria);
    return { type: 'choice', choice: first, probabilities: Object.fromEntries([[first, 0.8], ...rest.map((k) => [k, 0.2 / rest.length])]), confidence: 0.8 };
  }
  return { type: 'score', score: 2, probabilities: {}, legend: {}, confidence: 0.7 };
}

export function okBody(request, model = 'jev-1.13.0') {
  const answers = Object.fromEntries(Object.entries(request.questions).map(([id, q]) => [id, answerFor(id, q)]));
  return { model, answers, usage: { input_tokens: 400, output_tokens: 12 } };
}

export function fakeFetch(respond = (req) => ({ status: 200, body: okBody(req) })) {
  const calls = [];
  const fetch = async (url, init) => {
    const request = JSON.parse(init.body);
    calls.push({ url, init, request });
    const r = await respond(request, calls.length);
    const headers = new Map(Object.entries(r.headers ?? {}));
    return {
      ok: r.status >= 200 && r.status < 300,
      status: r.status,
      headers: { get: (k) => headers.get(k.toLowerCase()) ?? null },
      text: async () => (typeof r.body === 'string' ? r.body : JSON.stringify(r.body ?? {})),
    };
  };
  fetch.calls = calls;
  fetch.judgeCalls = () => calls.filter((c) => !Object.keys(c.request.questions).some((k) => k.startsWith('canary_')));
  return fetch;
}
