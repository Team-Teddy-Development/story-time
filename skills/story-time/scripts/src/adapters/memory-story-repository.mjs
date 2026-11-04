import { assertValidId, byId, formatId, highestNumber } from '../ports/story-repository.mjs';

// In-memory adapter: tests and dry runs. Stores structured clones so callers
// can never mutate repository state through a returned reference.
export class InMemoryStoryRepository {
  constructor(stories = []) {
    this.stories = new Map(stories.map((s) => [s.id, structuredClone(s)]));
  }

  async list({ status } = {}) {
    return [...this.stories.values()]
      .filter((s) => !status || s.status === status)
      .map((s) => structuredClone(s))
      .sort(byId);
  }

  async get(id) {
    const s = this.stories.get(id);
    return s ? structuredClone(s) : null;
  }

  async save(story) {
    assertValidId(story.id);
    this.stories.set(story.id, structuredClone(story));
  }

  async remove(id) {
    return this.stories.delete(id);
  }

  async nextId(prefix) {
    return formatId(prefix, highestNumber([...this.stories.keys()], prefix) + 1);
  }
}
