import { mkdir, readdir, readFile, rm, writeFile, rename } from 'node:fs/promises';
import { join } from 'node:path';
import { parseStory, serializeStory } from '../domain/story.mjs';
import { assertValidId, byId, formatId, highestNumber, ID_PATTERN } from '../ports/story-repository.mjs';

// Filesystem adapter: one markdown file per story, <dir>/<ID>.md.
// Files are the source of truth so humans and agents can edit them directly.
export class FileSystemStoryRepository {
  // reservedIds: optional async () => string[] of ids used elsewhere (for example on other git
  // branches). nextId allocates above them too, so parallel branches never collide.
  constructor(dir, { reservedIds } = {}) {
    this.dir = dir;
    this.reservedIds = reservedIds;
  }

  path(id) {
    return join(this.dir, `${assertValidId(id)}.md`);
  }

  async ids() {
    try {
      const files = await readdir(this.dir);
      return files.filter((f) => f.endsWith('.md')).map((f) => f.slice(0, -3)).filter((id) => ID_PATTERN.test(id));
    } catch (e) {
      if (e.code === 'ENOENT') return [];
      throw e;
    }
  }

  async list({ status } = {}) {
    const stories = await Promise.all((await this.ids()).map((id) => this.get(id)));
    return stories.filter((s) => s && (!status || s.status === status)).sort(byId);
  }

  async get(id) {
    try {
      return parseStory(await readFile(this.path(id), 'utf8'));
    } catch (e) {
      if (e.code === 'ENOENT') return null;
      throw e;
    }
  }

  async save(story) {
    const target = this.path(story.id);
    await mkdir(this.dir, { recursive: true });
    // Write-then-rename so a crash never leaves a half-written story.
    const tmp = `${target}.${process.pid}.tmp`;
    await writeFile(tmp, serializeStory(story), 'utf8');
    await rename(tmp, target);
  }

  async remove(id) {
    const p = this.path(id);
    try {
      await rm(p);
      return true;
    } catch (e) {
      if (e.code === 'ENOENT') return false;
      throw e;
    }
  }

  async nextId(prefix) {
    const reserved = this.reservedIds ? await this.reservedIds() : [];
    return formatId(prefix, highestNumber([...(await this.ids()), ...reserved], prefix) + 1);
  }
}
