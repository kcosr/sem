# Phase Task Plan: Diff TUI Entity Review State

## Status
Locked

## 1. Scope
Deliver entity-level reviewed toggling, reviewed/unreviewed filtering, deterministic navigation under filtered views, and persistent local review-state metadata while preserving unified-stepping behavior and footer contracts.

## 2. Global Rules
1. Execute phases in strict order: `H0 -> H1 -> H2 -> H3`.
2. Preserve existing non-TUI and existing TUI diff semantics.
3. Keep entity-level scope only (no hunk-level review state).
4. Keep all persistence failures non-fatal.
5. Preserve unified stepping semantics (`[`/`]`, `m`, comparator endpoints, and step-mode indicator token).
6. Run 2 independent external reviews in authoring stage (Gemini + PI).
7. Triage each finding as `accept`, `defer`, or `reject`.
8. Section 9 evidence updates are mandatory.

## 3. Phases, Deliverables, Acceptance

### H0: Contract Lock
Deliverables:
1. Lock keybindings for toggle/filter actions.
2. Lock identity+hash carryover semantics.
3. Lock filter/no-match navigation behavior.
4. Lock persistence file path/shape and error semantics.
5. Lock `logicalEntityKey` grammar and `targetContentHash` normalization.
6. Lock comparator-target hash source semantics for commit/index/working endpoints.
7. Lock toggle-under-filter cursor behavior and startup filter-restore behavior.

Acceptance:
1. No ambiguity on reviewed identity key semantics.
2. No ambiguity on no-match UX and keyboard no-op behavior.
3. Docs internally consistent (`design`, `schema`, `plan`).

Gate:
- GO only when contract docs are consistent and review triage is complete.

### H1: State + Persistence Foundation
Deliverables:
1. Add app-state review map and filter state.
2. Implement persistence load/save with versioned schema.
3. Add deterministic identity + target content hash helpers.
4. Add unit tests for state transitions and persistence edge cases.

Acceptance:
1. Existing TUI behavior unchanged when feature not used.
2. Persistence read/write failures are non-fatal and visible.

Gate:
- GO only if tests pass and baseline behavior remains stable.

### H2: Interaction + Rendering
Deliverables:
1. Wire `Space` toggle in list/detail.
2. Wire filter cycle key and footer indicator.
3. Render reviewed markers.
4. Filtered list projection with file-header suppression and global no-match row.
5. Navigation skips hidden rows and no-ops when none visible.
6. Preserve `m` step-mode footer cell while adding `r: <state>` filter cell.

Acceptance:
1. Toggle/filter behavior deterministic in both list and detail modes.
2. No-match state is explicit and stable.

Gate:
- GO only with passing interaction/render tests and manual verification.

### H3: Hardening + Docs
Deliverables:
1. Hardening tests for cross-range/mode carryover and hash mismatch.
2. README/docs updates for keys and filter behavior.
3. Changelog milestone entry.
4. Section 9 evidence closure for all phases.

Acceptance:
1. Tests validate carryover and non-carryover scenarios.
2. User docs match runtime behavior.

Gate:
- GO only with complete evidence and resolved/triaged findings.

## 4. Verification Matrix
| Area | Verification | Command / Evidence |
|---|---|---|
| contract consistency | doc review | design/schema/plan cross-check |
| reviewed toggle state | app tests | `cargo test -p sem-cli` targeted app tests |
| filter cycle state | app tests | filter state reducer assertions |
| hidden-row navigation | app tests | skip/no-op behavior assertions |
| toggle-under-filter cursor behavior | app tests | focused-row hide path advances deterministically or falls back to no-match |
| reviewed identity carryover | unit/integration | identity+hash match/mismatch tests |
| stepping compatibility | unit/integration | pairwise/cumulative carryover invariants |
| endpoint-kind compatibility | unit/integration | comparator target endpoint commit/index/working coverage |
| fallback identity key | unit tests | fallback key grammar + ordinal tests |
| added entity hash path | unit/integration | add-path target-content hash + carryover tests |
| deleted entity handling | unit/integration | deleted-content hash + carryover tests |
| non-UTF-8 hash material | unit/integration | invalid UTF-8 treated as missing hash material |
| persistence behavior | unit tests | missing/corrupt/version/repo mismatch + atomic write tests |
| persistence compaction | unit tests | dedupe + max-record cap assertions |
| filter preference restore | unit tests | `uiPrefs.reviewFilter` load/default behavior |
| debounce exit flush | unit/integration | state mutation persists on shutdown flush |
| filtered rendering | render tests | reviewed markers + file header suppression |
| footer filter cell | render tests | `r: all|unreviewed|reviewed` coexists with `m: <mode>` |
| no-match rendering | render tests | explicit empty-state row |
| scale smoke | perf/manual | startup/load with `>=10k` records |
| regression safety | full tests | `cargo test -p sem-cli && cargo test -p sem-core` |

## 5. Milestone Commit Gate
1. One milestone commit per phase.
2. Commit template:
   - `feat(sem): H0 review-state contracts lock`
   - `feat(sem): H1 review-state store + identity foundation`
   - `feat(sem): H2 reviewed toggle + filter UX`
   - `feat(sem): H3 review-state hardening + docs`

## 6. Risks and Mitigations
1. Risk: identity drift across parser/rename scenarios.
- Mitigation: locked grammar + normalization + targeted tests.
2. Risk: filter hides all rows and causes confusing controls.
- Mitigation: explicit no-match row + no-op controls.
3. Risk: persistence corruption or write failures.
- Mitigation: atomic write + non-fatal fallback.
4. Risk: cross-range carryover confusion.
- Mitigation: strict identity+hash matching only.
5. Risk: unbounded state growth.
- Mitigation: startup compaction + max-record cap.
6. Risk: review UI changes regress unified-stepping footer semantics.
- Mitigation: explicit footer composition contract + render regression tests.
7. Risk: footer-cell contract drift across topics.
- Mitigation: enforce `diff-tui-footer-cell-layout` addendum as shared source of truth.
8. Risk: repo path relocation invalidates `repoId` and suppresses prior review records.
- Mitigation: explicit canonicalization contract + non-fatal mismatch semantics and status hint.

## 7. Deferred Items
1. Hunk-level review state.
2. Annotation authoring/display.
3. Resume exact range/cursor/session view.
4. Cross-repo/cloud synchronization.
5. Strong multi-process merge conflict handling beyond last-writer-wins.

## 8. Review Findings Triage Ledger (Authoring Stage)

### Review Run A (generic-gemini)
- Run ID: `r_20260308183714494_0f85631f`
- Findings summary:
  - missing pruning/eviction strategy
  - concurrent-session persistence concerns
  - deleted-entity hash contract ambiguity
  - repo/hash normalization ambiguity
  - missing scale/corrupt-file safety tests
- Triage:
  - `accept`:
    - locked compaction + max-record cap
    - locked deleted-entity hash semantics
    - locked normalization and repoId derivation notes
    - expanded matrix for scale and corrupt/mismatch paths
  - `defer`:
    - robust multi-process merge handling (beyond last-writer-wins) to future hardening
  - `reject`:
    - none

### Review Run B (generic-pi)
- Run ID: `r_20260308183748681_b592554a`
- Findings summary:
  - underspecified fallback key grammar
  - underspecified normalization details
  - repoId mismatch behavior missing
  - step-mode preference coupling concern
  - deleted path and fallback-key test gaps
- Triage:
  - `accept`:
    - locked `logicalEntityKey` grammar and normalization algorithm
    - locked repo mismatch behavior
    - removed `stepMode` from review-state persistence scope
    - expanded test matrix for deleted/fallback cases
    - clarified detail-mode `Space` scope
  - `defer`:
    - broader concurrent writer conflict resolution model to future topic
  - `reject`:
    - none

## 9. Operator Checklist and Evidence Log Schema

### 9.1 Checklist Per Phase
1. Validate prior phase GO.
2. Execute only current phase deliverables.
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
- Completion date: 2026-03-08
- Commit hash(es): N/A (planning stream)
- Acceptance evidence:
  - drafted required artifacts (`design.md`, `phase-task-plan.md`, `schema-proposal.md`)
  - executed 2 independent review runs with stream-confirmed terminal events
  - triaged all findings and applied accepted edits
- Review run IDs + triage outcomes:
  - `r_20260308183714494_0f85631f`: accept + defer, no rejects
  - `r_20260308183748681_b592554a`: accept + defer, no rejects
- Go/No-Go: GO
- Notes:
  - review completion confirmed from live session stream terminal events (`result.completed`).

### 9.4 Post-Lock Alignment Amendment
- Completion date: 2026-03-08
- Commit hash(es): pending
- Acceptance evidence:
  - cross-checked this spec set against post-H3 unified-stepping contracts (`pairwise`/`cumulative`, commit/index/working endpoints, and footer mode indicator requirements).
  - amended design/plan/schema to preserve stepping semantics, define comparator-target hash source across endpoint kinds, and lock footer filter cell format (`r: <state>`) under shared footer-cell addendum.
- Review run IDs + triage outcomes:
  - N/A (alignment-only amendment; no contract-scope expansion beyond compatibility clarifications)
- Go/No-Go: GO
- Notes:
  - changes are compatibility clarifications and do not alter review-state feature scope or persistence schema version.

### 9.5 H0 Evidence
- Completion date: 2026-03-08
- Commit hash(es): `eadaa64`
- Acceptance evidence:
  - manual: cross-checked and updated `design.md`, `schema-proposal.md`, and this plan for H0 lock completeness:
    - finalized `r` keybinding wording
    - locked toggle-under-filter cursor behavior
    - locked startup `uiPrefs.reviewFilter` restore/default behavior
    - locked canonical UTC `updatedAt` format and canonicalized `repoId` derivation wording
    - clarified add/delete hash material and non-UTF-8 hash-material fallback semantics
    - expanded verification matrix coverage for the new lock clarifications
  - `npm run lint` => `NO-GO` for global JS/TS workspace baseline (missing Node/module typings and other pre-existing TypeScript dependency issues unrelated to H0 docs scope).
  - `npm test` => `NO-GO` for global JS workspace baseline (`vitest` binary unavailable in current environment; unrelated to H0 docs scope).
  - `cargo test -p sem-cli` (run in `crates/`) => PASS (76 passed).
- Review run IDs + triage outcomes:
  - `r_20260308202215129_b071d13a` (`generic-gemini`): `accept` startup filter-restore contract lock and strict `updatedAt` format lock; `defer` whitespace-driven hash churn UX expectations and concurrent/debounce loss caveats to implementation/hardening phases; `reject` none.
  - `r_20260308202254539_3d17ffc6` (`generic-pi`): `accept` keybinding finalization wording (`r`), semantic-emission-order definition, toggle-under-filter cursor rule, repo-path canonicalization wording, add-path hash clarification, and non-UTF-8 hash-material fallback semantics; `defer` `.gitignore` guidance final wording to H3 docs pass; `reject` none.
- Go/No-Go: GO
- Notes:
  - external review completion was confirmed from live session stream terminal events (`result.completed`) for both runs.

### 9.6 H1 Evidence
- Completion date: 2026-03-08
- Commit hash(es): `0596184`
- Acceptance evidence:
  - manual: verified H1 scope landed in Rust TUI foundation only (no H2 keybinding/render behavior changes):
    - new persistence + identity/hash module: `crates/sem-cli/src/tui/review_state.rs`
    - app-state review map/filter foundation and dirty-snapshot plumbing: `crates/sem-cli/src/tui/app.rs`
    - startup load + debounced save + exit flush integration: `crates/sem-cli/src/tui/mod.rs`
    - footer status slot now surfaces review-state warnings: `crates/sem-cli/src/tui/render.rs`
  - `npm run lint` => `NO-GO` for global JS/TS workspace baseline (pre-existing missing Node/module typings and dependency issues unrelated to Rust H1 scope).
  - `npm test` => `NO-GO` for global JS workspace baseline (`vitest` binary unavailable in current environment; unrelated to Rust H1 scope).
  - `cargo fmt -p sem-cli` (run in `crates/`) => PASS.
  - `cargo test -p sem-cli` (run in `crates/`) => PASS (91 passed), including new H1 state/persistence tests in:
    - `tui::app::tests::{review_toggle_tracks_review_records_when_hash_context_is_available, review_toggle_noops_when_comparator_hash_source_is_unavailable, review_filter_cycle_marks_persistence_dirty}`
    - `tui::review_state::tests::*` (load/save/compaction/schema-mismatch/hash helpers)
  - `cargo test -p sem-core` (run in `crates/`) => PASS (41 passed).
- Review run IDs + triage outcomes:
  - `r_20260308203516511_f6613e52` (`generic-gemini`): `accept` H1 scope completeness and persistence-loop contract alignment; `defer` cross-session debounce/write-window caveats to later hardening scope; `reject` none.
  - `r_20260308203630821_e589a884` (`generic-pi`): `accept` H1 boundary correctness (no premature H2 key/render wiring) and broad test coverage; `defer` explicit timed scale smoke and fallback-ordinal collision hardening assertions to H3; `reject` none.
- Go/No-Go: GO
- Notes:
  - external review completion was confirmed from live session stream terminal events (`result.completed`) for both runs.

### 9.7 H2 Evidence
- Completion date: 2026-03-08
- Commit hash(es): `d8066cf`
- Acceptance evidence:
  - manual: verified H2 interaction/render scope landed and remains deterministic in list/detail modes:
    - `Space` toggle wired in list + detail mode handlers
    - `r` filter cycle wired in list + detail mode handlers
    - filtered projection drives list rows, file-header suppression, and no-match row rendering
    - list/detail navigation skips hidden rows and no-ops when no visible entities remain
    - footer/help text updated with `r: <state>` cell while preserving `m` cell ordering contract
  - `cargo fmt -p sem-cli` (run in `crates/`) => PASS.
  - `cargo test -p sem-cli` (run in `crates/`) => PASS (99 passed), including H2-focused coverage:
    - `tui::app::tests::{filtered_navigation_skips_hidden_rows_in_list_mode, toggle_under_active_filter_can_result_in_no_match_state, detail_left_right_navigation_respects_active_filter_visibility, detail_mode_space_toggles_reviewed_state_for_opened_entity, detail_mode_filter_cycle_retargets_when_focused_entity_becomes_hidden}`
    - `tui::render::tests::{draw_list_mode_shows_no_match_row_when_filter_hides_all_entities, draw_list_mode_shows_reviewed_marker_for_non_selected_reviewed_rows, draw_list_mode_hides_file_headers_without_visible_entities}`
  - `cargo test -p sem-core` (run in `crates/`) => PASS (41 passed).
  - `npm run lint` => `NO-GO` for global JS/TS workspace baseline (pre-existing missing Node/module typings and dependency issues unrelated to Rust H2 scope).
  - `npm test` => `NO-GO` for global JS workspace baseline (`vitest` binary unavailable in current environment; unrelated to Rust H2 scope).
- Review run IDs + triage outcomes:
  - `r_20260308204354661_5140108c` (`generic-gemini`): `accept` additional H2 coverage for reviewed-marker rendering, partial file-header suppression, and explicit detail-mode toggle/filter behavior (applied in H2 tests); `defer` list projection caching/perf concerns and optional detail-view reviewed indicator to future hardening/UX iteration; `reject` none.
  - `r_20260308204459814_0a0cbe1e` (`generic-pi`): `accept` additional H2 coverage for reviewed-marker render assertions and detail-mode behavior when filter visibility changes (applied in H2 tests); `defer` non-blocking selection-overflow UX nuance and visible-index allocation optimization to future hardening; `reject` none.
- Go/No-Go: GO
- Notes:
  - external review completion was confirmed from live session stream terminal events (`result.completed`) for both runs.

## 10. Execution Handoff Contract
0. Prerequisite:
   - `diff-tui-footer-cell-layout` implementation is landed (shared footer cell baseline with `m` cell and cell-order contract).
1. Required read order:
   1) `docs/implementation/diff-tui-entity-review-state/schema-proposal.md`
   2) `docs/implementation/diff-tui-entity-review-state/design.md`
   3) `docs/implementation/diff-tui-entity-review-state/phase-task-plan.md`
2. Start at `H0`.
3. Execute phases in strict order using this plan as source of truth for scope, gates, review/triage policy, and Section 9 evidence updates.
