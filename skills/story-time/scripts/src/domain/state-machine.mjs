// An entity lifecycle as data (model/state/<entity>.toml): check it for defects, and derive its
// acceptance criteria and diagram. Pure, no I/O.

const isNameList = (v) => Array.isArray(v) && v.length > 0 && v.every((x) => typeof x === 'string' && x);
const defect = (code, message) => ({ code, message });

// Normalizes a parsed document into a machine and lists every defect. Criteria and diagrams are
// only meaningful when defects is empty.
export function checkMachine(doc, { name } = {}) {
  const defects = [];
  if (!isNameList(doc.states)) defects.push(defect('schema', 'states must be a non-empty list of names.'));
  if (typeof doc.initial !== 'string' || !doc.initial) defects.push(defect('schema', 'initial must name a state.'));
  if (!isNameList(doc.events)) defects.push(defect('schema', 'events must be a non-empty list of names.'));
  if (!Array.isArray(doc.transitions) || !doc.transitions.length) {
    defects.push(defect('schema', 'transitions must be one or more [[transitions]] tables.'));
  }
  if (doc.terminal !== undefined && !(Array.isArray(doc.terminal) && doc.terminal.every((x) => typeof x === 'string'))) {
    defects.push(defect('schema', 'terminal must be a list of state names.'));
  }
  if (doc.guards !== undefined && (typeof doc.guards !== 'object' || Array.isArray(doc.guards))) {
    defects.push(defect('schema', 'guards must be a [guards] table of name = "description".'));
  }
  if (defects.length) return { machine: undefined, defects };

  const machine = {
    entity: doc.entity ?? doc.name ?? name ?? 'entity',
    states: doc.states,
    initial: doc.initial,
    terminal: doc.terminal ?? [],
    events: doc.events,
    guards: doc.guards ?? {},
    transitions: doc.transitions.map((t) => ({
      from: t.from,
      to: t.to,
      event: t.event,
      ...(t.guard === undefined ? {} : { guard: t.guard }),
    })),
    invariants: doc.invariants ?? [],
  };

  const states = new Set(machine.states);
  const events = new Set(machine.events);
  const terminal = new Set(machine.terminal);
  if (!states.has(machine.initial)) defects.push(defect('unknown-state', `Initial state "${machine.initial}" is not in states.`));
  for (const s of machine.terminal) {
    if (!states.has(s)) defects.push(defect('unknown-state', `Terminal state "${s}" is not in states.`));
  }

  machine.transitions.forEach((t, i) => {
    const n = i + 1;
    if (!states.has(t.from)) defects.push(defect('unknown-state', `Transition ${n} comes from unknown state "${t.from}".`));
    if (!states.has(t.to)) defects.push(defect('unknown-state', `Transition ${n} goes to unknown state "${t.to}".`));
    if (!t.event) defects.push(defect('missing-event', `Transition ${n} (${t.from} -> ${t.to}) has no event.`));
    else if (!events.has(t.event)) defects.push(defect('unknown-event', `Transition ${n} uses unknown event "${t.event}".`));
    if (t.guard !== undefined && !Object.hasOwn(machine.guards, t.guard)) {
      defects.push(defect('unknown-guard', `Transition ${n} uses unknown guard "${t.guard}".`));
    }
    if (terminal.has(t.from) && t.event) {
      defects.push(defect('terminal-exit', `Terminal state "${t.from}" has an outgoing transition on "${t.event}".`));
    }
  });

  const groups = new Map();
  for (const t of machine.transitions) {
    if (!t.event) continue;
    const k = JSON.stringify([t.from, t.event, t.guard ?? null]);
    groups.set(k, (groups.get(k) ?? 0) + 1);
  }
  for (const [k, count] of groups) {
    const [from, event] = JSON.parse(k);
    if (count > 1) defects.push(defect('ambiguous', `State "${from}" has ${count} transitions on "${event}" with the same guard.`));
  }

  const reached = new Set(states.has(machine.initial) ? [machine.initial] : []);
  for (let frontier = [...reached]; frontier.length; ) {
    const next = [];
    for (const t of machine.transitions) {
      if (frontier.includes(t.from) && states.has(t.to) && !reached.has(t.to)) {
        reached.add(t.to);
        next.push(t.to);
      }
    }
    frontier = next;
  }
  for (const s of machine.states) {
    if (reached.size && !reached.has(s)) {
      defects.push(defect('unreachable', `State "${s}" cannot be reached from the initial state "${machine.initial}".`));
    }
    if (!terminal.has(s) && !machine.transitions.some((t) => t.from === s)) {
      defects.push(defect('dead-end', `State "${s}" is not terminal and has no outgoing transition.`));
    }
  }

  return { machine, defects };
}

// One criterion per transition, one per event a state refuses, and one per (state, event) whose
// every transition is guarded (the case where no guard holds).
export function machineCriteria(machine) {
  const e = machine.entity;
  const a = `${/^[aeiou]/i.test(e) ? 'an' : 'a'} ${e}`;
  const guardText = (g) => machine.guards[g] || `guard ${g} holds`;
  const refuse = (state, event) => `then ${event} is refused and the ${e} stays in ${state}.`;

  const allowed = machine.transitions.map((t) => ({
    kind: 'transition',
    from: t.from,
    event: t.event,
    to: t.to,
    ...(t.guard ? { guard: t.guard } : {}),
    text: `Given ${a} in ${t.from}${t.guard ? ` and ${guardText(t.guard)}` : ''}, when ${t.event} is received, then the ${e} is in ${t.to}.`,
  }));

  const guarded = [];
  const refused = [];
  for (const state of machine.states) {
    for (const event of machine.events) {
      const ts = machine.transitions.filter((t) => t.from === state && t.event === event);
      if (!ts.length) {
        refused.push({ kind: 'refused', from: state, event, text: `Given ${a} in ${state}, when ${event} is received, ${refuse(state, event)}` });
      } else if (ts.every((t) => t.guard)) {
        const fails = ts.map((t) => `guard ${t.guard} fails${machine.guards[t.guard] ? ` (${machine.guards[t.guard]})` : ''}`).join(' and ');
        guarded.push({
          kind: 'guard',
          from: state,
          event,
          guards: ts.map((t) => t.guard),
          text: `Given ${a} in ${state} and ${fails}, when ${event} is received, ${refuse(state, event)}`,
        });
      }
    }
  }
  return [...allowed, ...guarded, ...refused];
}

export function machineDiagram(machine) {
  return [
    'stateDiagram-v2',
    `    [*] --> ${machine.initial}`,
    ...machine.transitions.map((t) => `    ${t.from} --> ${t.to}: ${t.event}${t.guard ? ` [${t.guard}]` : ''}`),
    ...machine.terminal.map((s) => `    ${s} --> [*]`),
  ].join('\n');
}
