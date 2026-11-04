// Weighted Shortest Job First. Pure ranking; no I/O.
// cost of delay = user/business value + time criticality + risk reduction/opportunity enablement
// score = cost of delay / job size

const isNum = (v) => typeof v === 'number' && Number.isFinite(v);

export function wsjf(s) {
  const parts = [s.value, s.time_criticality, s.risk_reduction];
  if (!parts.every(isNum) || !isNum(s.size) || s.size <= 0) return null;
  const costOfDelay = parts.reduce((a, b) => a + b, 0);
  return { costOfDelay, score: round(costOfDelay / s.size) };
}

const FINISHED = new Set(['done', 'split', 'archived']);

export function rankBacklog(stories, { statuses = ['ready'] } = {}) {
  const byId = new Map(stories.map((s) => [s.id, s]));
  const ranked = [];
  const excluded = [];
  for (const s of stories) {
    if (FINISHED.has(s.status)) continue;
    if (!statuses.includes(s.status)) {
      excluded.push({ story: s, reason: `status ${s.status}` });
      continue;
    }
    const blockers = (s.depends_on ?? []).filter((id) => byId.get(id)?.status !== 'done');
    if (blockers.length) {
      excluded.push({ story: s, reason: `blocked by ${blockers.join(', ')}` });
      continue;
    }
    const w = wsjf(s);
    if (!w) {
      excluded.push({ story: s, reason: 'missing WSJF inputs (value, time_criticality, risk_reduction, size)' });
      continue;
    }
    ranked.push({ story: s, ...w });
  }
  ranked.sort(
    (a, b) =>
      b.score - a.score ||
      a.story.size - b.story.size ||
      String(a.story.created ?? '').localeCompare(String(b.story.created ?? '')),
  );
  return { ranked, excluded };
}

function round(n) {
  return Math.round(n * 100) / 100;
}
