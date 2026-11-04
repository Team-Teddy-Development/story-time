#!/usr/bin/env node
// story — CLI for the story-time backlog. This file is the composition root
// and the only boundary: it parses argv, picks the adapter, calls use cases,
// and formats output. No business rules live here.
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { FileSystemStoryRepository } from './src/adapters/fs-story-repository.mjs';
import { lintStory } from './src/domain/invest.mjs';
import { rankBacklog } from './src/domain/wsjf.mjs';
import { serializeStory } from './src/domain/story.mjs';
import { createStory } from './src/application/create-story.mjs';
import { updateStory } from './src/application/update-story.mjs';
import { splitStory } from './src/application/split-story.mjs';
import { backlogStats } from './src/application/backlog-stats.mjs';

const USAGE = `story — agile backlog toolkit (story-time)

Usage: story <command> [args] [--json]

Commands:
  new    --title T [--type story|spike|bug|chore] [--statement S] [--ac A]... [--epic E]
         [--persona P] [--size N] [--value N] [--time-criticality N] [--risk-reduction N]
         [--timebox 2d] [--question Q] [--prefix ST]
  list   [--status S]                      List stories
  show   <id>                              Print a story file
  lint   [id...]                           INVEST / Definition of Ready check (exit 1 on errors)
  rank   [--limit N] [--include-refined]   WSJF-ranked ready stories, with exclusions
  set    <id> key=value... [--note N] [--force]
                                           Update fields; status=ready is lint-gated
  split  <id> --from children.json         Split into children (saga: all or nothing)
  stats  [--stale-days 30]                 Backlog health summary
  help                                     This text

Backlog directory: --dir, else $STORY_TIME_DIR, else ./backlog`;

const BOOLEAN = new Set(['json', 'force', 'include-refined']);
const REPEATABLE = new Set(['ac']);

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

  async new(repo, _args, f) {
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
      { prefix: f.prefix ?? process.env.STORY_TIME_PREFIX ?? 'ST' },
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

  async lint(repo, ids, f) {
    const all = await repo.list();
    const targets = ids.length ? await Promise.all(ids.map((id) => requireStory(repo, id))) : all;
    const results = targets.map((s) => ({ ...lintStory(s, { stories: all }), title: s.title }));
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

  async set(repo, [id, ...pairs], f) {
    if (!id || !pairs.length) throw new UsageError('Usage: story set <id> key=value...');
    const changes = Object.fromEntries(
      pairs.map((p) => {
        const i = p.indexOf('=');
        if (i < 1) throw new UsageError(`Expected key=value, got "${p}".`);
        return [p.slice(0, i).replace(/-/g, '_'), p.slice(i + 1)];
      }),
    );
    const s = await updateStory(repo, id, changes, { note: f.note, force: f.force });
    out(f, s, () => `${s.id} updated.`);
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

function formatLint(r) {
  const errors = r.findings.filter((x) => x.severity === 'error').length;
  const warnings = r.findings.length - errors;
  const verdict = r.ready ? 'READY' : 'NOT READY';
  const lines = r.findings.map((x) => `  ${x.severity.padEnd(7)} [${x.invest}] ${x.rule.padEnd(18)} ${x.message}`);
  return [`${r.id}  ${r.title}  ${verdict} (${errors} errors, ${warnings} warnings)`, ...lines].join('\n');
}

function table(headers, rows) {
  const widths = headers.map((h, i) => Math.max(h.length, ...rows.map((r) => String(r[i]).length)));
  const line = (cells) => cells.map((c, i) => String(c).padEnd(widths[i])).join('  ').trimEnd();
  return [line(headers), ...rows.map(line)].join('\n');
}

function out(flags, data, human) {
  console.log(flags.json ? JSON.stringify(data, null, 2) : human());
}

async function main(argv) {
  const [command = 'help', ...rest] = argv;
  const handler = commands[command];
  if (!handler) {
    console.error(`Unknown command "${command}".\n\n${USAGE}`);
    return 2;
  }
  try {
    const { positional, flags } = parseArgs(rest);
    const dir = resolve(flags.dir ?? process.env.STORY_TIME_DIR ?? 'backlog');
    await handler(new FileSystemStoryRepository(dir), positional, flags);
    return process.exitCode ?? 0;
  } catch (e) {
    console.error(e.message);
    return e instanceof UsageError ? 2 : 1;
  }
}

process.exitCode = await main(process.argv.slice(2));
