// Export a story as a copy-paste card for a tracker, or as the CLI command
// that creates it there. Pure: story + config in, strings out.
// Strategy registry: add a format by adding an exporter, not by editing renderCard.
import { acceptanceCriteria } from './story.mjs';
import { wsjf } from './wsjf.mjs';

const INTERNAL_SECTIONS = new Set(['Story', 'Question', 'Exit Criteria', 'Acceptance Criteria', 'Changelog', 'Test Plan']);

// --- shared helpers ---

const section = (s, name) => s.sections?.[name]?.trim() || '';
const isNum = (v) => typeof v === 'number' && Number.isFinite(v);

function criteriaRaw(s) {
  return section(s, s.type === 'spike' ? 'Exit Criteria' : 'Acceptance Criteria');
}

function plainCriteria(s) {
  return acceptanceCriteria(s).map((ac) => `- ${ac.text}`).join('\n');
}

function extraSections(s) {
  return Object.keys(s.sections ?? {}).filter((name) => !INTERNAL_SECTIONS.has(name) && section(s, name));
}

function footer(s) {
  const w = wsjf(s);
  return `<sub>story-time ${s.id}${w ? ` · WSJF ${w.score.toFixed(2)}` : ''}</sub>`;
}

function lead(s) {
  if (s.type === 'spike') {
    const parts = [`### Question\n${section(s, 'Question')}`];
    if (s.timebox) parts.push(`Timebox: ${s.timebox}`);
    return parts.join('\n\n');
  }
  return section(s, 'Story');
}

function markdownBody(s) {
  const parts = [lead(s)];
  const ac = criteriaRaw(s);
  if (ac) parts.push(`### ${s.type === 'spike' ? 'Exit Criteria' : 'Acceptance Criteria'}\n${ac}`);
  for (const name of extraSections(s)) parts.push(`### ${name}\n${section(s, name)}`);
  return parts.filter(Boolean).join('\n\n');
}

// --- exporters ---

const github = {
  render(s) {
    const labels = [s.type ?? 'story'];
    if (isNum(s.size)) labels.push(`size:${s.size}`);
    if (isNum(s.value)) labels.push(`value:${s.value}`);
    if (s.epic) labels.push(`epic:${s.epic}`);
    const body = `${markdownBody(s)}\n\n${footer(s)}`;
    const text = `**Title:** ${s.title}\n**Labels:** ${labels.map((l) => `\`${l}\``).join(' ')}\n\n${body}`;
    return { format: 'github', title: s.title, labels, body, text };
  },
};

const markdown = {
  render(s) {
    const meta = [
      isNum(s.size) && `**Size:** ${s.size}`,
      isNum(s.value) && `**Value:** ${s.value}`,
      s.epic && `**Epic:** ${s.epic}`,
      s.status && `**Status:** ${s.status}`,
    ].filter(Boolean);
    const body = [markdownBody(s), meta.join(' · '), footer(s)].filter(Boolean).join('\n\n');
    return { format: 'markdown', title: s.title, body, text: `## ${s.title}\n\n${body}` };
  },
};

const ADO_TYPES = {
  agile: { story: 'User Story', bug: 'Bug', points: 'Story Points', pointsRef: 'Microsoft.VSTS.Scheduling.StoryPoints', criteriaField: true },
  scrum: { story: 'Product Backlog Item', bug: 'Bug', points: 'Effort', pointsRef: 'Microsoft.VSTS.Scheduling.Effort', criteriaField: true },
  basic: { story: 'Issue', bug: 'Issue', points: 'Effort', pointsRef: 'Microsoft.VSTS.Scheduling.Effort', criteriaField: false },
};

function adoFields(s, config) {
  const ado = config?.ado ?? { process: 'agile' };
  const proc = ADO_TYPES[ado.process ?? 'agile'];
  const isBug = s.type === 'bug';
  const useRepro = isBug && proc.bug === 'Bug';
  const criteria = plainCriteria(s);

  let description = lead(s).replace(/^### Question\n/, 'Question: ');
  if (!proc.criteriaField && criteria) description += `\n\nAcceptance Criteria:\n${criteria}`;
  const outOfScope = section(s, 'Out of Scope');
  if (outOfScope) description += `\n\nOut of Scope:\n${outOfScope}`;

  const tags = ['story-time', s.id];
  if (s.type && s.type !== 'story' && !isBug) tags.push(s.type);
  if (s.epic) tags.push(`epic:${s.epic}`);

  const fields = [
    { name: 'Work Item Type', value: isBug ? proc.bug : proc.story },
    { name: 'Title', value: s.title },
  ];
  if (useRepro) {
    const repro = [description, ['Steps to Reproduce', 'Expected', 'Actual'].map((n) => section(s, n) && `${n}:\n${section(s, n)}`).filter(Boolean).join('\n\n')]
      .filter(Boolean)
      .join('\n\n');
    fields.push({ name: 'Repro Steps', value: repro, ref: 'Microsoft.VSTS.TCM.ReproSteps' });
  } else {
    fields.push({ name: 'Description', value: description, ref: 'System.Description' });
  }
  if (proc.criteriaField && criteria) fields.push({ name: 'Acceptance Criteria', value: criteria, ref: 'Microsoft.VSTS.Common.AcceptanceCriteria' });
  if (isNum(s.size)) fields.push({ name: proc.points, value: String(s.size), ref: proc.pointsRef });
  fields.push({ name: 'Tags', value: tags.join('; '), ref: 'System.Tags' });
  if (ado.areaPath) fields.push({ name: 'Area Path', value: ado.areaPath, ref: 'System.AreaPath' });
  if (ado.iterationPath) fields.push({ name: 'Iteration Path', value: ado.iterationPath, ref: 'System.IterationPath' });
  return fields;
}

const ado = {
  render(s, { config } = {}) {
    const fields = adoFields(s, config);
    // One fenced block per field: each gets its own copy button.
    const text = fields.map((f) => `${f.name}\n\`\`\`\n${f.value}\n\`\`\``).join('\n\n');
    return { format: 'ado', title: s.title, fields, text };
  },
};

export const EXPORTERS = { github, ado, markdown };

export function renderCard(story, format, { config, exporters = EXPORTERS } = {}) {
  const exporter = exporters[format];
  if (!exporter) throw new Error(`Unknown export format "${format}". Use one of: ${Object.keys(exporters).join(', ')}.`);
  return exporter.render(story, { config });
}

// --- create commands ---

const q = (v) => `'${String(v).replace(/'/g, `'\\''`)}'`;
// Quote only when the shell needs it, so simple args stay readable.
const arg = (v) => (/^[\w.:/@-]+$/.test(String(v)) ? String(v) : q(v));
const html = (v) => String(v).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

function toHtml(text) {
  const lines = text.split('\n');
  const items = lines.filter((l) => /^- /.test(l));
  if (items.length && items.length === lines.filter(Boolean).length) {
    return `<ul>${items.map((l) => `<li>${html(l.slice(2))}</li>`).join('')}</ul>`;
  }
  return text
    .split(/\n{2,}/)
    .map((p) => `<p>${html(p).replace(/\n/g, '<br>')}</p>`)
    .join('');
}

const COMMANDS = {
  github(s, config) {
    if (!config?.github?.repo) throw new Error('Set github.repo in .story-time.json (run the story-setup skill) to print a gh command.');
    const card = github.render(s);
    const labels = card.labels.map((l) => `--label ${arg(l)}`).join(' ');
    return `gh issue create --repo ${arg(config.github.repo)} --title ${q(s.title)} ${labels} --body-file - <<'STORY_TIME_EOF'\n${card.body}\nSTORY_TIME_EOF`;
  },
  ado(s, config) {
    if (!config?.ado?.organization) throw new Error('Set ado.organization and ado.project in .story-time.json (run the story-setup skill) to print an az command.');
    const fields = adoFields(s, config);
    const get = (name) => fields.find((f) => f.name === name);
    const head = `az boards work-item create --org ${q(config.ado.organization)} --project ${q(config.ado.project)} --type ${q(get('Work Item Type').value)} --title ${q(s.title)}`;
    const lines = [head];
    const desc = get('Description');
    if (desc) lines.push(`--description ${q(toHtml(desc.value))}`);
    if (get('Area Path')) lines.push(`--area ${q(get('Area Path').value)}`);
    if (get('Iteration Path')) lines.push(`--iteration ${q(get('Iteration Path').value)}`);
    const extra = fields.filter((f) => f.ref && !['System.Description', 'System.AreaPath', 'System.IterationPath'].includes(f.ref));
    if (extra.length) {
      const value = (f) => (/AcceptanceCriteria|ReproSteps/.test(f.ref) ? toHtml(f.value) : f.value);
      lines.push(`--fields ${extra.map((f) => q(`${f.ref}=${value(f)}`)).join(' ')}`);
    }
    return lines.join(' \\\n  ');
  },
};

export function renderCommand(story, format, { config } = {}) {
  const build = COMMANDS[format];
  if (!build) throw new Error(`Format "${format}" has no create command. Use github or ado.`);
  return build(story, config);
}
