#!/usr/bin/env node
// story — CLI for the story-time backlog. This file is the composition root
// and the only boundary: it parses argv, picks the adapter, calls use cases,
// and formats output. No business rules live here.
import { readFile, writeFile, access, mkdir } from 'node:fs/promises';
import { existsSync, readFileSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { FileSystemStoryRepository } from './src/adapters/fs-story-repository.mjs';
import { gitReservedIds } from './src/adapters/git-reserved-ids.mjs';
import { lintStory } from './src/domain/invest.mjs';
import { rankBacklog } from './src/domain/wsjf.mjs';
import { serializeStory } from './src/domain/story.mjs';
import { createStory } from './src/application/create-story.mjs';
import { updateStory } from './src/application/update-story.mjs';
import { noteStory } from './src/application/note-story.mjs';
import { splitStory } from './src/application/split-story.mjs';
import { backlogStats } from './src/application/backlog-stats.mjs';
import { CONFIG_DIR, CONFIG_FILE, LEGACY_CONFIG_FILE, defaultExportFormat, parseConfig, pushPolicy } from './src/config/config.mjs';
import { SECRET_LABELS, SECRET_SPECS, mask, tokenEnv } from './src/config/secrets.mjs';
import { selectDecisionEngine } from './src/application/select-decision-engine.mjs';
import { FileSecretStore, ensureGitignore } from './src/adapters/file-secret-store.mjs';
import { GitHubIssuesGateway } from './src/adapters/github-issues-gateway.mjs';
import { AdoBoardsGateway } from './src/adapters/ado-boards-gateway.mjs';
import { GiteaIssuesGateway } from './src/adapters/gitea-issues-gateway.mjs';
import { processExec } from './src/adapters/process-exec.mjs';
import { pushStories } from './src/application/push-stories.mjs';
import { renderCard, renderCommand } from './src/domain/export.mjs';
import { parseToml } from './src/domain/toml.mjs';
import { checkMachine, machineCriteria, machineDiagram } from './src/domain/state-machine.mjs';
import { checkTable, tableCriteria } from './src/domain/decision-table.mjs';

const USAGE = readFileSync(new URL('./usage.txt', import.meta.url), 'utf8').trimEnd();

const NATIVE_ONLY = new Set(['plan', 'adopt', 'issues', 'metrics']);
const BOOLEAN = new Set(['json', 'force', 'include-refined', 'command', 'all', 'dry-run']);
const REPEATABLE = new Set(['ac']);
const PUSH_FLAGS = new Set(['all', 'dry-run', 'json', 'config', 'dir']);

function parseArgs(argv) {
  const positional = [];
  const flags = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith('--')) {
      positional.push(a);
      continue;
    }
    const key = a.slice(2);
    if (BOOLEAN.has(key)) {
      flags[key] = true;
      continue;
    }
    const value = argv[++i];
    if (value === undefined) throw new UsageError(`Flag --${key} needs a value.`);
    if (REPEATABLE.has(key)) (flags[key] ??= []).push(value);
    else flags[key] = value;
  }
  return { positional, flags };
}

class UsageError extends Error {}

const num = (v) => (v === undefined ? undefined : Number(v));

const commands = {
  async help() {
    console.log(USAGE);
  },

  async new(repo, _args, f, ctx) {
    const s = await createStory(
      repo,
      {
        title: f.title,
        type: f.type,
        statement: f.statement,
        acceptance: f.ac,
        epic: f.epic,
        persona: f.persona,
        size: num(f.size),
        value: num(f.value),
        time_criticality: num(f['time-criticality']),
        risk_reduction: num(f['risk-reduction']),
        timebox: f.timebox,
        question: f.question,
      },
      { prefix: f.prefix ?? process.env.STORY_TIME_PREFIX ?? ctx.config.prefix },
    );
    out(f, s, () => `${s.id} created: ${repo.path(s.id)}`);
  },

  async list(repo, _args, f) {
    const stories = await repo.list({ status: f.status });
    out(f, stories, () =>
      table(['ID', 'Status', 'Size', 'Title'], stories.map((s) => [s.id, s.status, s.size ?? '-', s.title])),
    );
  },

  async show(repo, [id], f) {
    const s = await requireStory(repo, id);
    out(f, s, () => serializeStory(s).trimEnd());
  },

  async lint(repo, ids, f, ctx) {
    const all = await repo.list();
    const targets = ids.length ? await Promise.all(ids.map((id) => requireStory(repo, id))) : all;
    const results = targets.map((s) => ({ ...lintStory(s, { stories: all, exists: inRepo(ctx) }), title: s.title }));
    out(f, results, () => results.map(formatLint).join('\n\n') || 'No stories.');
    if (results.some((r) => !r.ready)) process.exitCode = 1;
  },

  async rank(repo, _args, f) {
    const statuses = f['include-refined'] ? ['ready', 'refined'] : ['ready'];
    const { ranked, excluded } = rankBacklog(await repo.list(), { statuses });
    const top = f.limit ? ranked.slice(0, Number(f.limit)) : ranked;
    const json = {
      ranked: top.map((r, i) => ({ rank: i + 1, id: r.story.id, title: r.story.title, wsjf: r.score, costOfDelay: r.costOfDelay, size: r.story.size })),
      excluded: excluded.map((e) => ({ id: e.story.id, title: e.story.title, reason: e.reason })),
    };
    out(f, json, () => {
      const head = top.length
        ? table(['#', 'ID', 'WSJF', 'CoD', 'Size', 'Title'], json.ranked.map((r) => [r.rank, r.id, r.wsjf.toFixed(2), r.costOfDelay, r.size, r.title]))
        : 'No ready, unblocked, scored stories.';
      const tail = json.excluded.length ? `\n\nExcluded:\n${json.excluded.map((e) => `  ${e.id}  ${e.reason}  (${e.title})`).join('\n')}` : '';
      return head + tail;
    });
  },

  async set(repo, [id, ...pairs], f, ctx) {
    if (!id || !pairs.length) throw new UsageError('Usage: story set <id> key=value...');
    const changes = Object.fromEntries(
      pairs.map((p) => {
        const i = p.indexOf('=');
        if (i < 1) throw new UsageError(`Expected key=value, got "${p}".`);
        return [p.slice(0, i).replace(/-/g, '_'), p.slice(i + 1)];
      }),
    );
    const s = await updateStory(repo, id, changes, { note: f.note, force: f.force, exists: inRepo(ctx) });
    out(f, s, () => `${s.id} updated.`);
  },

  async note(repo, [id, ...words], f) {
    if (!id || !words.length) throw new UsageError('Usage: story note <id> "what changed and why"');
    const s = await noteStory(repo, id, words.join(' '));
    out(f, s, () => `${s.id} noted.`);
  },

  async split(repo, [id], f) {
    if (!id || !f.from) throw new UsageError('Usage: story split <id> --from children.json');
    const specs = JSON.parse(await readFile(f.from, 'utf8'));
    const r = await splitStory(repo, id, specs);
    if (!r.ok) {
      const lost = r.compensationFailures?.length
        ? `\nCould not roll back: ${r.compensationFailures.map((c) => `${c.step} (${c.error.message})`).join('; ')}`
        : '';
      throw new Error(`Split failed${r.failedStep ? ` at "${r.failedStep}"` : ''}: ${r.error.message}${lost}`);
    }
    out(f, r, () => `${id} split into ${r.childIds.join(', ')}.`);
  },

  async export(repo, [id], f, ctx) {
    const s = await requireStory(repo, id);
    const format = f.format ?? defaultExportFormat(ctx.config);
    if (f.command) {
      console.log(renderCommand(s, format, { config: ctx.config }));
      return;
    }
    const card = renderCard(s, format, { config: ctx.config });
    out(f, card, () => card.text);
  },

  async init(_repo, _args, f, ctx) {
    const raw = f.from ? JSON.parse(await readFile(f.from, 'utf8')) : {};
    const config = parseConfig(raw); // validate before touching the disk
    if (!f.force && (await exists(ctx.configPath))) {
      throw new Error(`${ctx.configPath} already exists. Re-run with --force to replace it.`);
    }
    await mkdir(dirname(ctx.configPath), { recursive: true });
    await ensureGitignore(join(ctx.root, CONFIG_DIR));
    await writeFile(ctx.configPath, `${JSON.stringify(config, null, 2)}\n`, 'utf8');
    out(f, config, () => `Wrote ${ctx.configPath} (tracker: ${config.tracker}).`);
  },

  async push(repo, ids, f, ctx) {
    const unknown = Object.keys(f).find((k) => !PUSH_FLAGS.has(k));
    if (unknown !== undefined) throw new UsageError(`Unknown push flag --${unknown}.`);
    if (f.all && ids.length) throw new UsageError('Use --all or explicit story IDs, not both.');
    pushPolicy(ctx.config);
    const targets = f.all ? (await repo.list()).map((s) => s.id) : ids;
    if (!targets.length) throw new UsageError('Usage: story push <id...> | --all [--dry-run]');
    const gateway = await makeGateway(ctx);
    const r = await pushStories(repo, gateway, targets, { dryRun: f['dry-run'] });
    if (!r.ok) {
      const undone = r.compensated?.length ? `\nRolled back: ${r.compensated.join(', ')}.` : '';
      const stuck = r.compensationFailures?.length
        ? `\nCould not roll back (fix by hand): ${r.compensationFailures.map((c) => `${c.step} (${c.error.message})`).join('; ')}`
        : '';
      throw new Error(`Push failed${r.failedStep ? ` at "${r.failedStep}"` : ''}: ${r.error.message}${undone}${stuck}`);
    }
    if (r.plan) {
      out(f, r, () => table(['Action', 'ID', 'Remote'], r.plan.map((p) => [p.action, p.id, p.remoteId ?? '-'])));
      return;
    }
    out(f, r, () =>
      [
        r.pushed.length ? table(['Action', 'ID', 'URL'], r.pushed.map((p) => [p.action, p.id, p.url])) : 'Nothing to push.',
        r.skipped.length ? `Skipped (closed, never pushed): ${r.skipped.join(', ')}` : '',
      ].filter(Boolean).join('\n'),
    );
  },

  async secret(_repo, [action, tracker, ...extra], f, ctx) {
    const store = new FileSecretStore(join(ctx.root, CONFIG_DIR));
    if (action === 'status') {
      const rows = [];
      for (const [name, spec] of Object.entries(SECRET_SPECS)) {
        // Always inspect the file: a leaked or loose token file matters even when the env wins.
        let file;
        try {
          const token = await store.load(name);
          file = token ? `set (${mask(token)})  ${join(CONFIG_DIR, spec.file)}` : undefined;
        } catch (e) {
          file = `unusable: ${e.message}`;
        }
        const fromEnv = process.env[spec.variable] ? `set via ${spec.variable} (environment)` : undefined;
        const state = [fromEnv, fromEnv && file ? `file ${file}` : file].filter(Boolean).join('; ') || 'not set';
        rows.push([name, state]);
      }
      out(f, Object.fromEntries(rows), () => table(['Tracker', 'Token'], rows));
      return;
    }
    if (!['set', 'clear'].includes(action) || !tracker) throw new UsageError('Usage: story secret set|clear <github|ado|gitea|jev>, or story secret status');
    if (!SECRET_SPECS[tracker]) throw new UsageError(`There is no token support for "${tracker}". Use: ${Object.keys(SECRET_SPECS).join(', ')}.`);
    if (action === 'clear') {
      const removed = await store.clear(tracker);
      console.log(removed ? `Removed the ${tracker} token.` : `No ${tracker} token was stored.`);
      return;
    }
    if (extra.length) {
      throw new UsageError('Pass the token at the hidden prompt or on stdin, not as an argument (arguments end up in shell history and process lists).');
    }
    const token = await readToken(`Paste your ${SECRET_LABELS[tracker]} ${tracker === 'jev' ? 'API key' : 'token'} (input hidden): `);
    if (!token) throw new Error('No token received.');
    await store.save(tracker, token);
    console.log(`Saved the ${tracker} token (${mask(token)}) to ${join(CONFIG_DIR, SECRET_SPECS[tracker].file)} with mode 600. It is git-ignored.`);
  },

  async judge(repo, ids, f, ctx) {
    const CLOSED = new Set(['done', 'split', 'archived']);
    const targets = ids.length
      ? await Promise.all(ids.map((id) => requireStory(repo, id)))
      : (await repo.list()).filter((s) => !CLOSED.has(s.status));
    const store = new FileSecretStore(join(ctx.root, CONFIG_DIR));
    const apiKey = process.env.TYPESAFE_API_KEY || (await store.load('jev'));
    const { engine, mode, model, why } = selectDecisionEngine({ apiKey, config: ctx.config });
    const judgements = [];
    for (const s of targets) judgements.push({ ...(await engine.judge(s)), title: s.title });
    const info = { mode, model, why };
    out(f, { engine: info, judgements }, () =>
      [`engine: ${mode === 'jev' ? `jev (${model})` : `code (${why})`}`, ...judgements.map(formatJudgement)].join('\n\n'),
    );
  },

  async machine(_repo, [action, file], f) {
    if (!['check', 'criteria', 'diagram'].includes(action) || !file) throw new UsageError('Usage: story machine check|criteria|diagram <file>');
    const name = modelName(file);
    const { machine, defects } = checkMachine(await readModel(file), { name });
    if (defects.length || action === 'check') {
      reportDefects(f, machine?.entity ?? name, defects, () =>
        `${machine.states.length} states, ${machine.events.length} events, ${machine.transitions.length} transitions`);
      return;
    }
    if (action === 'diagram') {
      const diagram = machineDiagram(machine);
      out(f, { diagram }, () => diagram);
      return;
    }
    const criteria = machineCriteria(machine);
    out(f, criteria, () => criteria.map((c) => `- [ ] ${c.text}`).join('\n'));
  },

  async table(_repo, [action, file], f) {
    if (!['check', 'criteria'].includes(action) || !file) throw new UsageError('Usage: story table check|criteria <file>');
    const { table: t, defects } = checkTable(await readModel(file), { name: modelName(file) });
    if (defects.length || action === 'check') {
      const combinations = t.inputs.reduce((p, i) => p * i.domain.length, 1);
      reportDefects(f, t.name, defects, () =>
        `${t.inputs.length} inputs, ${combinations} combinations, ${t.rules.length} rules, hit policy ${t.hitPolicy}`);
      return;
    }
    const c = tableCriteria(t);
    out(f, c, () => c.markdown);
  },

  async config(_repo, _args, f, ctx) {
    out(f, ctx.config, () => `# ${ctx.configFound ? ctx.configPath : 'no config file; defaults'}\n${JSON.stringify(ctx.config, null, 2)}`);
  },

  async stats(repo, _args, f) {
    const st = await backlogStats(repo, { staleDays: f['stale-days'] ? Number(f['stale-days']) : 30 });
    out(f, st, () =>
      [
        `Stories: ${st.total} total, ${st.open} open`,
        `By status: ${Object.entries(st.byStatus).map(([k, v]) => `${k} ${v}`).join(', ') || '-'}`,
        `Points: ${st.readyPoints} ready / ${st.openPoints} open`,
        `Unestimated: ${st.unestimated.join(', ') || 'none'}`,
        `Stale: ${st.stale.join(', ') || 'none'}`,
      ].join('\n'),
    );
  },
};

async function requireStory(repo, id) {
  if (!id) throw new UsageError('A story id is required.');
  const s = await repo.get(id);
  if (!s) throw new Error(`Story ${id} not found.`);
  return s;
}

function formatJudgement(j) {
  const d = j.decisions;
  const yn = (x) => (x.value ? 'yes' : 'no');
  const c = (x) => (typeof x.confidence === 'number' ? x.confidence.toFixed(2) : '-');
  const rows = [];
  if (d.negotiable) rows.push(['need, not solution', yn(d.negotiable), c(d.negotiable), d.negotiable.reason ?? '']);
  if (d.valuable) rows.push(['real benefit', yn(d.valuable), c(d.valuable), d.valuable.reason ?? '']);
  for (const k of d.criteria) rows.push([`criterion ${k.index} testable`, k.testable ? 'yes' : 'no', c(k), '']);
  rows.push(['type', d.type.value, c(d.type), '']);
  if (d.size) rows.push(['size', String(d.size.value), c(d.size), d.size.reason ?? '']);
  if (d.splitPattern) rows.push(['split pattern', d.splitPattern.value, c(d.splitPattern), '']);
  const head = `${j.id}  ${j.title}  [${j.source}${j.model ? ` ${j.model}` : ''}]`;
  const note = j.source === 'fallback' ? `\n  Jev unavailable (${j.reason}); code heuristics used.` : '';
  const usage = j.usage ? `\n  tokens: ${j.usage.inputTokens} in / ${j.usage.outputTokens} out, about $${j.usage.costUsd}` : '';
  const body = table(['Decision', 'Value', 'Conf', 'Why'], rows).split('\n').map((l) => `  ${l}`).join('\n');
  return `${head}${note}\n${body}${usage}`;
}

function formatLint(r) {
  const errors = r.findings.filter((x) => x.severity === 'error').length;
  const warnings = r.findings.length - errors;
  const verdict = r.ready ? 'READY' : 'NOT READY';
  const lines = r.findings.map((x) => `  ${x.severity.padEnd(7)} [${x.invest}] ${x.rule.padEnd(20)} ${x.message}`);
  return [`${r.id}  ${r.title}  ${verdict} (${errors} errors, ${warnings} warnings)`, ...lines].join('\n');
}

function table(headers, rows) {
  const widths = headers.map((h, i) => Math.max(h.length, ...rows.map((r) => String(r[i]).length)));
  const line = (cells) => cells.map((c, i) => String(c).padEnd(widths[i])).join('  ').trimEnd();
  return [line(headers), ...rows.map(line)].join('\n');
}

const exists = (p) => access(p).then(() => true, () => false);

// Story fields that name a file (state_machine) are relative to the project root.
const inRepo = (ctx) => (path) => existsSync(resolve(ctx.root, path));

const modelName = (file) => basename(file).replace(/\.toml$/, '');

async function readModel(file) {
  let text;
  try {
    text = await readFile(file, 'utf8');
  } catch (e) {
    throw new Error(e.code === 'ENOENT' ? `${file}: not found.` : `${file}: ${e.message}`);
  }
  try {
    return parseToml(text);
  } catch (e) {
    throw new Error(`${file}: ${e.message}`);
  }
}

function reportDefects(flags, name, defects, summary) {
  out(flags, { ok: !defects.length, defects }, () =>
    defects.length
      ? [`${name}: ${defects.length} defect${defects.length === 1 ? '' : 's'}`, ...defects.map((d) => `  ${d.code.padEnd(14)} ${d.message}`)].join('\n')
      : `${name}: OK (${summary()})`,
  );
  if (defects.length) process.exitCode = 1;
}

async function makeGateway(ctx) {
  const { config } = ctx;
  const store = new FileSecretStore(join(ctx.root, CONFIG_DIR));
  if (config.tracker === 'gitea') {
    // Like gh/az, an already-set variable (CI, a shell export) wins over the file.
    const token = process.env[SECRET_SPECS.gitea.variable] || (await store.load('gitea'));
    return new GiteaIssuesGateway({ gitea: config.gitea, token });
  }
  const env = tokenEnv(config.tracker, { file: await store.load(config.tracker) });
  if (config.tracker === 'github') return new GitHubIssuesGateway({ repo: config.github.repo, exec: processExec, env });
  return new AdoBoardsGateway({ ado: config.ado, exec: processExec, env });
}

// Reads a token without echoing it: a raw-mode prompt on a terminal, or the
// first line of stdin when piped. The token never touches argv.
async function readToken(prompt) {
  const stdin = process.stdin;
  if (!stdin.isTTY) {
    let data = '';
    for await (const chunk of stdin) data += chunk;
    return data.split(/\r?\n/)[0].trim();
  }
  process.stderr.write(prompt);
  stdin.setRawMode(true);
  stdin.resume();
  stdin.setEncoding('utf8');
  return new Promise((resolveToken, reject) => {
    let value = '';
    const finish = (fn) => {
      stdin.setRawMode(false);
      stdin.pause();
      stdin.removeListener('data', onData);
      process.stderr.write('\n');
      fn();
    };
    const onData = (chunk) => {
      for (const ch of chunk) {
        if (ch === '\r' || ch === '\n') return finish(() => resolveToken(value.trim()));
        if (ch === '\u0003') return finish(() => reject(new Error('Cancelled.')));
        if (ch === '\u007f' || ch === '\b') value = value.slice(0, -1);
        else value += ch;
      }
    };
    stdin.on('data', onData);
  });
}

async function resolveConfigPath(flags) {
  const explicit = flags.config ?? process.env.STORY_TIME_CONFIG;
  if (explicit) return resolve(explicit);
  if (await exists(resolve(CONFIG_FILE))) return resolve(CONFIG_FILE);
  if (await exists(resolve(LEGACY_CONFIG_FILE))) return resolve(LEGACY_CONFIG_FILE);
  return resolve(CONFIG_FILE);
}

// .story-time/settings.json lives one level below the project root.
function projectRoot(configPath) {
  const dir = dirname(configPath);
  return basename(dir) === CONFIG_DIR ? dirname(dir) : dir;
}

async function loadConfig(path) {
  let text;
  try {
    text = await readFile(path, 'utf8');
  } catch (e) {
    if (e.code === 'ENOENT') return { config: parseConfig({}), found: false };
    throw e;
  }
  try {
    return { config: parseConfig(JSON.parse(text)), found: true };
  } catch (e) {
    throw new Error(`${path}: ${e.message}`);
  }
}

function out(flags, data, human) {
  console.log(flags.json ? JSON.stringify(data, null, 2) : human());
}

async function main(argv) {
  const [command = 'help', ...rest] = argv;
  const handler = commands[command];
  if (!handler) {
    const problem = NATIVE_ONLY.has(command)
      ? `The "${command}" command runs only in the native story binary (native/story-push); this Node CLI cannot run it.`
      : `Unknown command "${command}".`;
    console.error(`${problem}\n\n${USAGE}`);
    return 2;
  }
  try {
    const { positional, flags } = parseArgs(rest);
    const configPath = command === 'init' ? resolve(flags.config ?? process.env.STORY_TIME_CONFIG ?? CONFIG_FILE) : await resolveConfigPath(flags);
    const root = projectRoot(configPath);
    // init must work even when the existing config is broken, so it skips loading.
    const { config, found } = command === 'init' ? { config: parseConfig({}), found: false } : await loadConfig(configPath);
    const dir = resolve(flags.dir ?? process.env.STORY_TIME_DIR ?? resolve(root, config.backlogDir));
    const ctx = { config, configPath, configFound: found, root };
    await handler(new FileSystemStoryRepository(dir, { reservedIds: gitReservedIds(dir) }), positional, flags, ctx);
    return process.exitCode ?? 0;
  } catch (e) {
    console.error(e.message);
    return e instanceof UsageError ? 2 : 1;
  }
}

process.exitCode = await main(process.argv.slice(2));
