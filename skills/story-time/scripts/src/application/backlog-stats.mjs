import { today as todayFn } from '../domain/story.mjs';
import { assertStoryRepository } from '../ports/story-repository.mjs';

const CLOSED = new Set(['done', 'split', 'archived']);
const DAY = 86_400_000;

export async function backlogStats(repo, { today = todayFn(), staleDays = 30 } = {}) {
  assertStoryRepository(repo);
  const stories = await repo.list();
  const open = stories.filter((s) => !CLOSED.has(s.status));
  const byStatus = {};
  for (const s of stories) byStatus[s.status] = (byStatus[s.status] ?? 0) + 1;
  const points = (xs) => xs.reduce((sum, s) => sum + (typeof s.size === 'number' ? s.size : 0), 0);
  const now = Date.parse(today);
  return {
    total: stories.length,
    open: open.length,
    byStatus,
    readyPoints: points(open.filter((s) => s.status === 'ready')),
    openPoints: points(open),
    unestimated: open.filter((s) => s.type !== 'spike' && typeof s.size !== 'number').map((s) => s.id),
    stale: open.filter((s) => s.updated && (now - Date.parse(s.updated)) / DAY > staleDays).map((s) => s.id),
  };
}
