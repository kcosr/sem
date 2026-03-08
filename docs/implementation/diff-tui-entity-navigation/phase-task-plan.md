# Phase Task Plan: Diff TUI Entity Navigation

## Status
Locked

## 1. Scope
Deliver Rust `sem diff` TUI navigation with entity diff inspection and side-by-side support, including model/JSON extensions for entity line ranges and full verification evidence.

## 2. Global Rules
1. Follow phase order strictly; do not start next phase before prior go/no-go is `GO`.
2. Preserve existing non-TUI CLI behavior.
3. Treat `--tui` as opt-in; no default behavior changes.
4. Keep model additions backward-compatible (optional fields only).
5. Apply mandatory independent review policy when updating design/plan artifacts.
6. Every review finding must be triaged as `accept`, `defer`, or `reject`.
7. Capture Section 9 evidence before phase closure.
8. Do not add TypeScript CLI work in this stream.

## 3. Phases, Deliverables, Acceptance Criteria

### H0: Contracts and Data Model Lock
Deliverables:
1. Finalized schema contract for CLI flags and JSON fields.
2. `SemanticChange` extended with optional line fields in `sem-core`.
3. Unit tests covering line field population by change type.
4. Decision lock for defaults: `--diff-view` default = `unified`; `--tui` incompatible with any `--format`.

Acceptance Criteria:
1. New fields compile and serialize cleanly.
2. Existing consumers continue to work without requiring new fields.
3. Tests validate modified/added/deleted/moved/renamed range behavior, including cross-file moved cases.

Gate: `GO` only if contract is deterministic and tests pass.

### H1: Diff Command Refactor + TUI Skeleton
Deliverables:
1. Diff command split into data acquisition and output execution paths.
2. `--tui` and `--diff-view` argument support in CLI.
3. TUI app skeleton with list view and key loop.
4. Explicit deterministic handling for `--tui` with `--stdin` and two-file mode.

Acceptance Criteria:
1. `sem diff` static output remains unchanged by default.
2. `sem diff --tui` opens interactive list when changes are present.
3. Invalid arg combos fail deterministically.

Gate: `GO` only if command contracts are stable and backward-compatible.

### H2: Entity Diff Detail View + Side-by-Side
Deliverables:
1. Enter/Esc detail mode.
2. Unified and side-by-side renderers.
3. Hunk navigation (`n/p`) and help overlay (`?`).
4. Page scrolling (`PageUp/PageDown`) and top/bottom jumps (`g/G`).
5. Range labels in list/detail headers.
6. Width-aware side-by-side fallback and reversible toggle behavior.

Acceptance Criteria:
1. Multi-hunk diffs render for modified entities.
2. Added/deleted entities display one-sided diff correctly.
3. Side-by-side fallback behavior is deterministic and test-covered.
4. Mode-scoped key handling prevents accidental global actions.

Gate: `GO` only if interaction model is usable and deterministic.

### H3: Hardening, Docs, and Release Readiness
Deliverables:
1. Automated tests (state machine + formatter + argument validation + render tests).
2. Manual verification evidence across representative repositories.
3. Dependency audit notes for `ratatui`, `crossterm`, `similar`.
4. Docs updates (`README.md`, `crates/README.md`, optional architecture reference if stabilized).
5. Changelog entry and implementation-plan milestone update.

Acceptance Criteria:
1. CI-relevant tests pass.
2. User-facing docs include key bindings and feature scope.
3. Evidence log complete for all phases.

Gate: `GO` only if acceptance evidence is complete and review findings are resolved/triaged.

## 4. Verification Matrix
| Area | Verification Type | Command / Evidence |
|---|---|---|
| sem-core model fields | unit tests | `cargo test -p sem-core` |
| sem-cli arg parsing | unit/integration tests | `cargo test -p sem-cli` |
| TUI state transitions | unit tests | targeted app-state tests |
| hunk navigation boundaries | unit tests | `n/p` boundary cases |
| Diff rendering modes | unit/render tests | test backend snapshots |
| width fallback behavior | unit/render tests | constrained-width render checks |
| No-regression static diff | manual + tests | `sem diff` vs baseline behavior |
| JSON compatibility | unit/manual | `sem diff --format json` output checks |
| no-change TUI path | integration/manual | `sem diff --tui` with no changes |
| non-UTF8/binary resilience | integration/manual | crash-free behavior evidence |
| large diff responsiveness | manual perf smoke | evidence on representative large change set |

## 5. Milestone Commit Gate
1. One milestone commit required at end of each phase (`H0`..`H3`).
2. Commit message format:
   - `feat(sem): H0 contract + line-range model`
   - `feat(sem): H1 diff command refactor + tui skeleton`
   - `feat(sem): H2 entity detail + side-by-side diff`
   - `feat(sem): H3 hardening + docs`
3. No phase may be closed without tests/evidence attached in Section 9.

## 6. Risks and Mitigations
1. Risk: terminal rendering complexity across environments.
   - Mitigation: isolate state/reducer from renderer; add width fallback behavior tests.
2. Risk: argument-contract regressions.
   - Mitigation: explicit parser tests for invalid combinations.
3. Risk: line range mismatch in renamed/moved cases.
   - Mitigation: per-change-type unit fixtures in `sem-core`, including cross-file cases.
4. Risk: duplication when populating new line fields in multiple `SemanticChange` construction paths.
   - Mitigation: helper constructor or exhaustive tests over all emission paths.
5. Risk: large diffs may degrade TUI responsiveness.
   - Mitigation: performance smoke evidence and deferred optimization follow-up if threshold exceeded.

## 7. Deferred Items (Explicit)
1. Intra-line color diffing inside a changed line is deferred.
2. Mouse support deferred.
3. Persistent per-user TUI preferences deferred.
4. Full lazy-loading/pagination optimization for extreme monorepo-scale diffs is deferred unless H3 evidence shows unacceptable performance.

## 8. Review Findings Triage Ledger (Authoring Stage)

### Review Run A (generic-gemini)
- Run ID: `r_20260308061458603_6d327e9a`
- Findings (summary):
  - naming consistency note (snake_case vs camelCase)
  - list hierarchy ambiguity
  - missing context-lines/diff styling requirement
  - dynamic resize and key-scoping risks
  - missing tests for resize, navigation boundaries, stress, non-UTF8
- Triage outcomes:
  - `accept`: naming convention lock, flattened list decision, context-lines=3, dynamic-resize behavior, key-scoped handling, boundary tests, resize tests, non-UTF8 tests
  - `defer`: strict performance scaling/pagination optimization (tracked in Section 7)
  - `reject`: none

### Review Run B (generic-pi)
- Run ID: `r_20260308061535268_44dba12f`
- Findings (summary):
  - unspecified `--diff-view` default
  - JSON schema omitted existing optional fields (`id`, `timestamp`, `structuralChange`)
  - unspecified `--tui` interaction with `--format`, `--stdin`, and two-file mode
  - missing scroll/sort requirements and hunk verification rows
  - risks around duplicated change construction and dependency impact
- Triage outcomes:
  - `accept`: default lock (`unified`), schema field completeness, `--tui` contract clarifications, scroll/sort requirements, verification matrix expansion, dependency audit, duplication risk handling
  - `defer`: broad performance optimization policy (tracked in Section 7)
  - `reject`: none

## 9. Operator Checklist and Evidence Log Schema (Mandatory)

### 9.1 Operator Checklist Per Phase
1. Confirm prior phase go/no-go is `GO`.
2. Implement only deliverables listed for current phase.
3. Run required verification commands.
4. Record review runs and triage outcomes.
5. Record evidence using schema below.
6. Decide go/no-go and document rationale.

### 9.2 Evidence Log Schema (Required Entry Shape)
For each phase `H0..H3`, record:
1. Completion date (`YYYY-MM-DD`).
2. Commit hash(es).
3. Acceptance evidence (commands, test outputs, manual checks).
4. Review run IDs + triage outcomes.
5. Go/no-go decision.

Example template:

```md
### Hx Evidence
- Completion date: YYYY-MM-DD
- Commit hash(es): <hash list>
- Acceptance evidence:
  - `<command>` => `<result summary>`
  - manual: `<what was validated>`
- Review run IDs + triage outcomes:
  - `<run-id>`: `accept|defer|reject` summary
- Go/No-Go: GO | NO-GO
- Notes: <optional>
```

### 9.3 Authoring-Stage Evidence (Spec Plan)
- Completion date: 2026-03-08
- Commit hash(es): N/A (planning artifacts only in this stream)
- Acceptance evidence:
  - Drafted required artifacts under `docs/implementation/diff-tui-entity-navigation/`
  - Ran 2 independent reviews and triaged all findings
- Review run IDs + triage outcomes:
  - `r_20260308061458603_6d327e9a`: accepted + deferred items captured
  - `r_20260308061535268_44dba12f`: accepted + deferred items captured
- Go/No-Go: GO
- Notes:
  - Initial review command with forbidden CLI override was retried using compliant invocation and then completed successfully.

### H0 Evidence
- Completion date: 2026-03-08
- Commit hash(es): `8549bd3`
- Acceptance evidence:
  - `npm run lint` => `FAIL` (pre-existing TypeScript workspace/type dependency errors unrelated to Rust `sem-core`; unchanged by H0 scope)
  - `npm test` => `FAIL` (`vitest` not installed in current environment; unchanged by H0 scope)
  - `cargo test -p sem-core` => `PASS` (35 passed, 0 failed), including new line-range coverage:
    - `model::change::tests::test_semantic_change_serializes_range_fields_as_camel_case`
    - `model::change::tests::test_semantic_change_omits_none_optional_fields`
    - `model::identity::tests::test_similarity_moved_cross_file_line_ranges`
  - manual: verified all `SemanticChange` construction paths in `match_entities` now route through `build_change` and populate optional before/after line fields.
- Review run IDs + triage outcomes:
  - `r_20260308062646574_383aadf6` (generic-gemini):
    - `accept`: add serialization assertions for new line-range fields, add fuzzy/similarity-path line-range test.
    - `defer`: none.
    - `reject`: speculative future timestamp propagation concern (no current timestamp producer in this phase scope).
  - `r_20260308062821707_6f93d8f6` (generic-pi):
    - `accept`: add explicit serialization coverage and similarity-path line-range coverage.
    - `defer`: sem-cli JSON formatter parity updates (out of H0 deliverable scope; scheduled for later execution phase).
    - `reject`: schema-minimum risk on zero-based parser lines (no evidence of zero-based emission in current sem-core plugins/tests).
- Go/No-Go: GO
- Notes:
  - Phase gate outcome is GO for H0 Rust scope (`sem-core`) with mandatory Node gate failures documented as pre-existing baseline/environmental constraints.

## 10. Execution Start Point
Execution stream must start at `H0` only.

## 11. Execution Handoff Contract
1. Required read order:
   - `docs/implementation/diff-tui-entity-navigation/schema-proposal.md`
   - `docs/implementation/diff-tui-entity-navigation/design.md`
   - `docs/implementation/diff-tui-entity-navigation/phase-task-plan.md`
2. Start point:
   - start at `H0` only.
3. Boundaries and semantic-preservation constraints:
   - Rust-only (`crates/sem-core`, `crates/sem-cli`), no TypeScript CLI work.
   - Preserve default non-TUI behavior and backward-compatible JSON semantics.
4. Review command policy requirements:
   - use `agent-runner-review` mechanics.
   - no timeout/reasoning CLI overrides.
   - completion by session stream terminal event (`result.completed|result.failed`).
5. Completion requirements:
   - update docs/reference architecture only if behavior is stabilized.
   - update `docs/implementation/implementation-plan.md` milestone status.
   - add `CHANGELOG.md` milestone entry.
   - complete Section 9 evidence per phase.
   - publish final phase summary and go/no-go state.
