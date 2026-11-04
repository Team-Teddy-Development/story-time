import { githubIssue } from '../domain/export.mjs';
import { TYPES } from '../domain/story.mjs';
import { CLOSING, requireRemote } from '../ports/tracker-gateway.mjs';

// Labels story-time owns. Anything else on the issue belongs to the team.
const MANAGED = (label) => TYPES.includes(label) || /^(size|value|epic):/.test(label);

// GitHub Issues through the gh CLI. Bodies go on stdin, the token in env.
export class GitHubIssuesGateway {
  constructor({ repo, exec, env = {} }) {
    this.repo = repo;
    this.exec = exec;
    this.env = env;
    this.knownLabels = new Set();
  }

  run(args, input) {
    return this.exec('gh', args, { input, env: this.env });
  }

  url(n) {
    return `https://github.com/${this.repo}/issues/${n}`;
  }

  async ensureLabels(labels) {
    for (const label of labels) {
      if (this.knownLabels.has(label)) continue;
      await this.run(['label', 'create', label, '--repo', this.repo, '--force']);
      this.knownLabels.add(label);
    }
  }

  async create(story) {
    const issue = githubIssue(story);
    await this.ensureLabels(issue.labels);
    const out = await this.run(
      ['issue', 'create', '--repo', this.repo, '--title', issue.title, '--body-file', '-', ...issue.labels.flatMap((l) => ['--label', l])],
      issue.body,
    );
    const m = /\/issues\/(\d+)/.exec(out);
    if (!m) throw new Error(`gh issue create did not return an issue URL: ${out.trim()}`);
    await this.syncState(m[1], story.status, 'OPEN');
    return { remoteId: m[1], url: this.url(m[1]) };
  }

  async update(story) {
    const n = requireRemote(story);
    const view = JSON.parse(await this.run(['issue', 'view', n, '--repo', this.repo, '--json', 'labels,state']));
    const current = view.labels.map((l) => l.name);
    const issue = githubIssue(story);
    const stale = current.filter((l) => MANAGED(l) && !issue.labels.includes(l));
    await this.ensureLabels(issue.labels);
    await this.run(
      [
        'issue', 'edit', n, '--repo', this.repo, '--title', issue.title, '--body-file', '-',
        ...issue.labels.flatMap((l) => ['--add-label', l]),
        ...stale.flatMap((l) => ['--remove-label', l]),
      ],
      issue.body,
    );
    await this.syncState(n, story.status, view.state);
    return { remoteId: n, url: this.url(n) };
  }

  async syncState(n, status, current) {
    const reason = CLOSING[status];
    if (reason && current === 'OPEN') await this.run(['issue', 'close', n, '--repo', this.repo, '--reason', reason]);
    if (!reason && current === 'CLOSED') await this.run(['issue', 'reopen', n, '--repo', this.repo]);
  }

  async remove(remoteId) {
    await this.run(['issue', 'close', String(remoteId), '--repo', this.repo, '--reason', 'not planned']);
  }
}
