# Phase Task Plan: Diff TUI View Cycle + Compact Split

## Status
Locked

## 1. Scope
Deliver a third TUI mode (`Split`) that shows a compact entity sidebar and live diff pane simultaneously, plus deterministic view cycling via `v` across `List`, `Split`, and `Detail`.

## 2. Global Rules
1. Execute phases in strict order: `H0 -> H1 -> H2 -> H3`.
2. Preserve existing startup default (`List` mode).
3. Keep semantic diff/model outputs unchanged.
4. Keep list ordering/filtering/review-state semantics identical across list and split sidebars.
5. Keep footer cell rail consistent with existing `m`/`r`/`e` conventions, with additive `v` cell.
6. Require 2 independent external reviews in authoring stage (Gemini + PI).
7. Triage every finding as `accept`, `defer`, or `reject`.
8. Section 9 evidence logging is mandatory for every phase.

## 3. Phases, Deliverables, Acceptance

### H0: Contract Lock
Deliverables:
1. Lock mode model (`list`, `split`, `detail`) and `v` cycle order.
2. Lock `Enter`/`Esc` behavior, including `last_non_detail_mode` initialization (`List`).
3. Lock split row density contract (icon + name + inline `+/-`, no verbose type/change label columns).
4. Lock split focus model (left pane active, right pane passive preview) and split no-op key behavior (`Left/Right`, `n/p`, paging keys).
5. Lock split layout constraints (ratio and minimum columns), preview-state semantics, and narrow-width fallback notice behavior.
6. Lock footer/help contracts for `v`.

Acceptance:
1. No ambiguity in mode transition rules.
2. No ambiguity in split interaction/focus behavior.
3. Design/schema/plan are internally consistent.

Gate:
- GO only when authoring reviews are complete and triaged.

### H1: App State + Key Handling
Deliverables:
1. Extend TUI mode enum to include `Split`.
2. Add `v` key reducer for deterministic mode cycling.
3. Add and wire `last_non_detail_mode` state.
4. Keep `Enter` from `List`/`Split` opening detail.
5. Keep `Esc` in detail returning to prior non-detail mode.
6. Add app-state tests for cycle order, wrap behavior, enter behavior, and detail escape target.

Acceptance:
1. `v` cycles exactly `list -> split -> detail -> list`.
2. `Esc` behavior is correct for both list-entered and split-entered detail.
3. Existing mode-agnostic controls remain functional.

Gate:
- GO only if `cargo test -p sem-cli` passes with new app-state coverage.

### H2: Split Renderer + Compact Sidebar
Deliverables:
1. Add split render path with horizontal pane layout.
2. Implement compact left pane rows and file headers with ellipsis truncation.
3. Reuse extracted detail render helpers for split right pane preview.
4. Wire split `Tab` behavior for unified/side-by-side preview toggle.
5. Implement narrow-width fallback render path (`split compact-list-only with notice`).
6. Add renderer tests for compact content, file grouping, truncation, right-pane preview updates, and fallback behavior.

Acceptance:
1. Split sidebar is visibly denser than list view.
2. Right pane updates with selected row.
3. Narrow-width behavior is stable and non-panicking.
4. No regressions in list/detail rendering.

Gate:
- GO only if split render tests and regression tests pass.

### H3: Footer/Help/Docs/Hardening
Deliverables:
1. Add `v: <view>` footer cell in locked order `m`, `r`, `e`, `v`.
2. Update controls string and help overlay text for `v`.
3. Update README docs and changelog entries.
4. Add hardening tests for:
   - split selection-right-pane sync,
   - filter-driven selection fallback in split,
   - detail-wrap cycle boundary,
   - split side-by-side behavior under narrow width.
5. Complete Section 9 evidence entries.

Acceptance:
1. Footer and help reflect locked view-cycle contract.
2. Docs match runtime behavior.
3. Hardening tests confirm stable behavior under constrained layouts.

Gate:
- GO only with passing tests, docs updates, and completed Section 9 evidence.

## 4. Verification Matrix
| Area | Verification Type | Command / Evidence |
|---|---|---|
| contract lock | doc review | design/schema/plan consistency pass |
| mode cycle order | app test | `cargo test -p sem-cli` mode transition assertions |
| detail cycle wrap | app test | explicit `detail -> v -> list` assertion |
| enter semantics | app test | `Enter` from list/split opens detail |
| esc return path | app test | list-entered and split-entered detail return target assertions |
| split compact row format | render test | split row excludes verbose type/change text |
| split file grouping | render test | file headers preserved and deterministic |
| split selection sync | render/app test | sidebar selection change updates right-pane preview |
| split truncation | render test | long file/entity labels truncate with ellipsis |
| split filter fallback | app/render test | filtered-out selected row reselects deterministic visible row |
| split tab toggle | app/render test | split `Tab` toggles unified/side-by-side preview |
| split hunk keys no-op | app test | split `n/p` does not mutate mode, selection, or cursor state |
| split paging keys no-op | app test | split `PageUp/PageDown` and line-scroll keys are no-op |
| split narrow fallback | render test | degraded split render path with notice; no panic |
| cycle round-trip stability | app test | `List -> Enter -> Detail -> v -> List -> v -> Split` remains deterministic |
| footer view cell | render test | `v: list|split|detail` appears in footer rail |
| help overlay update | render test | includes `v` view-cycle guidance |
| key regression | app/render tests | existing `m`, `r`, `e`, `Tab`, `n/p` behavior unaffected where applicable |
| full regression | test suite | `cargo test -p sem-cli && cargo test -p sem-core` |

## 5. Milestone Commit Gate
1. One milestone commit per phase.
2. Commit template:
   - `feat(sem): H0 split-view contract lock`
   - `feat(sem): H1 view-cycle app state + key handling`
   - `feat(sem): H2 split renderer compact sidebar`
   - `feat(sem): H3 split footer/help/docs hardening`

## 6. Risks and Mitigations
1. Risk: split layout adds renderer complexity and regression risk.
- Mitigation: isolate split renderer path and keep list/detail regression tests.
2. Risk: mode transition confusion (`v`, `Enter`, `Esc`).
- Mitigation: lock deterministic transition matrix and explicit boundary tests.
3. Risk: footer overcrowding under narrow width.
- Mitigation: compact `v` cell and existing footer width arbitration.
4. Risk: rapid sidebar navigation can stress preview updates.
- Mitigation: add high-frequency navigation safety tests; deeper caching/debounce remains deferred.

## 7. Deferred Items
1. Persisting selected view mode between sessions.
2. User-configurable split ratio beyond locked defaults.
3. Class-nested tree sidebar.
4. Dedicated right-pane focus mode in split.
5. Diff-preview caching/debouncing optimizations for extreme high-frequency selection changes.

## 8. Review Findings Triage Ledger (Authoring Stage)

### Review Run A (generic-gemini)
- Run ID: `r_20260310020429305_434d799e`
- Findings summary:
  - ambiguous split input routing and pane-focus semantics
  - missing width/overflow constraints and truncation expectations
  - test gaps for wrap boundaries and high-frequency selection updates
  - performance risk on rapid selection movement
- Triage:
  - `accept`:
    - locked split focus model as left-pane-active/right-pane-passive,
    - locked split width/min-column + truncation contract,
    - expanded verification matrix for detail-wrap, selection-sync, truncation, and input-routing behavior.
  - `defer`:
    - advanced preview caching/debouncing policy to post-baseline hardening scope.
  - `reject`:
    - none.

### Review Run B (generic-pi)
- Run ID: `r_20260310020506477_bb62e53e`
- Findings summary:
  - ambiguity around startup value for `last_non_detail_mode`
  - insufficiently explicit split key map (`j/k`, `Tab`, `Left/Right`)
  - unclear runtime-schema narration (conceptual model vs serialized payload)
  - test gaps around split filter fallback and selection sync
- Triage:
  - `accept`:
    - locked `last_non_detail_mode` initialization to `List`,
    - locked split key semantics (`j/k` left-pane navigation, `Tab` preview toggle, `Left/Right` split no-op),
    - clarified schema as documentation-only runtime contract model,
    - expanded test matrix for selection sync, filter fallback, and cycle boundary behavior,
    - corrected baseline wording from optional `r` cell to current `m/r/e` reality.
  - `defer`:
    - dedicated right-pane focus mode in split to a future UX iteration.
  - `reject`:
    - alternative footer order variants; `m,r,e,v` is intentionally locked to minimize disruption while adding `v`.

## 9. Operator Checklist and Evidence Log Schema

### 9.1 Checklist Per Phase
1. Validate prior phase GO.
2. Execute only current-phase deliverables.
3. Run required verification commands.
4. Run required reviews and triage all findings.
5. Record Section 9 evidence before phase close.

### 9.2 Evidence Schema Template
```md
### Hx Evidence
- Completion date: YYYY-MM-DD
- Commit hash(es): <hashes>
- Acceptance evidence:
  - <command> => <summary>
  - manual: <validated behavior>
- Review run IDs + triage outcomes:
  - <run-id>: accept|defer|reject summary
- Go/No-Go: GO | NO-GO
- Notes: <optional>
```

### 9.3 Authoring-Stage Evidence (Spec Plan)
- Completion date: 2026-03-10
- Commit hash(es): N/A (planning stream)
- Acceptance evidence:
  - drafted required artifacts (`design.md`, `phase-task-plan.md`, `schema-proposal.md`)
  - executed 2 independent review runs with stream-confirmed terminal events
  - triaged findings and applied accepted updates
- Review run IDs + triage outcomes:
  - `r_20260310020429305_434d799e`: accept + defer, no rejects
  - `r_20260310020506477_bb62e53e`: accept + defer + reject
- Go/No-Go: GO
- Notes:
  - both runs completed via stream terminal event `result.completed`; no fallback retry needed.

### 9.4 H0 Evidence
- Completion date: 2026-03-10
- Commit hash(es): `b83d726`
- Acceptance evidence:
  - manual: design/schema/plan consistency pass completed after clarifying split preview-state semantics, split no-op key semantics, and fallback notice behavior.
  - manual: H0 contract deliverables locked across `design.md`, `schema-proposal.md`, and verification matrix additions in `phase-task-plan.md`.
- Review run IDs + triage outcomes:
  - `r_20260310022103762_522eb9e1`: `accept` clarity additions for split preview-scrolling boundary + fallback notice placement; `defer` performance/layout-thrash mitigation to post-baseline optimization scope.
  - `r_20260310022150794_eda06e4d`: `accept` split key-contract clarifications (`Tab` preview-only, `n/p` + paging no-op in split), preview-state contract, and matrix expansion; `defer` footer-width stress/perf-hardening guidance to later phase; `reject` schema token-mismatch concern (`e: hunk`) as not a contract defect.
- Go/No-Go: GO
- Notes:
  - both execution-stage review runs were closed only after stream terminal event `result.completed`.

### 9.5 H1 Evidence
- Completion date: 2026-03-10
- Commit hash(es): `c5b406d`
- Acceptance evidence:
  - `cargo test -p sem-cli` (run from `crates/`) => pass, 126 passed / 0 failed; includes new mode-cycle and escape-path assertions.
  - `npm run lint` => fail (environment baseline: missing Node/TS dependency resolution in workspace; unrelated to Rust TUI scope).
  - `npm test` => fail (`vitest: command not found` in current environment baseline).
  - manual: `Mode::Split` added, global `v` cycle wired (`list -> split -> detail -> list`), `Enter` works from list/split, `Esc` in detail returns recorded prior non-detail mode.
- Review run IDs + triage outcomes:
  - `r_20260310023037903_6645bc0f`: `accept` baseline transition implementation and test scope; `defer` duplicate list/split key-handler cleanup to later hardening.
  - `r_20260310023214886_456dd586`: `accept` additional transition coverage (`escape_from_detail_entered_via_v_cycle_returns_to_split`, `escape_in_split_mode_is_noop`); `defer` split rendering/help-copy items to H2/H3 where they are in scope; `reject` claims conflicting with locked H1 scope (`Split` visual parity with `List` in H1, `Esc` in split requiring mode change).
- Go/No-Go: GO
- Notes:
  - both reviewer runs were completed via stream terminal event `result.completed`.

## 10. Execution Handoff Contract
1. Required read order:
   1) `docs/implementation/diff-tui-view-cycle-compact-split/schema-proposal.md`
   2) `docs/implementation/diff-tui-view-cycle-compact-split/design.md`
   3) `docs/implementation/diff-tui-view-cycle-compact-split/phase-task-plan.md`
2. Start point:
   - start at `H0` only.
3. Boundaries and semantic-preservation constraints:
   - preserve default startup mode (`List`),
   - preserve existing list/detail behavior outside additive split feature,
   - no semantic diff/model/output contract changes,
   - no new CLI flags.
4. Review command policy requirements:
   - use `agent-runner-review` mechanics,
   - no timeout/reasoning-effort CLI overrides,
   - completion must be confirmed from stream events (`result.completed|result.failed`).
5. Completion requirements:
   - update docs when behavior stabilizes,
   - add `CHANGELOG.md` milestone entry,
   - complete Section 9 evidence per phase,
   - publish final phase summary.

## 11. Default Compact Handoff Prompt
Use `$agent-runner-spec-execution` and `$agent-runner-review`.

Topic slug: `diff-tui-view-cycle-compact-split`.

Read:
1) `docs/implementation/diff-tui-view-cycle-compact-split/schema-proposal.md`
2) `docs/implementation/diff-tui-view-cycle-compact-split/design.md`
3) `docs/implementation/diff-tui-view-cycle-compact-split/phase-task-plan.md`

Execute all phases declared in `phase-task-plan.md` in strict order, using the plan as source of truth for scope, gates, review/triage policy, and Section 9 evidence updates.
