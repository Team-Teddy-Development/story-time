// Project configuration (.story-time/settings.json): what this project uses.
// parseConfig is pure: raw JSON in, validated config with defaults out.
// Reading the file is the CLI's job (boundary discipline).

export const TRACKERS = ['files', 'github', 'ado', 'jira', 'linear'];
export const ADO_PROCESSES = ['agile', 'scrum', 'basic'];
export const SYNC_MODES = ['off', 'ask', 'auto'];
export const PUSHABLE_TRACKERS = ['github', 'ado'];
export const CONFIG_DIR = '.story-time';
export const CONFIG_FILE = `${CONFIG_DIR}/settings.json`;
export const LEGACY_CONFIG_FILE = '.story-time.json';

const KNOWN_KEYS = new Set([
  '$schema', 'tracker', 'sync', 'backlogDir', 'prefix', 'github', 'ado', 'jira', 'linear',
  'team', 'project', 'definitionOfDone', 'definitionOfReady', 'jev',
]);

export function parseConfig(raw = {}) {
  if (raw === null || typeof raw !== 'object' || Array.isArray(raw)) throw new Error('Config must be a JSON object.');
  for (const key of Object.keys(raw)) {
    if (!KNOWN_KEYS.has(key)) throw new Error(`Unknown config key "${key}".`);
  }

  const tracker = raw.tracker ?? 'files';
  if (!TRACKERS.includes(tracker)) throw new Error(`Unknown tracker "${tracker}". Use one of: ${TRACKERS.join(', ')}.`);

  const prefix = raw.prefix ?? 'ST';
  if (!/^[A-Z][A-Z0-9]*$/.test(prefix)) throw new Error(`prefix must be uppercase letters/digits, e.g. "ST" or "PAY"; got "${prefix}".`);

  const sync = raw.sync ?? 'off';
  if (!SYNC_MODES.includes(sync)) throw new Error(`sync must be one of: ${SYNC_MODES.join(', ')}; got "${sync}".`);
  if (sync !== 'off' && !PUSHABLE_TRACKERS.includes(tracker)) {
    throw new Error(`sync "${sync}" needs tracker github or ado; tracker is "${tracker}". Use sync "off" and story export cards.`);
  }

  const config = {
    tracker,
    sync,
    backlogDir: str(raw.backlogDir, 'backlogDir') ?? 'backlog',
    prefix,
    team: parseTeam(raw.team ?? {}),
    project: parseProject(raw.project ?? {}),
    definitionOfDone: strList(raw.definitionOfDone, 'definitionOfDone'),
    definitionOfReady: strList(raw.definitionOfReady, 'definitionOfReady'),
  };

  if (raw.github || tracker === 'github') config.github = parseGithub(raw.github ?? {});
  if (raw.ado || tracker === 'ado') config.ado = parseAdo(raw.ado ?? {});
  if (raw.jira) config.jira = { ...raw.jira };
  if (raw.linear) config.linear = { ...raw.linear };
  if (raw.jev) config.jev = parseJev(raw.jev);
  return config;
}

// Pushing is opt-in per project. Throws with the reason and the way out.
export function pushPolicy(config) {
  if (config.sync === 'off') {
    throw new Error(
      'Pushing is off for this project (sync: "off" in .story-time/settings.json). ' +
        'Use story export for a copyable card, or run the story-setup skill to set sync to "ask" or "auto".',
    );
  }
  if (!PUSHABLE_TRACKERS.includes(config.tracker)) throw new Error(`story push supports github and ado; tracker is "${config.tracker}".`);
}

export function defaultExportFormat(config) {
  return config.tracker === 'github' || config.tracker === 'ado' ? config.tracker : 'markdown';
}

// Jev settings carry no secrets: the key is stored with `story secret set jev`.
function parseJev(j) {
  for (const key of Object.keys(j)) {
    if (key !== 'model') throw new Error(`Unknown jev key "${key}". Keys never go in settings; store one with: story secret set jev`);
  }
  if (j.model !== undefined && !/^jev-[\w.-]+$/.test(String(j.model))) {
    throw new Error(`jev.model must be a Jev model id such as "jev-1.13.0"; got "${j.model}".`);
  }
  return j.model ? { model: j.model } : {};
}

function parseGithub(g) {
  if (!g.repo) throw new Error('tracker "github" needs github.repo, e.g. "acme/shop".');
  if (!/^[\w.-]+\/[\w.-]+$/.test(g.repo)) throw new Error(`github.repo must be owner/repo; got "${g.repo}".`);
  return { repo: g.repo };
}

function parseAdo(a) {
  if (!a.organization) throw new Error('tracker "ado" needs ado.organization, e.g. "https://dev.azure.com/acme".');
  if (!a.project) throw new Error('tracker "ado" needs ado.project.');
  const process = a.process ?? 'agile';
  if (!ADO_PROCESSES.includes(process)) throw new Error(`ado.process must be one of: ${ADO_PROCESSES.join(', ')}; got "${process}".`);
  return {
    organization: a.organization,
    project: a.project,
    process,
    areaPath: str(a.areaPath, 'ado.areaPath'),
    iterationPath: str(a.iterationPath, 'ado.iterationPath'),
  };
}

function parseTeam(t) {
  const sprintDays = t.sprintDays ?? 10;
  const wipLimit = t.wipLimit ?? 1;
  if (!Number.isInteger(sprintDays) || sprintDays < 1) throw new Error('team.sprintDays must be a positive integer.');
  if (!Number.isInteger(wipLimit) || wipLimit < 1) throw new Error('team.wipLimit must be a positive integer.');
  if (t.velocity !== undefined && !(typeof t.velocity === 'number' && t.velocity > 0)) {
    throw new Error('team.velocity must be a positive number of points per sprint.');
  }
  return { sprintDays, velocity: t.velocity, wipLimit };
}

function parseProject(p) {
  return {
    stack: strList(p.stack, 'project.stack'),
    testFramework: str(p.testFramework, 'project.testFramework'),
    testCommand: str(p.testCommand, 'project.testCommand'),
  };
}

function str(v, name) {
  if (v === undefined || v === null || v === '') return undefined;
  if (typeof v !== 'string') throw new Error(`${name} must be a string.`);
  return v;
}

function strList(v, name) {
  if (v === undefined) return [];
  if (!Array.isArray(v) || !v.every((x) => typeof x === 'string')) throw new Error(`${name} must be a list of strings.`);
  return [...v];
}
