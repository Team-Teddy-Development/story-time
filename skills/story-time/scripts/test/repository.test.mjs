import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readdir, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { runRepositoryContract } from './repository.contract.mjs';
import { InMemoryStoryRepository } from '../src/adapters/memory-story-repository.mjs';
import { FileSystemStoryRepository } from '../src/adapters/fs-story-repository.mjs';
import { assertStoryRepository } from '../src/ports/story-repository.mjs';
import { story } from './fixtures.mjs';

const tempDir = () => mkdtemp(join(tmpdir(), 'story-time-'));

runRepositoryContract('InMemoryStoryRepository', async () => new InMemoryStoryRepository());
runRepositoryContract('FileSystemStoryRepository', async () => new FileSystemStoryRepository(await tempDir()));

test('FileSystemStoryRepository writes one human-readable markdown file per story', async () => {
  const dir = await tempDir();
  const repo = new FileSystemStoryRepository(dir);
  await repo.save(story({ id: 'ST-007', title: 'Save cart' }));
  assert.deepEqual(await readdir(dir), ['ST-007.md']);
  const text = await readFile(join(dir, 'ST-007.md'), 'utf8');
  assert.match(text, /^---\nid: ST-007\ntitle: Save cart/);
  assert.match(text, /## Story\nAs a shopper/);
});

test('FileSystemStoryRepository ignores non-story files', async () => {
  const dir = await tempDir();
  const repo = new FileSystemStoryRepository(dir);
  await repo.save(story({ id: 'ST-1' }));
  await (await import('node:fs/promises')).writeFile(join(dir, 'README.md'), '# notes');
  assert.equal((await repo.list()).length, 1);
});

test('FileSystemStoryRepository rejects ids that could escape the directory', async () => {
  const repo = new FileSystemStoryRepository(await tempDir());
  await assert.rejects(repo.save(story({ id: '../evil' })), /invalid story id/i);
  await assert.rejects(repo.get('../../etc/passwd'), /invalid story id/i);
});

test('assertStoryRepository rejects objects missing port methods', () => {
  assert.throws(() => assertStoryRepository({ get() {} }), /missing: list, save, remove, nextId/);
  assert.doesNotThrow(() => assertStoryRepository(new InMemoryStoryRepository()));
});
