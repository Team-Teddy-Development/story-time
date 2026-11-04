// Use case: mirror stories to the remote tracker as one saga.
// Phase 1 creates/updates remote items; phase 2 links them back locally.
// Any failure undoes what already happened, in reverse:
//   - a remote item created by this run is removed (closed / recycle bin)
//   - a local story is restored to its exact previous content
// Updates of existing remote items are idempotent: re-running push converges.
import { appendChangelog, today } from '../domain/story.mjs';
import { assertStoryRepository } from '../ports/story-repository.mjs';
import { assertTrackerGateway } from '../ports/tracker-gateway.mjs';
import { runSaga } from '../saga/saga.mjs';

const CLOSED = new Set(['done', 'split', 'archived']);
const hasRemote = (s) => s.remote !== undefined && s.remote !== null && s.remote !== '';

export async function planPush(repo, ids) {
  assertStoryRepository(repo);
  const stories = [];
  for (const id of ids) {
    const s = await repo.get(id);
    if (!s) throw new Error(`Story ${id} not found.`);
    stories.push(s);
  }
  return stories.map((story) => ({
    story,
    // A closed story that was never pushed has nothing worth creating.
    action: hasRemote(story) ? 'update' : CLOSED.has(story.status) ? 'skip' : 'create',
  }));
}

export async function pushStories(repo, gateway, ids, { date = today(), dryRun = false } = {}) {
  assertTrackerGateway(gateway);
  const plan = await planPush(repo, ids);
  const summary = plan.map((p) => ({ id: p.story.id, action: p.action, remoteId: p.story.remote ? String(p.story.remote) : undefined }));
  if (dryRun) return { ok: true, plan: summary };

  const active = plan.filter((p) => p.action !== 'skip');
  const remoteSteps = active.map(({ story, action }) => ({
    name: `${action} ${story.id}`,
    execute: () =>
      (action === 'create' ? gateway.create(story) : gateway.update(story)).catch((e) => {
        throw new Error(`${story.id}: ${e.message}`);
      }),
    compensate: action === 'create' ? (_ctx, result) => gateway.remove(result.remoteId) : undefined,
  }));

  const linkSteps = active.map(({ story, action }) => ({
    name: `link ${story.id}`,
    execute: (_ctx, results) => {
      const { remoteId, url } = results[`${action} ${story.id}`];
      const note = action === 'create' ? `pushed to tracker as ${remoteId}` : `updated tracker item ${remoteId}`;
      // Issue numbers and work item ids are numeric: store them unquoted so the file reads naturally.
      const remote = /^\d+$/.test(String(remoteId)) ? Number(remoteId) : remoteId;
      return repo.save(appendChangelog({ ...story, remote, remote_url: url }, note, date));
    },
    compensate: () => repo.save(story),
  }));

  const result = await runSaga([...remoteSteps, ...linkSteps]);
  if (!result.ok) return { ...result, ok: false };

  const pushed = active.map(({ story, action }) => ({ id: story.id, action, ...result.results[`${action} ${story.id}`] }));
  const skipped = plan.filter((p) => p.action === 'skip').map((p) => p.story.id);
  return { ok: true, pushed, skipped };
}
