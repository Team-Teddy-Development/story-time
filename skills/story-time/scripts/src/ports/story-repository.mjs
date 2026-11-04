// StoryRepository port. Use cases depend on this shape, never on a concrete
// store (dependency inversion). Adapters: memory, filesystem; GitHub Issues,
// Jira, or Linear adapters implement the same five methods.
//
// interface StoryRepository {
//   list(filter?: { status?: string }): Promise<Story[]>   // sorted by id
//   get(id: string): Promise<Story | null>
//   save(story: Story): Promise<void>                      // upsert
//   remove(id: string): Promise<boolean>                   // true if removed
//   nextId(prefix: string): Promise<string>                // e.g. "ST-012"
// }

export const PORT_METHODS = ['list', 'get', 'save', 'remove', 'nextId'];

export function assertStoryRepository(repo) {
  const missing = PORT_METHODS.filter((m) => typeof repo?.[m] !== 'function');
  if (missing.length) throw new TypeError(`Not a StoryRepository; missing: ${missing.join(', ')}`);
  return repo;
}

export const ID_PATTERN = /^[A-Z][A-Z0-9]*-\d+$/;

export function assertValidId(id) {
  if (typeof id !== 'string' || !ID_PATTERN.test(id)) throw new Error(`Invalid story id: ${id}`);
  return id;
}

export function formatId(prefix, n) {
  return `${prefix}-${String(n).padStart(3, '0')}`;
}

export function highestNumber(ids, prefix) {
  return ids.reduce((max, id) => {
    const m = new RegExp(`^${prefix}-(\\d+)$`).exec(id);
    return m ? Math.max(max, Number(m[1])) : max;
  }, 0);
}

export const byId = (a, b) => a.id.localeCompare(b.id, 'en', { numeric: true });
