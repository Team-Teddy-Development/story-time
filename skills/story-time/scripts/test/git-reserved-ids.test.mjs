import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { FileSystemStoryRepository } from '../src/adapters/fs-story-repository.mjs';
import { gitReservedIds } from '../src/adapters/git-reserved-ids.mjs';

const git = (cwd, ...args) => execFileSync('git', args, { cwd, stdio: 'pipe' }).toString();

async function repoWithBranches() {
  const root = await mkdtemp(join(tmpdir(), 'story-ids-'));
  git(root, 'init', '-q', '-b', 'main');
  git(root, 'config', 'user.email', 'test@example.invalid');
  git(root, 'config', 'user.name', 'test');
  const backlog = join(root, 'backlog');
  await mkdir(backlog);
  await writeFile(join(backlog, 'ST-001.md'), 'x');
  git(root, 'add', '.');
  git(root, 'commit', '-qm', 'main');
  git(root, 'switch', '-qc', 'other');
  await writeFile(join(backlog, 'ST-007.md'), 'x');
  git(root, 'add', '.');
  git(root, 'commit', '-qm', 'other');
  git(root, 'switch', '-q', 'main');
  return { root, backlog };
}

test('nextId skips ids already used on another branch', async () => {
  const { backlog } = await repoWithBranches();
  const repo = new FileSystemStoryRepository(backlog, { reservedIds: gitReservedIds(backlog) });
  assert.equal(await repo.nextId('ST'), 'ST-008');
});

test('nextId still counts only local files when no reserved-id source is given', async () => {
  const { backlog } = await repoWithBranches();
  assert.equal(await new FileSystemStoryRepository(backlog).nextId('ST'), 'ST-002');
});

test('gitReservedIds returns nothing outside a git repository', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'story-nogit-'));
  assert.deepEqual(await gitReservedIds(dir)(), []);
});
