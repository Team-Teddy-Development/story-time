// TrackerGateway port: push one story to a remote tracker. The local backlog
// stays the source of truth; the gateway mirrors it.
//
// interface TrackerGateway {
//   create(story): Promise<{ remoteId: string, url: string }>   // story has no remote yet
//   update(story): Promise<{ remoteId: string, url: string }>   // story.remote is set
//   remove(remoteId): Promise<void>                              // saga compensation: take it out of the active backlog
// }

export const GATEWAY_METHODS = ['create', 'update', 'remove'];

export function assertTrackerGateway(gateway) {
  const missing = GATEWAY_METHODS.filter((m) => typeof gateway?.[m] !== 'function');
  if (missing.length) throw new TypeError(`Not a TrackerGateway; missing: ${missing.join(', ')}`);
  return gateway;
}

// Statuses that close the remote item, and how.
export const CLOSING = { done: 'completed', archived: 'not planned', split: 'not planned' };

export function requireRemote(story) {
  if (story.remote === undefined || story.remote === null || story.remote === '') {
    throw new Error(`${story.id} has no remote item to update; push it to create one.`);
  }
  return String(story.remote);
}
