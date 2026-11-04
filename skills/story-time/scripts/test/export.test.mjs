import { test } from 'node:test';
import assert from 'node:assert/strict';
import { renderCard, renderCommand, EXPORTERS } from '../src/domain/export.mjs';
import { parseConfig } from '../src/config/config.mjs';
import { parseStory } from '../src/domain/story.mjs';
import { READY_MD, story } from './fixtures.mjs';

const ready = () => parseStory(READY_MD);
const adoConfig = (process = 'agile', extra = {}) =>
  parseConfig({ tracker: 'ado', ado: { organization: 'https://dev.azure.com/acme', project: 'Shop', process, ...extra } });
const field = (card, name) => card.fields.find((f) => f.name === name)?.value;

test('github card: title, labels, statement, checklist criteria, scope, and a traceable footer', () => {
  const card = renderCard(ready(), 'github');
  assert.equal(card.title, 'Reset password via email');
  assert.deepEqual(card.labels, ['story', 'size:3', 'value:8', 'epic:EP-1']);
  assert.match(card.body, /^As a registered user, I want/);
  assert.match(card.body, /### Acceptance Criteria\n- \[ \] Given a registered email/);
  assert.match(card.body, /### Out of Scope\n- SMS reset/);
  assert.match(card.body, /<sub>story-time ST-001 · WSJF 5\.33<\/sub>$/);
  assert.doesNotMatch(card.body, /Changelog|---\nid:/, 'internal history and frontmatter stay out');
});

test('github card text is one paste-ready block', () => {
  const { text } = renderCard(ready(), 'github');
  assert.match(text, /^\*\*Title:\*\* Reset password via email\n\*\*Labels:\*\* `story` `size:3` `value:8` `epic:EP-1`\n\nAs a registered user/);
});

test('cards omit sections and labels that have no content', () => {
  const card = renderCard(story({ size: undefined, value: undefined, sections: { Story: 'As a shopper, I want x, so that y.' } }), 'github');
  assert.deepEqual(card.labels, ['story']);
  assert.doesNotMatch(card.body, /Acceptance Criteria|Out of Scope|WSJF/);
});

test('spike cards carry the question and exit criteria', () => {
  const spike = story({ type: 'spike', timebox: '1d', sections: { Question: 'Can X do 3DS?', 'Exit Criteria': '- [ ] Decision recorded' } });
  const card = renderCard(spike, 'github');
  assert.ok(card.labels.includes('spike'));
  assert.match(card.body, /### Question\nCan X do 3DS\?/);
  assert.match(card.body, /### Exit Criteria\n- \[ \] Decision recorded/);
  assert.match(card.body, /Timebox: 1d/);
});

test('ado agile card maps to User Story fields with semicolon tags', () => {
  const card = renderCard(ready(), 'ado', { config: adoConfig() });
  assert.equal(field(card, 'Work Item Type'), 'User Story');
  assert.equal(field(card, 'Title'), 'Reset password via email');
  assert.match(field(card, 'Description'), /^As a registered user/);
  assert.match(field(card, 'Acceptance Criteria'), /^- Given a registered email/);
  assert.equal(field(card, 'Story Points'), '3');
  assert.equal(field(card, 'Tags'), 'story-time; ST-001; epic:EP-1');
  assert.match(card.text, /^Work Item Type\n```\nUser Story\n```/);
});

test('ado scrum uses Product Backlog Item and Effort', () => {
  const card = renderCard(ready(), 'ado', { config: adoConfig('scrum') });
  assert.equal(field(card, 'Work Item Type'), 'Product Backlog Item');
  assert.equal(field(card, 'Effort'), '3');
  assert.equal(field(card, 'Story Points'), undefined);
});

test('ado basic has no acceptance criteria field, so criteria fold into the description', () => {
  const card = renderCard(ready(), 'ado', { config: adoConfig('basic') });
  assert.equal(field(card, 'Work Item Type'), 'Issue');
  assert.equal(field(card, 'Acceptance Criteria'), undefined);
  assert.match(field(card, 'Description'), /Acceptance Criteria:\n- Given a registered email/);
});

test('ado bugs use the Bug type and Repro Steps', () => {
  const bug = story({ type: 'bug', sections: { ...story().sections, 'Steps to Reproduce': '1. Open cart\n2. Refresh' } });
  const card = renderCard(bug, 'ado', { config: adoConfig() });
  assert.equal(field(card, 'Work Item Type'), 'Bug');
  assert.match(field(card, 'Repro Steps'), /1\. Open cart/);
});

test('ado card includes area and iteration paths when configured', () => {
  const card = renderCard(ready(), 'ado', { config: adoConfig('agile', { areaPath: 'Shop\\Web', iterationPath: 'Shop\\Sprint 12' }) });
  assert.equal(field(card, 'Area Path'), 'Shop\\Web');
  assert.equal(field(card, 'Iteration Path'), 'Shop\\Sprint 12');
});

test('markdown card is tracker-neutral', () => {
  const card = renderCard(ready(), 'markdown');
  assert.match(card.text, /^## Reset password via email\n\nAs a registered user/);
  assert.match(card.text, /\*\*Size:\*\* 3 · \*\*Value:\*\* 8/);
});

test('unknown formats are rejected; new exporters plug in without editing renderCard', () => {
  assert.throws(() => renderCard(ready(), 'trello'), /Unknown export format "trello"/);
  const exporters = { ...EXPORTERS, plain: { render: (s) => ({ title: s.title, text: `PLAIN ${s.id}` }) } };
  assert.equal(renderCard(ready(), 'plain', { exporters }).text, 'PLAIN ST-001');
});

test('github command creates the issue with gh, body on a quoted heredoc', () => {
  const cfg = parseConfig({ tracker: 'github', github: { repo: 'acme/shop' } });
  const cmd = renderCommand(story({ title: "Don't lose the cart" }), 'github', { config: cfg });
  assert.match(cmd, /^gh issue create --repo acme\/shop --title 'Don'\\''t lose the cart' --label story/);
  assert.match(cmd, /--body-file - <<'STORY_TIME_EOF'\n/);
  assert.match(cmd, /\nSTORY_TIME_EOF$/);
});

test('ado command creates the work item with az boards, mapping fields by reference name', () => {
  const cmd = renderCommand(ready(), 'ado', { config: adoConfig() });
  assert.match(cmd, /^az boards work-item create --org 'https:\/\/dev\.azure\.com\/acme' --project 'Shop' --type 'User Story' --title 'Reset password via email'/);
  assert.match(cmd, /'Microsoft\.VSTS\.Common\.AcceptanceCriteria=<ul><li>Given a registered email/);
  assert.match(cmd, /'Microsoft\.VSTS\.Scheduling\.StoryPoints=3'/);
  assert.match(cmd, /'System\.Tags=story-time; ST-001; epic:EP-1'/);
});

test('commands need the tracker coordinates from config', () => {
  assert.throws(() => renderCommand(ready(), 'github', { config: parseConfig({}) }), /github\.repo/);
  assert.throws(() => renderCommand(ready(), 'ado', { config: parseConfig({}) }), /ado\.organization/);
  assert.throws(() => renderCommand(ready(), 'markdown', { config: parseConfig({}) }), /no create command/);
});
