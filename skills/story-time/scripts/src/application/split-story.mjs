// Use case: split one story into N children as a saga, so a partial failure
// never leaves orphan children or a parent pointing at stories that don't exist.
import { appendChangelog, today } from '../domain/story.mjs';
import { assertStoryRepository, formatId, highestNumber } from '../ports/story-repository.mjs';
import { runSaga } from '../saga/saga.mjs';

const UNSPLITTABLE = new Set(['done', 'split', 'archived']);

export async function splitStory(repo, parentId, childSpecs, { date = today() } = {}) {
  assertStoryRepository(repo);
  if (!Array.isArray(childSpecs) || childSpecs.length < 2) {
    return failure(new Error('A split needs at least two child stories.'));
  }

  const parent = await repo.get(parentId);
  if (!parent) return failure(new Error(`Story ${parentId} not found.`));
  if (UNSPLITTABLE.has(parent.status)) {
    return failure(new Error(`Cannot split ${parentId}: it is ${parent.status}.`));
  }

  const prefix = parentId.split('-')[0];
  const first = highestNumber((await repo.list()).map((s) => s.id), prefix) + 1;
  const children = childSpecs.map((spec, i) => buildChild(parent, spec, formatId(prefix, first + i), date));
  const childIds = children.map((c) => c.id);

  const steps = [
    ...children.map((child) => ({
      name: `create ${child.id}`,
      execute: () => repo.save(child),
      compensate: () => repo.remove(child.id),
    })),
    {
      name: `mark ${parentId} split`,
      execute: () =>
        repo.save(appendChangelog({ ...parent, status: 'split', children: childIds }, `split into ${childIds.join(', ')}`, date)),
      compensate: () => repo.save(parent),
    },
  ];

  const result = await runSaga(steps);
  return result.ok ? { ok: true, parentId, childIds } : { ...result, ok: false, parentId };
}

function buildChild(parent, spec, id, date) {
  const acceptance = (spec.acceptance ?? []).map((a) => `- [ ] ${a}`).join('\n');
  const sections = { Story: spec.statement ?? '' };
  if (acceptance) sections['Acceptance Criteria'] = acceptance;
  if (spec.notes) sections.Notes = spec.notes;
  sections.Changelog = `- ${date} created by splitting ${parent.id}`;
  return {
    id,
    title: spec.title,
    status: 'draft',
    type: spec.type ?? parent.type ?? 'story',
    epic: parent.epic,
    persona: spec.persona ?? parent.persona,
    value: spec.value,
    time_criticality: spec.time_criticality,
    risk_reduction: spec.risk_reduction,
    size: spec.size,
    depends_on: spec.depends_on ?? [],
    parent: parent.id,
    created: date,
    updated: date,
    sections,
  };
}

function failure(error) {
  return { ok: false, error, compensated: [], compensationFailures: [] };
}
