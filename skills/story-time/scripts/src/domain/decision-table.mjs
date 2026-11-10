// A business rule as a decision table (finite input domains, condition rows, a hit policy):
// check it for gaps and overlaps, and derive one parameterized acceptance criterion. Pure, no I/O.

export const HIT_POLICIES = ['unique', 'first'];
export const ANY = '-';
const MAX_COMBINATIONS = 4096;
const isScalar = (v) => ['string', 'number', 'boolean'].includes(typeof v);
const defect = (code, message, extra = {}) => ({ code, message, ...extra });
const show = (combo) => Object.entries(combo).map(([k, v]) => `${k}=${v}`).join(', ');

export function checkTable(doc, { name } = {}) {
  const defects = [];
  const schema = (message) => defects.push(defect('schema', message));

  if (!HIT_POLICIES.includes(doc.hit_policy)) {
    schema(`hit_policy must be "unique" or "first", got ${JSON.stringify(doc.hit_policy ?? null)}.`);
  }
  const inputDoc = doc.inputs && typeof doc.inputs === 'object' && !Array.isArray(doc.inputs) ? doc.inputs : {};
  const inputs = Object.entries(inputDoc).map(([n, domain]) => ({ name: n, domain }));
  if (!inputs.length) schema('inputs must be an [inputs] table of name = [values].');
  for (const i of inputs) {
    if (!Array.isArray(i.domain) || !i.domain.length || !i.domain.every(isScalar)) {
      schema(`Input "${i.name}" needs a non-empty list of values.`);
    } else if (new Set(i.domain).size !== i.domain.length) schema(`Input "${i.name}" lists a value twice.`);
  }
  const outputs = Array.isArray(doc.outputs) ? doc.outputs : [];
  if (!outputs.length || !outputs.every((o) => typeof o === 'string')) schema('outputs must be a non-empty list of names.');
  if (!Array.isArray(doc.rules) || !doc.rules.length) schema('rules must be one or more [[rules]] tables.');

  const rules = (Array.isArray(doc.rules) ? doc.rules : []).map((r, idx) => {
    const n = idx + 1;
    const when = r.when ?? {};
    const then = r.then ?? {};
    for (const i of inputs) {
      if (!Object.hasOwn(when, i.name)) schema(`Rule ${n}: no condition for input "${i.name}"; write "-" for any value.`);
      else if (when[i.name] !== ANY && Array.isArray(i.domain) && !i.domain.includes(when[i.name])) {
        schema(`Rule ${n}: ${i.name} = ${JSON.stringify(when[i.name])} is not in its domain (${i.domain.join(', ')}).`);
      }
    }
    for (const k of Object.keys(when)) if (!Object.hasOwn(inputDoc, k)) schema(`Rule ${n}: unknown input "${k}".`);
    for (const o of outputs) if (!Object.hasOwn(then, o)) schema(`Rule ${n}: no value for output "${o}".`);
    for (const k of Object.keys(then)) if (!outputs.includes(k)) schema(`Rule ${n}: unknown output "${k}".`);
    return { when, then };
  });

  const table = { name: doc.name ?? name ?? 'table', hitPolicy: doc.hit_policy, inputs, outputs, rules };
  if (defects.length) return { table, defects };

  const total = inputs.reduce((p, i) => p * i.domain.length, 1);
  if (total > MAX_COMBINATIONS) {
    return { table, defects: [defect('schema', `${total} input combinations is more than ${MAX_COMBINATIONS}. Split the table.`)] };
  }

  const fires = new Set();
  const overlaps = new Map();
  for (const combo of combinations(inputs)) {
    const hits = matching(table, combo);
    if (!hits.length) defects.push(defect('uncovered', `No rule covers ${show(combo)}.`, { combination: combo }));
    else fires.add(hits[0]);
    if (table.hitPolicy === 'unique') {
      for (let a = 0; a < hits.length; a++) {
        for (let b = a + 1; b < hits.length; b++) {
          const k = `${hits[a]},${hits[b]}`;
          if (!overlaps.has(k)) overlaps.set(k, { pair: [hits[a] + 1, hits[b] + 1], combo });
        }
      }
    }
  }
  for (const { pair, combo } of overlaps.values()) {
    defects.push(defect('overlap', `Rules ${pair[0]} and ${pair[1]} both match ${show(combo)} (hit policy unique).`, { rules: pair }));
  }
  if (table.hitPolicy === 'first') {
    rules.forEach((_, i) => {
      if (!fires.has(i)) {
        defects.push(defect('unreachable', `Rule ${i + 1} never fires: earlier rules match every combination it covers (hit policy first).`, { rules: [i + 1] }));
      }
    });
  }
  return { table, defects };
}

// One Given/When/Then with a row per input combination and the rule that decides it.
export function tableCriteria(table) {
  const criterion = `Given the inputs in a row of the ${table.name} table, when the rule is evaluated, then the result is the outputs in that row.`;
  const columns = ['rule', ...table.inputs.map((i) => i.name), ...table.outputs];
  const rows = [...combinations(table.inputs)].map((combo) => {
    const r = matching(table, combo)[0];
    return [r + 1, ...Object.values(combo), ...table.outputs.map((o) => table.rules[r].then[o])];
  });
  const row = (cells) => `  | ${cells.join(' | ')} |`;
  const markdown = [`- [ ] ${criterion}`, '', row(columns), `  |${'---|'.repeat(columns.length)}`, ...rows.map(row)].join('\n');
  return { criterion, columns, rows, markdown };
}

function matching(table, combo) {
  const hits = [];
  table.rules.forEach((r, i) => {
    if (table.inputs.every((inp) => r.when[inp.name] === ANY || r.when[inp.name] === combo[inp.name])) hits.push(i);
  });
  return hits;
}

function* combinations(inputs, prefix = {}) {
  if (!inputs.length) {
    yield prefix;
    return;
  }
  const [head, ...rest] = inputs;
  for (const v of head.domain) yield* combinations(rest, { ...prefix, [head.name]: v });
}
