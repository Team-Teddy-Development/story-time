import { appendChangelog, today } from '../domain/story.mjs';
import { assertStoryRepository } from '../ports/story-repository.mjs';

// Records why a story's prose changed. Fields go through updateStory instead.
export async function noteStory(repo, id, note, { date = today() } = {}) {
  assertStoryRepository(repo);
  if (!note?.trim()) throw new Error('A note is required.');
  const current = await repo.get(id);
  if (!current) throw new Error(`Story ${id} not found.`);
  const next = appendChangelog(current, note.trim(), date);
  await repo.save(next);
  return next;
}
