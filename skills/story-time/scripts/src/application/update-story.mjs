import { STATUSES, TYPES, appendChangelog, today } from '../domain/story.mjs';
import { lintStory } from '../domain/invest.mjs';
import { assertStoryRepository } from '../ports/story-repository.mjs';

const NUMERIC = new Set(['value', 'time_criticality', 'risk_reduction', 'size']);
const LIST = new Set(['depends_on', 'children']);
const IMMUTABLE = new Set(['id', 'created', 'updated', 'sections']);

// Applies field changes with domain validation and an audit trail.
// Moving to "ready" is gated by the INVEST lint unless force is set.
export async function updateStory(repo, id, changes, { note, force = false, date = today(), exists } = {}) {
  assertStoryRepository(repo);
  const current = await repo.get(id);
  if (!current) throw new Error(`Story ${id} not found.`);

  const next = { ...current };
  for (const [key, raw] of Object.entries(changes)) {
    if (IMMUTABLE.has(key)) throw new Error(`Field "${key}" cannot be set directly.`);
    const value = coerce(key, raw);
    if (value === undefined) delete next[key];
    else next[key] = value;
  }

  if (next.status === 'ready' && current.status !== 'ready' && !force) {
    const { findings } = lintStory(next, { stories: await repo.list(), exists });
    const errors = findings.filter((f) => f.severity === 'error');
    if (errors.length) {
      throw new Error(`${id} is not ready:\n${errors.map((e) => `  ${e.rule}: ${e.message}`).join('\n')}`);
    }
  }

  const summary = Object.keys(changes).map((k) => `${k}=${display(next[k])}`).join(', ');
  const updated = appendChangelog(next, `set ${summary}${note ? ` — ${note}` : ''}`, date);
  await repo.save(updated);
  return updated;
}

function coerce(key, raw) {
  if (raw === '' || raw === null || raw === undefined) return undefined;
  if (NUMERIC.has(key)) {
    const n = typeof raw === 'number' ? raw : Number(raw);
    if (!Number.isFinite(n)) throw new Error(`${key} must be a number, got "${raw}".`);
    return n;
  }
  if (LIST.has(key)) {
    if (Array.isArray(raw)) return raw;
    const text = String(raw).trim();
    const items = text.startsWith('[') && text.endsWith(']') ? text.slice(1, -1) : text;
    return items.split(',').map((s) => s.trim()).filter(Boolean);
  }
  if (key === 'status' && !STATUSES.includes(raw)) throw new Error(`Unknown status "${raw}". Use one of: ${STATUSES.join(', ')}.`);
  if (key === 'type' && !TYPES.includes(raw)) throw new Error(`Unknown type "${raw}". Use one of: ${TYPES.join(', ')}.`);
  return raw;
}

function display(v) {
  if (v === undefined) return '';
  return Array.isArray(v) ? v.join(',') : String(v);
}
