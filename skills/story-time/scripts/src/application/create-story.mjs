import { TYPES, appendChangelog, today } from '../domain/story.mjs';
import { assertStoryRepository } from '../ports/story-repository.mjs';

export const STATEMENT_TODO = 'TODO: As a <persona>, I want <goal>, so that <benefit>.';

export async function createStory(repo, input, { prefix = 'ST', date = today() } = {}) {
  assertStoryRepository(repo);
  if (!input?.title?.trim()) throw new Error('A story needs a title.');
  const type = input.type ?? 'story';
  if (!TYPES.includes(type)) throw new Error(`Unknown type "${type}". Use one of: ${TYPES.join(', ')}.`);

  const sections = {};
  if (type === 'spike') {
    sections.Question = input.question ?? 'TODO: the one question this spike answers.';
    sections['Exit Criteria'] = bullets(input.acceptance) || '- [ ] TODO: decision recorded with evidence';
  } else {
    sections.Story = input.statement ?? STATEMENT_TODO;
    const ac = bullets(input.acceptance);
    if (ac) sections['Acceptance Criteria'] = ac;
  }
  if (input.notes) sections.Notes = input.notes;

  const story = appendChangelog(
    {
      id: await repo.nextId(prefix),
      title: input.title.trim(),
      status: 'draft',
      type,
      epic: input.epic,
      persona: input.persona,
      value: input.value,
      time_criticality: input.time_criticality,
      risk_reduction: input.risk_reduction,
      size: input.size,
      timebox: input.timebox,
      depends_on: input.depends_on ?? [],
      created: date,
      updated: date,
      sections,
    },
    'created',
    date,
  );
  const clean = Object.fromEntries(Object.entries(story).filter(([, v]) => v !== undefined));
  await repo.save(clean);
  return clean;
}

function bullets(items = []) {
  return items.map((a) => `- [ ] ${a}`).join('\n');
}
