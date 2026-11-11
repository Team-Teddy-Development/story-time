// In-process simulator of the Gitea REST API v1 issue and label endpoints,
// shaped like fetch(url, init). It enforces the token header, so a gateway
// that forgets auth or puts the token in the URL fails the tests.
export function newGiteaState() {
  return { next: 0, nextLabel: 0, issues: {}, labels: [], calls: [] };
}

const reply = (status, body) => ({
  ok: status >= 200 && status < 300,
  status,
  headers: { get: () => 'application/json' },
  text: async () => (body === undefined ? '' : JSON.stringify(body)),
  json: async () => body,
});

const labelView = (state, ids) => ids.map((id) => state.labels.find((l) => l.id === id)).filter(Boolean);

export function fakeGiteaFetch(state, { baseUrl = 'https://gitea.example.com', owner = 'acme', repo = 'shop', token = 'gt_token', pageSize = 2 } = {}) {
  const prefix = `${baseUrl.replace(/\/+$/, '')}/api/v1/repos/${owner}/${repo}`;
  return async (url, init = {}) => {
    const method = (init.method ?? 'GET').toUpperCase();
    const headers = Object.fromEntries(Object.entries(init.headers ?? {}).map(([k, v]) => [k.toLowerCase(), v]));
    const body = init.body ? JSON.parse(init.body) : undefined;
    state.calls.push({ method, url: String(url), auth: headers.authorization, body });

    const u = new URL(String(url));
    if (!String(url).startsWith(prefix)) return reply(404, { message: 'repository does not exist' });
    if (headers.authorization !== `token ${token}`) return reply(401, { message: 'token is required' });
    const path = u.pathname.slice(new URL(prefix).pathname.length);
    let m;

    if (path === '/labels' && method === 'GET') {
      const page = Number(u.searchParams.get('page') ?? 1);
      const limit = Math.min(Number(u.searchParams.get('limit') ?? pageSize), pageSize);
      return reply(200, state.labels.slice((page - 1) * limit, page * limit));
    }
    if (path === '/labels' && method === 'POST') {
      if (!body?.name || !/^#?[0-9a-f]{6}$/i.test(body.color ?? '')) return reply(422, { message: 'name and color are required' });
      if (state.labels.some((l) => l.name === body.name)) return reply(409, { message: 'label already exists' });
      const label = { id: ++state.nextLabel, name: body.name, color: body.color };
      state.labels.push(label);
      return reply(201, label);
    }
    if (path === '/issues' && method === 'POST') {
      if (!body?.title) return reply(422, { message: 'title is required' });
      const bad = (body.labels ?? []).filter((id) => typeof id !== 'number' || !state.labels.some((l) => l.id === id));
      if (bad.length) return reply(422, { message: `unknown label ids ${bad}` });
      const number = ++state.next;
      state.issues[number] = { number, title: body.title, body: body.body ?? '', state: 'open', labelIds: [...(body.labels ?? [])] };
      return reply(201, view(state, baseUrl, owner, repo, number));
    }
    if ((m = /^\/issues\/(\d+)$/.exec(path))) {
      const issue = state.issues[m[1]];
      if (!issue) return reply(404, { message: 'issue does not exist' });
      if (method === 'GET') return reply(200, view(state, baseUrl, owner, repo, m[1]));
      if (method === 'PATCH') {
        if (body.title !== undefined) issue.title = body.title;
        if (body.body !== undefined) issue.body = body.body;
        if (body.state !== undefined) {
          if (!['open', 'closed'].includes(body.state)) return reply(422, { message: 'bad state' });
          issue.state = body.state;
        }
        return reply(201, view(state, baseUrl, owner, repo, m[1]));
      }
    }
    if ((m = /^\/issues\/(\d+)\/labels$/.exec(path))) {
      const issue = state.issues[m[1]];
      if (!issue) return reply(404, { message: 'issue does not exist' });
      const ids = body?.labels ?? [];
      if (ids.some((id) => typeof id !== 'number' || !state.labels.some((l) => l.id === id))) return reply(422, { message: 'unknown label id' });
      if (method === 'PUT') issue.labelIds = [...ids];
      else if (method === 'POST') issue.labelIds = [...new Set([...issue.labelIds, ...ids])];
      else return reply(405, { message: 'method not allowed' });
      return reply(200, labelView(state, issue.labelIds));
    }
    return reply(404, { message: `no route ${method} ${path}` });
  };
}

function view(state, baseUrl, owner, repo, n) {
  const i = state.issues[n];
  return {
    number: i.number,
    title: i.title,
    body: i.body,
    state: i.state,
    labels: labelView(state, i.labelIds),
    html_url: `${baseUrl.replace(/\/+$/, '')}/${owner}/${repo}/issues/${i.number}`,
  };
}

// Label names on an issue, for assertions.
export function issueLabels(state, n) {
  return labelView(state, state.issues[n].labelIds).map((l) => l.name);
}
