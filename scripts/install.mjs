#!/usr/bin/env node
// Install story-time skills for Claude Code and/or Codex.
//
//   node scripts/install.mjs --agent codex|claude|all [--scope user|project]
//                            [--project DIR] [--codex-home DIR] [--force] [--dry-run] [--uninstall]
//
// planInstall is pure (targets in, copy plan out); main() is the I/O shell.
// Every destination is checked before anything is written, so an install
// either completes or changes nothing.
import { cp, readdir, rm, access } from 'node:fs/promises';
import { homedir } from 'node:os';
import { dirname, join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SKILLS_DIR = join(ROOT, 'skills');

// Where each agent discovers skills. Codex: ~/.agents/skills (current) or
// $CODEX_HOME/skills (deprecated, still read); repo scope: <repo>/.agents/skills.
export const AGENTS = {
  claude: {
    user: ({ home }) => join(home, '.claude', 'skills'),
    project: ({ project }) => join(project, '.claude', 'skills'),
  },
  codex: {
    user: ({ home, codexHome }) => (codexHome ? join(codexHome, 'skills') : join(home, '.agents', 'skills')),
    project: ({ project }) => join(project, '.agents', 'skills'),
  },
};

export function planInstall({ agents, scope = 'user', home, project = process.cwd(), codexHome, skills }) {
  return agents.flatMap((agent) => {
    const target = AGENTS[agent];
    if (!target) throw new Error(`Unknown agent "${agent}". Use: ${Object.keys(AGENTS).join(', ')}, all.`);
    const resolveRoot = target[scope];
    if (!resolveRoot) throw new Error(`Unknown scope "${scope}". Use: user, project.`);
    const root = resolveRoot({ home, project, codexHome });
    return skills.map((skill) => ({ agent, skill, src: join(SKILLS_DIR, skill), dest: join(root, skill) }));
  });
}

const exists = (p) => access(p).then(() => true, () => false);

// Ship the skill, not its test suite.
const shipFilter = (src) => !src.split(sep).join('/').includes('/scripts/test');

class UsageError extends Error {}

function parseArgs(argv) {
  const flags = { scope: 'user' };
  const valued = new Set(['agent', 'scope', 'project', 'codex-home']);
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith('--')) throw new UsageError(`Unexpected argument "${a}".`);
    const key = a.slice(2);
    if (valued.has(key)) {
      if (argv[i + 1] === undefined) throw new UsageError(`--${key} needs a value.`);
      flags[key] = argv[++i];
    } else if (['force', 'dry-run', 'uninstall', 'help'].includes(key)) {
      flags[key] = true;
    } else {
      throw new UsageError(`Unknown flag ${a}.`);
    }
  }
  return flags;
}

const USAGE = `Usage: node scripts/install.mjs --agent codex|claude|all [--scope user|project]
                                [--project DIR] [--codex-home DIR] [--force] [--dry-run] [--uninstall]

  codex  user    ~/.agents/skills      (or $CODEX_HOME/skills with --codex-home)
  codex  project <project>/.agents/skills
  claude user    ~/.claude/skills
  claude project <project>/.claude/skills`;

async function main(argv) {
  const flags = parseArgs(argv);
  if (flags.help) {
    console.log(USAGE);
    return 0;
  }
  if (!flags.agent) throw new UsageError(`--agent is required.\n\n${USAGE}`);
  const agents = flags.agent === 'all' ? Object.keys(AGENTS) : [flags.agent];
  const skills = (await readdir(SKILLS_DIR, { withFileTypes: true })).filter((d) => d.isDirectory()).map((d) => d.name).sort();
  const plan = planInstall({
    agents,
    scope: flags.scope,
    home: homedir(),
    project: resolve(flags.project ?? process.cwd()),
    codexHome: flags['codex-home'],
    skills,
  });

  if (flags.uninstall) {
    for (const p of plan) {
      if (!(await exists(p.dest))) continue;
      if (flags['dry-run']) console.log(`would remove ${p.dest}`);
      else await rm(p.dest, { recursive: true, force: true });
    }
    if (!flags['dry-run']) console.log(`Removed story-time skills for ${agents.join(', ')}.`);
    return 0;
  }

  const clashes = [];
  for (const p of plan) if (await exists(p.dest)) clashes.push(p.dest);
  if (clashes.length && !flags.force) {
    console.error(`A skill already exists at each of these paths:\n${clashes.map((c) => `  ${c}`).join('\n')}\nRe-run with --force to replace them (local edits will be lost).`);
    return 1;
  }

  for (const p of plan) {
    if (flags['dry-run']) {
      console.log(`${p.agent.padEnd(6)} ${p.skill.padEnd(20)} -> ${p.dest}`);
      continue;
    }
    if (flags.force) await rm(p.dest, { recursive: true, force: true });
    await cp(p.src, p.dest, { recursive: true, filter: shipFilter });
  }
  if (!flags['dry-run']) {
    const roots = [...new Set(plan.map((p) => dirname(p.dest)))];
    console.log(`Installed ${skills.length} story-time skills into:\n${roots.map((r) => `  ${r}`).join('\n')}`);
    if (agents.includes('codex')) console.log('Codex: restart codex, then try "$story-time" or "$story-pick".');
  }
  return 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    process.exitCode = await main(process.argv.slice(2));
  } catch (e) {
    console.error(e.message);
    process.exitCode = e instanceof UsageError ? 2 : 1;
  }
}
