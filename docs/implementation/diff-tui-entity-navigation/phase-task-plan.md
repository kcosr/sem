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

### H1 Evidence
- Completion date: 2026-03-08
- Commit hash(es): `f412e48`
- Acceptance evidence:
  - `npm run lint` => `FAIL` (pre-existing TypeScript workspace/type dependency errors unrelated to Rust H1 scope)
  - `npm test` => `FAIL` (`vitest` missing in current environment; unchanged by H1 scope)
  - `cargo test -p sem-cli` => `PASS` (10 passed, 0 failed), including:
    - CLI contract tests for `--tui`/`--format` conflict and `--diff-view` semantics
    - stdin + two-file acquisition tests
    - TUI list-state navigation/quit tests
  - `cargo test -p sem-core` => `PASS` (35 passed, 0 failed) regression check
  - manual:
    - `cargo run -p sem-cli -- diff --tui --format json` => deterministic clap error (mutual exclusivity enforced)
    - `cargo run -p sem-cli -- diff --diff-view unified` => deterministic clap error (`--tui` required)
    - `printf '[]' | cargo run -p sem-cli -- diff --stdin --tui` => `No changes detected.` and exit 0 (no TUI launch)
    - two-file same-content run with `--tui` => `No semantic changes detected.` and exit 0 (deterministic non-launch path)
- Review run IDs + triage outcomes:
  - `r_20260308063746875_35679ccf` (generic-gemini):
    - `accept`: add explicit comment clarifying stable file sort preserves within-file semantic order.
    - `defer`: none.
    - `reject`: none.
  - `r_20260308063903839_cbda06a2` (generic-pi):
    - `accept`: add missing low-risk tests (`files.len()==1` error path, `q` quit behavior, empty-change TUI fallback).
    - `defer`: two-file `old_file_path` enrichment for future detail headers (consider during H2 detail rendering scope).
    - `reject`: no-change behavior mismatch claim (current empty-result TUI path returns existing no-change terminal message); inert non-TUI `diff_view` presence and profile-duration concern accepted as non-blocking.
- Go/No-Go: GO
- Notes:
  - H1 deliverables closed: diff command phase refactor, `--tui`/`--diff-view` contracts, and TUI list-loop skeleton with deterministic stdin/two-file handling.

### H2 Evidence
- Completion date: 2026-03-08
- Commit hash(es): `1c7348a`
- Acceptance evidence:
  - `npm run lint` => `FAIL` (pre-existing TypeScript workspace/type dependency errors unrelated to Rust H2 scope)
  - `npm test` => `FAIL` (`vitest` missing in current environment; unchanged by H2 scope)
  - `cargo test -p sem-cli` => `PASS` (22 passed, 0 failed), including H2-focused coverage:
    - detail-mode transitions (`Enter`/`Esc`)
    - side-by-side width fallback behavior
    - hunk navigation boundaries (`n/p`)
    - paging/jump/help/tab key contracts (`PageUp/PageDown`, `g/G`, `?`, `Tab`)
    - multi-hunk diff rendering and one-sided added/deleted rendering
    - constrained-width render test with fallback path
  - `cargo test -p sem-core` => `PASS` (35 passed, 0 failed) regression check
  - `cargo fmt --all` => `PASS` (after restoring unintended out-of-scope workspace formatting changes)
- Review run IDs + triage outcomes:
  - `r_20260308064732786_bf0b0c9b` (generic-gemini):
    - `accept`: fix multi-hunk absolute line-number drift in detail adapter, replace UTF-8-unsafe truncation, expand key/width coverage tests.
    - `defer`: full render snapshot matrix (tracked for hardening in H3).
    - `reject`: range-label display ambiguity (`[Lx-Ly -> ...]`) as non-blocking format preference within locked H2 scope.
  - `r_20260308064828440_6e888de3` (generic-pi):
    - `accept`: add tests for help/tab/jump/scroll key behavior, make side-by-side truncation UTF-8 safe, remove side-by-side wrap to preserve columns, saturate scroll cast, and clear formatting/dead-code warnings.
    - `defer`: broader visual snapshot validation for renderer fidelity (H3 hardening scope).
    - `reject`: none.
- Go/No-Go: GO
- Notes:
  - During execution, `cargo fmt --all` initially touched unrelated files; those out-of-scope changes were restored before final H2 commit to preserve strict phase boundaries.

### H3 Evidence
- Completion date: 2026-03-08
- Commit hash(es): `923fd28`
- Acceptance evidence:
  - `npm run lint` => `FAIL` (pre-existing TypeScript workspace/type dependency failures; unchanged by Rust-only H3 scope)
  - `npm test` => `FAIL` (`vitest` missing in environment; unchanged by Rust-only H3 scope)
  - `cargo test -p sem-cli` => `PASS` (26 passed, 0 failed), including hardening additions:
    - formatter contract tests (`json` + terminal range label assertions)
    - additional TUI render/state tests (`help` overlay render path, missing-content resilience path)
  - `cargo test -p sem-core` => `PASS` (35 passed, 0 failed) regression check
  - manual/release-readiness:
    - `cargo run -p sem-cli -- diff --format json` => validated JSON output includes optional contract fields during real git-mode diff
    - synthetic large-diff smoke (`1500` entities, `5` semantic changes) => completed in `276ms`
    - `cargo tree -p sem-cli | rg \"ratatui|crossterm|similar\"` => dependency graph evidence captured for audit
  - docs/finalization closure:
    - updated `README.md`, `crates/README.md`
    - added `docs/reference/architecture.md`
    - added `docs/implementation/implementation-plan.md` milestone status
    - added root `CHANGELOG.md` entry
    - added topic dependency audit notes
- Review run IDs + triage outcomes:
  - `r_20260308065600430_7d0fd344` (generic-gemini):
    - `accept`: add missing H3 evidence closure, tighten hardening coverage (render/help + resilience tests), and ensure release docs/finalization artifacts are present.
    - `defer`: full renderer snapshot matrix breadth beyond current constrained-width render checks (tracked as residual hardening follow-up).
    - `reject`: none.
  - `r_20260308065737515_dd440b59` (generic-pi):
    - `accept`: avoid silent JSON serialization fallback (`expect` instead of default), expand test coverage for missing-content and overlay render paths, enrich dependency audit notes (license/MSRV/footprint).
    - `defer`: dedicated non-UTF8/binary integration scenario in full end-to-end TUI session (current placeholder-path resilience is unit-tested; broader binary ingestion path remains follow-up).
    - `reject`: workspace-wide `cargo fmt --check`/`cargo clippy` warning observations as pre-existing non-H3 gating conditions outside this topic’s scoped files.
- Go/No-Go: GO
- Notes:
  - H3 closure criteria satisfied for this topic: hardening + docs + finalization artifacts committed, reviews triaged, and Section 9 evidence completed.
  - 2026-03-08 post-H3 maintenance follow-up intentionally retired `docs/reference/architecture.md` and `docs/implementation/implementation-plan.md`; this phase plan remains the canonical implementation record for this topic.

### Post-H3 Follow-Up Evidence
- Completion date: 2026-03-08
- Commit hash(es): `445e6fc`, `751ef49`, `c2aefb5`
- Acceptance evidence:
  - `cargo test -p sem-cli` => `PASS` (35 passed, 0 failed) after follow-up fixes and additional renderer coverage.
  - `cargo build` => `PASS` (`sem-cli` builds cleanly with follow-up TUI styling/perf changes).
  - manual: verified first detail-open responsiveness improved via async syntax warmup + visible-window rendering and markdown-path syntax bypass fallback.
  - docs: removed redundant top-level docs:
    - `docs/reference/architecture.md`
    - `docs/implementation/implementation-plan.md`
    and retained this topic plan as source-of-truth.
- Review run IDs + triage outcomes:
  - `r_20260308084239404_b9d13078` (generic-gemini):
    - `accept`: monitor large-diff performance risks and keep TUI regression coverage active.
    - `defer`: none.
    - `reject`: none.
  - `r_20260308084351608_35d0ea3a` (generic-pi):
    - `accept`: close test gaps for hunk parsing/unified numbering/highlight overlay behavior; keep perf mitigations in place.
    - `defer`: broader cache-eviction policy refinement (current clear-on-cap strategy retained for now).
    - `reject`: restoring detail view-mode label in header (intentionally omitted by product preference).
- Go/No-Go: GO
- Notes:
  - Follow-up scope was limited to post-phase UX/performance hardening and plan/documentation reconciliation; no additional phase expansion.

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
   - update architecture/implementation overview docs only if they remain part of the active documentation surface.
   - add `CHANGELOG.md` milestone entry.
   - complete Section 9 evidence per phase.
   - publish final phase summary and go/no-go state.
