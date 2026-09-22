---
name: milestone-review
description: Review a finished roadmap milestone across Native, Atlas and pdxscript-rs, then write the cut-and-repair report.
disable-model-invocation: true
argument-hint: "<milestone number>"
---

# Milestone review

A milestone that meets its exit gate proves one slice. The review asks if the method **transfers**:
what work gives the next ten results from unfamiliar cases? Hold each finding and each
recommendation against that question.

Repositories: `~/Developer/pdx-native` (holds the specification, design and roadmap),
`~/Developer/pdx-atlas`, `~/Developer/pdxscript-rs`. The review is read-only until step 5. Run
default builds, tests and lints only; the ignored live tests start a game.

The **product core**: Native is a deep module that answers questions about Stellaris with no
platform or game-build knowledge in the consumer. Atlas generates the knowledge that the cwtools
config writes by hand. The user's own statement of the core in the prompt replaces this one.

## Steps

1. **Read the authorities.** `docs/specs/native.md`, `docs/design/architecture.md`,
   `docs/roadmap.md`, and each earlier `docs/design/milestone-*-review.md`. In the Linear project
   "Atlas": the milestone, its exit gate, and each of its tickets with status.
   Done when you can state the exit gate, which tickets are open, and which items of the last
   review's order of work are still not done.

2. **Dispatch one review agent for each repository, in parallel.** Give each agent the brief
   below. While they run, read the tickets that are open or were closed last.
   Done when each agent has reported.

3. **Verify in the source.** Read the code behind each finding that will lead the answer, and
   behind each cut that the report will recommend. An agent report is a claim, not a fact. Mark
   what stays unchecked as unchecked.
   Done when each lead finding has a `path:line` that you read yourself.

4. **Answer the three questions** to the user: on track, focus, cracks. Lead with the verdict.
   Say what is solid. Stop here; the user decides if a report follows.

5. **Write the report** to `docs/design/milestone-<n>-review.md` in Native, with the structure
   in [REPORT.md](REPORT.md). Leave it uncommitted.
   Done when each cut and each repair has a reason, a condition, and a place in the order of work.

6. **Second opinion.** When the user brings another reviewer's response: check each factual
   correction in the source and fix the report at once where the correction is right. For each
   point of judgement, concede it or hold it with a reason. List the held points for the user.
   When the held points are settled, rewrite the report as one agreed document with status
   "agreed recommendation". Quote agreed decision text exactly.

## Agent brief

Each agent gets: the repository path; read-only; the product core; the authorities to read first;
and these three questions.

1. **On track.** Does the source agree with the specification and design? Each divergence, each
   stale document, each Testing Decision with no test. For Atlas: what real output exists, how far
   it is from one hand-written config file, the dependency pins against their main branches.
2. **Focus.** Lines for each module, and source against tests, tools and documents. For each large
   part: does it serve the core path now, is it speculative, or is it a leftover of a cut design?
   Code that no supported operation can reach. Work that belongs to a later milestone.
3. **Cracks.** Look hardest for **handwritten knowledge behind a general name**: constant tables
   of tokens, offsets or names in Native; hard-coded registry names, field lists and special cases
   in Atlas. Then: how many cases each "shared" method was measured on; a choice repeated in many
   places that the design says is made once; the same knowledge written in two languages or two
   layers; strings and `Debug` output used as identity; functions over 100 lines; how many files
   the last new operation touched; `git log --stat` churn; results of default tests and lints.

Report format: findings sorted by importance; each with a one-line claim, evidence as `path:line`
and numbers, and a confidence; verified facts separate from impressions; what is solid; under
1,200 words.

## Judgement rules

- A line count is a reason to inspect, not a verdict. A combination justifies action: a large
  feature with narrow use, code with no supported caller, case-specific knowledge on both sides of
  the boundary.
- Low early coverage is not a measure of value. Shared infrastructure gives few results at first;
  its value is a cheaper next case.
- One percentage can mislead. A sweep records reach, fields found, unresolved paths, known and
  unknown results, and cost. Record a frozen result before any fix; a case that informs a fix is
  a regression case from then on.
