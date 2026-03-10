# Handoff Prompt

Use `$agent-runner-spec-execution` and `$agent-runner-review`.

Topic slug: `diff-tui-view-cycle-compact-split`.

Read:
1. `docs/implementation/diff-tui-view-cycle-compact-split/schema-proposal.md`
2. `docs/implementation/diff-tui-view-cycle-compact-split/design.md`
3. `docs/implementation/diff-tui-view-cycle-compact-split/phase-task-plan.md`

Execute all phases declared in `phase-task-plan.md` in strict order, using the plan as source of truth for scope, gates, review/triage policy, and Section 9 evidence updates.

Constraints:
1. Preserve default startup mode (`List`).
2. Preserve existing list/detail behavior outside additive split feature.
3. No semantic diff/model/output contract changes.
4. No new CLI flags.

Review policy:
1. Use `agent-runner-review` mechanics.
2. No timeout or reasoning-effort CLI overrides.
3. Completion must be confirmed from stream events (`result.completed|result.failed`).

Completion requirements:
1. Update docs when behavior stabilizes.
2. Add `CHANGELOG.md` milestone entry.
3. Complete Section 9 evidence for each phase.
4. Publish final phase summary.
