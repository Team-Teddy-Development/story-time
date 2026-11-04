// Story entity: pure parse/serialize of the markdown + frontmatter format.
// No I/O here. Adapters own the filesystem; this module owns the format.

export const STATUSES = ['draft', 'refined', 'ready', 'in-progress', 'done', 'split', 'archived'];
export const TYPES = ['story', 'spike', 'bug', 'chore'];

// Canonical key order so serialized files diff cleanly.
const KEY_ORDER = [
  'id', 'title', 'status', 'type', 'epic', 'persona',
  'value', 'time_criticality', 'risk_reduction', 'size', 'timebox',
  'depends_on', 'parent', 'children', 'created', 'updated',
];

const FRONTMATTER = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/;

export function parseStory(text) {
  const m = FRONTMATTER.exec(text);
  if (!m) throw new Error('Story is missing YAML frontmatter (--- ... ---)');
  return { ...parseFrontmatter(m[1]), sections: parseSections(m[2]) };
}

export function serializeStory(story) {
  const { sections = {}, ...meta } = story;
  const keys = [
    ...KEY_ORDER.filter((k) => k in meta),
    ...Object.keys(meta).filter((k) => !KEY_ORDER.includes(k)).sort(),
  ];
  const fm = keys
    .filter((k) => meta[k] !== undefined && meta[k] !== null)
    .map((k) => `${k}: ${formatValue(meta[k])}`)
    .join('\n');
  const body = Object.entries(sections)
    .map(([name, content]) => `## ${name}\n${content}`.trimEnd())
    .join('\n\n');
  return `---\n${fm}\n---\n${body ? `\n${body}\n` : ''}`;
}

export function acceptanceCriteria(story) {
  const raw = story.sections?.['Acceptance Criteria'] ?? story.sections?.['Exit Criteria'] ?? '';
  return listItems(raw);
}

export function listItems(raw) {
  return raw
    .split(/\r?\n/)
    .map((line) => /^\s*[-*]\s+(?:\[( |x|X)\]\s+)?(.+)$/.exec(line))
    .filter(Boolean)
    .map(([, box, text]) => ({ text: text.trim(), done: box === 'x' || box === 'X' }));
}

export function appendChangelog(story, note, date = today()) {
  const prev = story.sections?.Changelog?.trimEnd() ?? '';
  const entry = `- ${date} ${note}`;
  return {
    ...story,
    updated: date,
    sections: { ...story.sections, Changelog: prev ? `${prev}\n${entry}` : entry },
  };
}

export function today() {
  return new Date().toISOString().slice(0, 10);
}

// --- frontmatter: the flat subset of YAML this format uses ---

function parseFrontmatter(src) {
  const out = {};
  for (const line of src.split(/\r?\n/)) {
    if (!line.trim() || line.trimStart().startsWith('#')) continue;
    const m = /^([A-Za-z_][\w-]*):\s*(.*)$/.exec(line);
    if (!m) throw new Error(`Unsupported frontmatter line: ${line}`);
    out[m[1]] = parseValue(stripComment(m[2]));
  }
  return out;
}

function stripComment(v) {
  if (/^["']/.test(v)) return v;
  const i = v.indexOf(' #');
  return (i === -1 ? v : v.slice(0, i)).trim();
}

function parseValue(v) {
  if (v === '' || v === '~' || v === 'null') return undefined;
  if (v.startsWith('[') && v.endsWith(']')) {
    const inner = v.slice(1, -1).trim();
    return inner ? inner.split(',').map((x) => parseValue(x.trim())) : [];
  }
  if (/^".*"$/.test(v)) return JSON.parse(v);
  if (/^'.*'$/.test(v)) return v.slice(1, -1).replace(/''/g, "'");
  if (v === 'true' || v === 'false') return v === 'true';
  if (/^-?\d+(\.\d+)?$/.test(v)) return Number(v);
  return v;
}

function formatValue(v) {
  if (Array.isArray(v)) return `[${v.map(formatValue).join(', ')}]`;
  if (typeof v === 'number' || typeof v === 'boolean') return String(v);
  const s = String(v);
  const needsQuotes = /[:#\[\],"']|^\s|\s$|^(true|false|null|~|-?\d+(\.\d+)?)$/.test(s) || s === '';
  return needsQuotes ? JSON.stringify(s) : s;
}

function parseSections(body) {
  const sections = {};
  let current = null;
  let buf = [];
  const flush = () => {
    if (current !== null) sections[current] = buf.join('\n').trim();
  };
  for (const line of body.split(/\r?\n/)) {
    const h = /^##\s+(.+?)\s*$/.exec(line);
    if (h) {
      flush();
      current = h[1];
      buf = [];
    } else if (current !== null) {
      buf.push(line);
    }
  }
  flush();
  return sections;
}
