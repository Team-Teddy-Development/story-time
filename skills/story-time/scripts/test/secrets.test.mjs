import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile, stat, chmod, writeFile, mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { parseEnvFile, mask, SECRET_SPECS, tokenEnv } from '../src/config/secrets.mjs';
import { FileSecretStore } from '../src/adapters/file-secret-store.mjs';
import { makeTempDir } from './temp-dir.mjs';

const tmp = () => makeTempDir('story-secret-');
const notTracked = async () => false;

test('secret specs name one env file and variable per tracker', () => {
  assert.deepEqual(SECRET_SPECS.github, { file: '.github.env', variable: 'GH_TOKEN' });
  assert.deepEqual(SECRET_SPECS.ado, { file: '.ado.env', variable: 'AZURE_DEVOPS_EXT_PAT' });
  assert.deepEqual(SECRET_SPECS.gitea, { file: '.gitea.env', variable: 'GITEA_TOKEN' });
});

test('parseEnvFile reads KEY=value lines, ignoring comments and quotes', () => {
  assert.deepEqual(parseEnvFile('# token\nGH_TOKEN="ghp_abc"\n\nOTHER=x y\n'), { GH_TOKEN: 'ghp_abc', OTHER: 'x y' });
});

test('mask never reveals more than the last four characters', () => {
  assert.equal(mask('ghp_1234567890abcd'), '…abcd');
  assert.equal(mask('abc'), '…');
});

test('tokenEnv prefers an existing environment variable (CI, gh auth) over the file', () => {
  assert.deepEqual(tokenEnv('github', { file: 'from-file', env: {} }), { GH_TOKEN: 'from-file' });
  assert.deepEqual(tokenEnv('github', { file: 'from-file', env: { GH_TOKEN: 'from-env' } }), {});
  assert.deepEqual(tokenEnv('github', { file: undefined, env: {} }), {});
});

test('save writes a 0600 file and a .gitignore that excludes every env file', async () => {
  const dir = await tmp();
  const store = new FileSecretStore(dir, { isTracked: notTracked });
  await store.save('github', 'ghp_secret');
  const file = join(dir, '.github.env');
  assert.equal((await stat(file)).mode & 0o777, 0o600);
  assert.equal(parseEnvFile(await readFile(file, 'utf8')).GH_TOKEN, 'ghp_secret');
  assert.match(await readFile(join(dir, '.gitignore'), 'utf8'), /^\*\.env$/m);
  assert.equal(await store.load('github'), 'ghp_secret');
});

test('load returns undefined when no token was saved', async () => {
  assert.equal(await new FileSecretStore(await tmp(), { isTracked: notTracked }).load('ado'), undefined);
});

test('load refuses a token file readable by group or others', async () => {
  const dir = await tmp();
  const store = new FileSecretStore(dir, { isTracked: notTracked });
  await store.save('ado', 'pat');
  await chmod(join(dir, '.ado.env'), 0o644);
  await assert.rejects(store.load('ado'), /permissions.*chmod 600/s);
});

test('load refuses a token file that is tracked by git', async () => {
  const dir = await tmp();
  await mkdir(dir, { recursive: true });
  await writeFile(join(dir, '.github.env'), 'GH_TOKEN=x\n', { mode: 0o600 });
  const store = new FileSecretStore(dir, { isTracked: async () => true });
  await assert.rejects(store.load('github'), /committed to git.*rotate/s);
});

test('clear removes the token file', async () => {
  const dir = await tmp();
  const store = new FileSecretStore(dir, { isTracked: notTracked });
  await store.save('github', 'x');
  assert.equal(await store.clear('github'), true);
  assert.equal(await store.load('github'), undefined);
});

test('rejects unknown trackers and empty tokens', async () => {
  const store = new FileSecretStore(await tmp(), { isTracked: notTracked });
  await assert.rejects(store.save('jira', 'x'), /no token support for "jira"/);
  await assert.rejects(store.save('github', '  '), /empty/);
});
