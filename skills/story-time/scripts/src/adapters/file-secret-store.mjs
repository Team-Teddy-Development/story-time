import { mkdir, readFile, rename, rm, stat, writeFile } from 'node:fs/promises';
import { execFile } from 'node:child_process';
import { join } from 'node:path';
import { SECRET_LABELS, formatEnvFile, parseEnvFile, secretSpec } from '../config/secrets.mjs';

const GITIGNORE = '# story-time: tracker tokens stay on this machine.\n*.env\n';

// Stores one token per tracker in <dir>/.<tracker>.env with mode 0600, and
// refuses to read a file that is world/group readable or tracked by git,
// the way ssh refuses a loose private key.
export class FileSecretStore {
  constructor(dir, { isTracked = gitTracks } = {}) {
    this.dir = dir;
    this.isTracked = isTracked;
  }

  path(tracker) {
    return join(this.dir, secretSpec(tracker).file);
  }

  async save(tracker, token) {
    const { variable } = secretSpec(tracker);
    const value = String(token ?? '').trim();
    if (!value) throw new Error('The token is empty.');
    if (/\s/.test(value)) throw new Error('The token contains whitespace; paste only the token.');
    await mkdir(this.dir, { recursive: true });
    await ensureGitignore(this.dir);
    const target = this.path(tracker);
    const tmp = `${target}.${process.pid}.tmp`;
    await writeFile(tmp, formatEnvFile(variable, value), { mode: 0o600 });
    await rename(tmp, target);
  }

  async load(tracker) {
    const { variable } = secretSpec(tracker);
    const file = this.path(tracker);
    let info;
    try {
      info = await stat(file);
    } catch (e) {
      if (e.code === 'ENOENT') return undefined;
      throw e;
    }
    if (process.platform !== 'win32' && (info.mode & 0o077) !== 0) {
      throw new Error(`${file} has permissions ${(info.mode & 0o777).toString(8)}; tokens must be private. Run: chmod 600 "${file}"`);
    }
    if (await this.isTracked(file)) {
      throw new Error(
        `${file} is committed to git. Treat the token as leaked: rotate it in ${SECRET_LABELS[tracker]}, ` +
          `then run git rm --cached "${file}" and story secret set ${tracker}.`,
      );
    }
    return parseEnvFile(await readFile(file, 'utf8'))[variable] || undefined;
  }

  async clear(tracker) {
    try {
      await rm(this.path(tracker));
      return true;
    } catch (e) {
      if (e.code === 'ENOENT') return false;
      throw e;
    }
  }
}

export async function ensureGitignore(dir) {
  const file = join(dir, '.gitignore');
  let current = '';
  try {
    current = await readFile(file, 'utf8');
  } catch (e) {
    if (e.code !== 'ENOENT') throw e;
  }
  if (/^\*\.env$/m.test(current)) return;
  await writeFile(file, current ? `${current.trimEnd()}\n*.env\n` : GITIGNORE);
}

// True when git tracks the file. Outside a git repo (or without git) it is not tracked.
function gitTracks(file) {
  return new Promise((resolve) => {
    execFile('git', ['ls-files', '--error-unmatch', file], { cwd: join(file, '..') }, (err) => resolve(!err));
  });
}
