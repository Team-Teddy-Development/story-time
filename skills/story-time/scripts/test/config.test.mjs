import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  parseConfig, defaultExportFormat, pushPolicy, TRACKERS, CONFIG_DIR, CONFIG_FILE, LEGACY_CONFIG_FILE,
} from '../src/config/config.mjs';

test('an empty config gets file-backed defaults', () => {
  const c = parseConfig({});
  assert.equal(c.tracker, 'files');
  assert.equal(c.backlogDir, 'backlog');
  assert.equal(c.prefix, 'ST');
  assert.deepEqual(c.team, { sprintDays: 10, velocity: undefined, wipLimit: 1 });
  assert.deepEqual(c.definitionOfDone, []);
  assert.deepEqual(c.definitionOfReady, []);
  assert.deepEqual(c.project, { stack: [], testFramework: undefined, testCommand: undefined });
});

test('supported trackers', () => {
  assert.deepEqual(TRACKERS, ['files', 'github', 'ado', 'jira', 'linear']);
});

test('github tracker requires an owner/repo', () => {
  assert.throws(() => parseConfig({ tracker: 'github' }), /github\.repo/);
  assert.throws(() => parseConfig({ tracker: 'github', github: { repo: 'nope' } }), /owner\/repo/);
  assert.equal(parseConfig({ tracker: 'github', github: { repo: 'acme/shop' } }).github.repo, 'acme/shop');
});

test('ado tracker requires organization and project, and defaults the process to agile', () => {
  assert.throws(() => parseConfig({ tracker: 'ado', ado: { project: 'Shop' } }), /ado\.organization/);
  assert.throws(() => parseConfig({ tracker: 'ado', ado: { organization: 'https://dev.azure.com/acme' } }), /ado\.project/);
  const c = parseConfig({ tracker: 'ado', ado: { organization: 'https://dev.azure.com/acme', project: 'Shop' } });
  assert.equal(c.ado.process, 'agile');
  assert.throws(
    () => parseConfig({ tracker: 'ado', ado: { organization: 'o', project: 'p', process: 'waterfall' } }),
    /process.*agile, scrum, basic/,
  );
});

test('rejects unknown trackers, bad prefixes, and nonsense team numbers', () => {
  assert.throws(() => parseConfig({ tracker: 'trello' }), /Unknown tracker "trello"/);
  assert.throws(() => parseConfig({ prefix: 'st-1' }), /prefix/);
  assert.throws(() => parseConfig({ team: { wipLimit: 0 } }), /wipLimit/);
  assert.throws(() => parseConfig({ team: { wipLimit: 1.5 } }), /wipLimit/);
  assert.throws(() => parseConfig({ team: { velocity: -3 } }), /velocity/);
});

test('rejects unknown top-level keys so typos surface', () => {
  assert.throws(() => parseConfig({ trackr: 'github' }), /Unknown config key "trackr"/);
});

test('keeps project stack, test framework, test command, and DoD/DoR additions', () => {
  const c = parseConfig({
    project: { stack: ['typescript', 'react'], testFramework: 'vitest', testCommand: 'npm test' },
    definitionOfDone: ['Feature flag removed or ticketed'],
    definitionOfReady: ['Design linked'],
  });
  assert.deepEqual(c.project.stack, ['typescript', 'react']);
  assert.equal(c.project.testFramework, 'vitest');
  assert.equal(c.project.testCommand, 'npm test');
  assert.deepEqual(c.definitionOfDone, ['Feature flag removed or ticketed']);
});

test('parseConfig does not mutate its input', () => {
  const raw = { tracker: 'files' };
  parseConfig(raw);
  assert.deepEqual(raw, { tracker: 'files' });
});

test('sync defaults to off so nothing is pushed unless the project opts in', () => {
  assert.equal(parseConfig({}).sync, 'off');
  assert.equal(parseConfig({ tracker: 'github', github: { repo: 'a/b' } }).sync, 'off');
});

test('sync accepts off, ask, and auto, and needs a pushable tracker', () => {
  for (const sync of ['off', 'ask', 'auto']) {
    assert.equal(parseConfig({ tracker: 'ado', sync, ado: { organization: 'o', project: 'p' } }).sync, sync);
  }
  assert.throws(() => parseConfig({ sync: 'always' }), /sync must be one of: off, ask, auto/);
  assert.throws(() => parseConfig({ sync: 'auto' }), /sync "auto" needs tracker github or ado/);
  assert.throws(() => parseConfig({ tracker: 'jira', sync: 'ask' }), /sync "ask" needs tracker github or ado/);
});

test('pushPolicy explains why pushing is refused', () => {
  assert.throws(() => pushPolicy(parseConfig({})), /Pushing is off for this project.*story export.*story-setup/s);
  assert.doesNotThrow(() => pushPolicy(parseConfig({ tracker: 'github', sync: 'ask', github: { repo: 'a/b' } })));
});

test('config lives in .story-time/settings.json, with the old .story-time.json as a fallback', () => {
  assert.equal(CONFIG_DIR, '.story-time');
  assert.equal(CONFIG_FILE, '.story-time/settings.json');
  assert.equal(LEGACY_CONFIG_FILE, '.story-time.json');
});

test('export format follows the tracker', () => {
  assert.equal(defaultExportFormat(parseConfig({})), 'markdown');
  assert.equal(defaultExportFormat(parseConfig({ tracker: 'github', github: { repo: 'a/b' } })), 'github');
  assert.equal(defaultExportFormat(parseConfig({ tracker: 'ado', ado: { organization: 'o', project: 'p' } })), 'ado');
  assert.equal(defaultExportFormat(parseConfig({ tracker: 'jira' })), 'markdown');
});
