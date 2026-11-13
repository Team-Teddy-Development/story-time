---
name: story-map
description: Break an epic, feature, or product idea into a user story map — backbone of user activities, the steps under each, stories under each step — and cut it into release slices starting with a walking skeleton, then create the stories. Use when the user brings an epic, PRD, or feature that is clearly many stories, says "story map", "break down this epic", or "plan the MVP".
---

# Story Map

`story` means the CLI from the **story-time** skill.

This skill owns epic outcomes, definitions, and story links. The **refinement**
skill inventories existing stories and brings evidence or unresolved epic questions
here. Return a defined outcome and its source path, a justified standalone reason,
or an explicit unresolved question. Sprint plans consume those decisions; they do
not create a parallel epic service.

A backlog is a flat list that hides the shape of the product. A story map
shows the shape: what users do from left to right, and how much of each
step we build from top to bottom.

## 1. Frame

Agree on these with the user before mapping. Ask if they aren't given:

- **Personas.** One to three.
- **Outcome.** What changes for the business when this ships. One sentence.
- **Boundary.** Where the journey starts and ends.

## 2. Backbone

List the user's **activities** left to right, in the order they happen
(e.g. *Discover → Choose → Pay → Track*). Under each, list the **steps**
(e.g. under *Pay*: enter address, choose shipping, pay, confirm). Use
verbs, in the user's language, with no system internals.

## 3. Stories under each step

Under each step, stack the stories from must-have at the top to
nice-to-have at the bottom. Each story is a vertical slice that passes the
**principle-invest** skill.

## 4. Slice releases

Draw horizontal lines across the map:

- **Walking skeleton (release 1).** The thinnest story under *every*
  activity, so a user can complete the whole journey end to end, however
  crudely. If an activity has nothing above the line, the skeleton is broken.
- **Next slices.** Each slice should make a measurable outcome better. Name
  each slice for the outcome, not the version number.

## 5. Show the map

Render it as a markdown table. Columns are activities, the first row is
steps, and the remaining rows are slices:

| | Discover | Choose | Pay | Track |
|---|---|---|---|---|
| **Steps** | search, browse | compare, add to cart | address, pay | status |
| **Skeleton** | keyword search | add one item | card only, US only | order emailed |
| **Slice 2: fewer abandoned carts** | filters | multi-item cart | saved addresses | status page |

Wait for the user to approve it or change it.

## 6. Create

Pick an epic id (e.g. `EP-003`; check `story list` for ones in use). Create
each story in the skeleton and the next slice with the **story-create**
skill, passing `--epic EP-003`. Record its slice under `## Notes`. Leave
later slices on the map, not in the backlog: they will change.

Save the map as `backlog/EP-003.map.md` so later grooming can see it. The
CLI ignores files that aren't story files.

## 7. Report

The map, the ids created per slice, and the suggested next step (usually the
**story-estimate** skill on the skeleton batch).
