import { githubIssue } from '../domain/export.mjs';
import { TYPES } from '../domain/story.mjs';
import { CLOSING, requireRemote } from '../ports/tracker-gateway.mjs';

// Gitea has no close reason, so "not planned" is a label.
export const NOT_PLANNED = 'not-planned';

// Labels story-time owns. Anything else on the issue belongs to the team.
const MANAGED = (label) => TYPES.includes(label) || label === NOT_PLANNED || /^(size|value|epic):/.test(label);
const LABEL_COLOR = '#ededed';
const PAGE = 50;

// Gitea Issues through the REST API v1 (fetch, no CLI). The token goes in the
// Authorization header only, never in a URL or an error message.
export class GiteaIssuesGateway {
  constructor({ gitea, token, fetch = globalThis.fetch }) {
    if (!token) throw new Error('No Gitea token. Store one with: story secret set gitea (or export GITEA_TOKEN).');
    this.baseUrl = gitea.baseUrl.replace(/\/+$/, '');
    this.owner = gitea.owner;
    this.repo = gitea.repo;
    this.token = token;
    this.fetch = fetch;
    this.labelIds = undefined; // name -> id, loaded on first use
  }

  url(n) {
    return `${this.baseUrl}/${this.owner}/${this.repo}/issues/${n}`;
  }

  async api(method, path, body) {
    const where = `/repos/${this.owner}/${this.repo}${path}`;
    let res;
    try {
      res = await this.fetch(`${this.baseUrl}/api/v1${where}`, {
        method,
        headers: {
          authorization: `token ${this.token}`,
          accept: 'application/json',
          ...(body === undefined ? {} : { 'content-type': 'application/json' }),
        },
        body: body === undefined ? undefined : JSON.stringify(body),
        redirect: 'error',
      });
    } catch (e) {
      throw new Error(`Gitea ${method} ${where} failed: ${e.message}`);
    }
    const text = await res.text();
    let json;
    try {
      json = text ? JSON.parse(text) : undefined;
    } catch {
      json = undefined;
    }
    if (!res.ok) {
      const detail = (json?.message ?? text.slice(0, 200)).split(this.token).join('…');
      throw new Error(`Gitea ${method} ${where} failed: HTTP ${res.status}${detail ? ` ${detail}` : ''}`);
    }
    return json;
  }

  async loadLabels() {
    if (this.labelIds) return this.labelIds;
    const ids = new Map();
    // Servers cap the page size, so read until an empty page.
    for (let page = 1; ; page++) {
      const batch = await this.api('GET', `/labels?page=${page}&limit=${PAGE}`);
      if (!Array.isArray(batch) || !batch.length) break;
      for (const l of batch) ids.set(l.name, l.id);
    }
    this.labelIds = ids;
    return ids;
  }

  async ensureLabels(names) {
    const ids = await this.loadLabels();
    for (const name of names) {
      if (ids.has(name)) continue;
      const label = await this.api('POST', '/labels', { name, color: LABEL_COLOR });
      ids.set(label.name, label.id);
    }
    return names.map((n) => ids.get(n));
  }

  // Managed labels a story should carry, including not-planned when it closes that way.
  labelsFor(story) {
    const issue = githubIssue(story);
    const labels = CLOSING[story.status] === 'not planned' ? [...issue.labels, NOT_PLANNED] : issue.labels;
    return { issue, labels };
  }

  async create(story) {
    const { issue, labels } = this.labelsFor(story);
    const ids = await this.ensureLabels(labels);
    const created = await this.api('POST', '/issues', { title: issue.title, body: issue.body, labels: ids });
    const n = String(created.number);
    if (CLOSING[story.status]) await this.api('PATCH', `/issues/${n}`, { state: 'closed' });
    return { remoteId: n, url: this.url(n) };
  }

  async update(story) {
    const n = requireRemote(story);
    const current = await this.api('GET', `/issues/${n}`);
    const { issue, labels } = this.labelsFor(story);
    const teamIds = (current.labels ?? []).filter((l) => !MANAGED(l.name)).map((l) => l.id);
    const ids = await this.ensureLabels(labels);
    await this.api('PATCH', `/issues/${n}`, { title: issue.title, body: issue.body, state: CLOSING[story.status] ? 'closed' : 'open' });
    await this.api('PUT', `/issues/${n}/labels`, { labels: [...teamIds, ...ids] });
    return { remoteId: n, url: this.url(n) };
  }

  // Compensation: close as not planned. Issues are never deleted.
  async remove(remoteId) {
    const n = String(remoteId);
    const [id] = await this.ensureLabels([NOT_PLANNED]);
    await this.api('POST', `/issues/${n}/labels`, { labels: [id] });
    await this.api('PATCH', `/issues/${n}`, { state: 'closed' });
  }
}
