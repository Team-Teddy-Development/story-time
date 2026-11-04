import { adoArgs } from '../domain/export.mjs';
import { requireRemote } from '../ports/tracker-gateway.mjs';

// Target workflow state per process template. Active stories are left alone,
// so story-time never fights the team's board.
const STATES = {
  done: { agile: 'Closed', scrum: 'Done', basic: 'Done' },
  archived: { agile: 'Removed', scrum: 'Removed', basic: 'Done' },
  split: { agile: 'Removed', scrum: 'Removed', basic: 'Done' },
};

// Azure DevOps Boards through `az boards`. The PAT goes in env (AZURE_DEVOPS_EXT_PAT).
export class AdoBoardsGateway {
  constructor({ ado, exec, env = {} }) {
    this.ado = ado;
    this.exec = exec;
    this.env = env;
  }

  async run(args) {
    return this.exec('az', ['boards', 'work-item', ...args], { env: this.env });
  }

  url(id) {
    return `${this.ado.organization.replace(/\/+$/, '')}/${encodeURIComponent(this.ado.project)}/_workitems/edit/${id}`;
  }

  targetState(status) {
    return STATES[status]?.[this.ado.process ?? 'agile'];
  }

  content(story) {
    const a = adoArgs(story, this.ado);
    const args = ['--title', a.title];
    if (a.description) args.push('--description', a.description);
    if (a.area) args.push('--area', a.area);
    if (a.iteration) args.push('--iteration', a.iteration);
    if (a.fields.length) args.push('--fields', ...a.fields);
    return { type: a.type, args };
  }

  async create(story) {
    const { type, args } = this.content(story);
    const out = await this.run(['create', '--org', this.ado.organization, '--project', this.ado.project, '--type', type, ...args, '--output', 'json']);
    const id = String(JSON.parse(out).id);
    const state = this.targetState(story.status);
    if (state) await this.run(['update', '--id', id, '--org', this.ado.organization, '--state', state, '--output', 'json']);
    return { remoteId: id, url: this.url(id) };
  }

  async update(story) {
    const id = requireRemote(story);
    const { args } = this.content(story);
    const state = this.targetState(story.status);
    await this.run(['update', '--id', id, '--org', this.ado.organization, ...args, ...(state ? ['--state', state] : []), '--output', 'json']);
    return { remoteId: id, url: this.url(id) };
  }

  // Compensation: the recycle bin, so a rolled-back create can still be restored.
  async remove(remoteId) {
    await this.run(['delete', '--id', String(remoteId), '--org', this.ado.organization, '--yes']);
  }
}
