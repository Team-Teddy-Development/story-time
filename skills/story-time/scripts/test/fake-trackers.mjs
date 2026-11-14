// In-process simulators of the `gh` and `az boards` CLIs, shaped like the
// injected exec(cmd, args, { input }) the gateways use. The same simulators
// back fake binaries on PATH for CLI end-to-end tests (see makeFakeBin).
import { writeFile, readFile, chmod } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { makeTempDir } from './temp-dir.mjs';

export function newState() {
  return { next: 0, issues: {}, items: {}, labels: [], calls: [] };
}

// Splits ["--a", "1", "--fields", "x=1", "y=2", "--b", "2"] into flags.
function parseFlags(args) {
  const flags = {};
  const rest = [];
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (!a.startsWith('--')) {
      rest.push(a);
      continue;
    }
    const key = a.slice(2);
    if (key === 'fields') {
      flags.fields = [];
      while (args[i + 1] !== undefined && !args[i + 1].startsWith('--')) flags.fields.push(args[++i]);
      continue;
    }
    if (key === 'yes') {
      flags.yes = true;
      continue;
    }
    const value = args[++i];
    if (key === 'label' || key === 'add-label' || key === 'remove-label') (flags[key] ??= []).push(value);
    else flags[key] = value;
  }
  return { flags, rest };
}

export function fakeGh(state, { failOn } = {}) {
  return async (cmd, args, { input } = {}) => {
    if (cmd !== 'gh') throw new Error(`fake gh got ${cmd}`);
    state.calls.push([cmd, ...args]);
    const [group, verb, ...tail] = args;
    if (failOn?.(group, verb, state)) throw new Error(`gh: simulated failure on ${group} ${verb}`);
    const { flags, rest } = parseFlags(tail);
    if (group === 'label' && verb === 'create') {
      if (!state.labels.includes(rest[0])) state.labels.push(rest[0]);
      return '';
    }
    if (group !== 'issue') throw new Error(`fake gh: unsupported ${group} ${verb}`);
    if (verb === 'create') {
      for (const l of flags.label ?? []) if (!state.labels.includes(l)) throw new Error(`could not add label: '${l}' not found`);
      const n = ++state.next;
      state.issues[n] = { title: flags.title, body: flags['body-file'] === '-' ? input : '', labels: [...(flags.label ?? [])], state: 'OPEN' };
      return `https://github.com/${flags.repo}/issues/${n}\n`;
    }
    const n = Number(rest[0]);
    const issue = state.issues[n];
    if (!issue) throw new Error(`GraphQL: Could not resolve to an issue or pull request with the number of ${rest[0]}.`);
    if (verb === 'view') return JSON.stringify({ labels: issue.labels.map((name) => ({ name })), state: issue.state });
    if (verb === 'edit') {
      if (flags.title) issue.title = flags.title;
      if (flags['body-file'] === '-') issue.body = input;
      for (const l of flags['add-label'] ?? []) {
        if (!state.labels.includes(l)) throw new Error(`could not add label: '${l}' not found`);
        if (!issue.labels.includes(l)) issue.labels.push(l);
      }
      issue.labels = issue.labels.filter((l) => !(flags['remove-label'] ?? []).includes(l));
      return `https://github.com/${flags.repo}/issues/${n}\n`;
    }
    if (verb === 'close') {
      issue.state = 'CLOSED';
      issue.reason = flags.reason;
      return '';
    }
    if (verb === 'reopen') {
      issue.state = 'OPEN';
      return '';
    }
    throw new Error(`fake gh: unsupported issue ${verb}`);
  };
}

export function fakeAz(state, { failOn } = {}) {
  return async (cmd, args) => {
    if (cmd !== 'az') throw new Error(`fake az got ${cmd}`);
    state.calls.push([cmd, ...args]);
    const [group, sub, verb, ...tail] = args;
    if (group !== 'boards' || sub !== 'work-item') throw new Error(`fake az: unsupported ${args.slice(0, 3).join(' ')}`);
    if (failOn?.(verb, state)) throw new Error(`az: simulated failure on ${verb}`);
    const { flags } = parseFlags(tail);
    const fieldMap = Object.fromEntries((flags.fields ?? []).map((f) => [f.slice(0, f.indexOf('=')), f.slice(f.indexOf('=') + 1)]));
    if (verb === 'create') {
      const id = ++state.next;
      state.items[id] = {
        type: flags.type, title: flags.title, description: flags.description, area: flags.area,
        iteration: flags.iteration, fields: fieldMap, state: 'New', org: flags.org, project: flags.project,
      };
      return JSON.stringify({ id, fields: { 'System.Title': flags.title } });
    }
    const id = Number(flags.id);
    const item = state.items[id];
    if (!item) throw new Error(`TF401232: Work item ${flags.id} does not exist, or you do not have permissions to read it.`);
    if (verb === 'update') {
      if (flags.title) item.title = flags.title;
      if (flags.description) item.description = flags.description;
      if (flags.state) item.state = flags.state;
      Object.assign(item.fields, fieldMap);
      return JSON.stringify({ id, fields: { 'System.State': item.state } });
    }
    if (verb === 'delete') {
      if (!flags.yes) throw new Error('az: delete needs --yes when not interactive');
      delete state.items[id];
      return '';
    }
    throw new Error(`fake az: unsupported ${verb}`);
  };
}

// Writes executable `gh` and `az` scripts that run the simulators against a
// JSON state file, so the real process-exec adapter can be tested end to end.
export async function makeFakeBin() {
  const dir = await makeTempDir('story-fakebin-');
  const stateFile = join(dir, 'state.json');
  await writeFile(stateFile, JSON.stringify(newState()));
  const self = fileURLToPath(import.meta.url);
  for (const name of ['gh', 'az']) {
    const script = `#!/usr/bin/env node
import { readFileSync, writeFileSync } from 'node:fs';
import { ${name === 'gh' ? 'fakeGh' : 'fakeAz'} as fake } from ${JSON.stringify(self)};
const stateFile = ${JSON.stringify(stateFile)};
const state = JSON.parse(readFileSync(stateFile, 'utf8'));
let input = '';
if (!process.stdin.isTTY) { try { input = readFileSync(0, 'utf8'); } catch {} }
try {
  const out = await fake(state)(${JSON.stringify(name)}, process.argv.slice(2), { input });
  writeFileSync(stateFile, JSON.stringify(state));
  process.stdout.write(out);
} catch (e) {
  writeFileSync(stateFile, JSON.stringify(state));
  process.stderr.write(e.message + '\\n');
  process.exit(1);
}
`;
    const path = join(dir, name);
    await writeFile(path, script);
    await chmod(path, 0o755);
  }
  return { dir, readState: async () => JSON.parse(await readFile(stateFile, 'utf8')) };
}
