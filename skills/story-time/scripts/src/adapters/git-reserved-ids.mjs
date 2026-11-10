import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { ID_PATTERN } from '../ports/story-repository.mjs';

const run = promisify(execFile);

// Ids that other branches already use, so two branches never hand out the same id.
// Reads the backlog folder on every local branch and remote-tracking branch. It sees only what
// the last `git fetch` brought in, and returns nothing outside a git repository.
export function gitReservedIds(dir) {
  return async () => {
    const git = (...args) => run('git', ['-C', dir, ...args], { maxBuffer: 16 * 1024 * 1024 }).then((r) => r.stdout);
    try {
      await git('rev-parse', '--show-toplevel');
    } catch {
      return [];
    }
    const refs = (await git('for-each-ref', '--format=%(refname)', 'refs/heads', 'refs/remotes'))
      .split('\n').filter((r) => r && !r.endsWith('/HEAD'));
    const ids = new Set();
    for (const ref of refs) {
      // The pathspec `.` is relative to `-C dir`, and names come back relative to it, so the
      // backlog folder can sit anywhere in the repo.
      const names = await git('ls-tree', '--name-only', ref, '--', '.').catch(() => '');
      for (const name of names.split('\n')) {
        if (!name.endsWith('.md')) continue;
        const id = name.slice(0, -3);
        if (ID_PATTERN.test(id)) ids.add(id);
      }
    }
    return [...ids];
  };
}
