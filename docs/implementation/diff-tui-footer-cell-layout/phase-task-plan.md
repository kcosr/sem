# Phase Task Plan: Diff TUI Footer Cell Layout

## Status
Locked

## 1. Scope
Implement the shared footer cell layout baseline and migrate step-mode display to `m: <mode>` so later topics can add `r` and `e` cells without layout churn.

## 2. Global Rules
1. Execute phases in strict order: `H0 -> H1 -> H2`.
2. Preserve stepping semantics and keybindings.
3. Keep footer-state rendering deterministic across list/detail.
4. Section 9 evidence updates are mandatory.

## 3. Phases, Deliverables, Acceptance

### H0: Contract Lock
Deliverables:
1. Lock footer rail contract, cell format, and ordering (`m`, `r`, `e`).
2. Lock migration of step-mode token to `m:`.
3. Lock canonical cell delimiter (` | `).
4. Lock status-slot behavior, loading color behavior, and status-vs-cell truncation priority.

Acceptance:
1. No ambiguity on token formats or ordering.
2. No ambiguity on status-slot behavior.

Gate:
- GO only when docs are consistent.

### H1: Renderer Baseline Implementation
Deliverables:
1. Add footer cell renderer helper.
2. Migrate mode token rendering to `m: pairwise|cumulative`.
3. Add status slot rendering that does not recolor whole footer.
4. Add list/detail render tests for mode cell.

Acceptance:
1. `m` cell appears in list and detail.
2. Loading/status slot does not mutate state-cell color/value.

Gate:
- GO only with passing `cargo test -p sem-cli`.

### H2: Hardening + Docs Alignment
Deliverables:
1. Add narrow-width layout tests for cell visibility priority (including long-status contention).
2. Update affected implementation specs to consume footer-cell contract.
3. Complete Section 9 evidence.

Acceptance:
1. Narrow-width behavior remains deterministic.
2. Downstream specs are aligned to new footer-cell baseline.

Gate:
- GO only with tests + docs + evidence complete.

## 4. Verification Matrix
| Area | Verification | Command / Evidence |
|---|---|---|
| contract consistency | doc review | design/schema/plan cross-check |
| mode cell render | render tests | list/detail footer contains `m: <mode>` |
| status-slot behavior | render tests | loading/status in dedicated slot only |
| narrow layout | render tests | cells preserved under constrained width, including long-status contention |
| regression safety | test suite | `cargo test -p sem-cli` |

## 5. Milestone Commit Gate
1. One milestone commit per phase.
2. Commit template:
   - `feat(sem): H0 footer-cell contract lock`
   - `feat(sem): H1 footer cell baseline + m token`
   - `feat(sem): H2 footer hardening + downstream spec alignment`

## 6. Risks and Mitigations
1. Risk: footer truncation hides critical state.
- Mitigation: prioritize state-cell visibility.
2. Risk: status rendering regresses existing footer styles.
- Mitigation: dedicated status slot and render tests.

## 7. Deferred Items
1. `r` cell implementation (review-state topic).
2. `e` cell implementation (full-entity-toggle topic).

## 8. Review Findings Triage Ledger
1. This addendum is a narrow contract/sequence alignment patch.
2. No independent external review runs were executed for this brief addendum.

## 9. Operator Checklist and Evidence Log Schema

### 9.1 Checklist Per Phase
1. Validate prior phase GO.
2. Execute only current phase deliverables.
3. Run required verification commands.
4. Record Section 9 evidence before phase close.

### 9.2 Evidence Schema Template
```md
### Hx Evidence
- Completion date: YYYY-MM-DD
- Commit hash(es): <hashes>
- Acceptance evidence:
  - <command> => <summary>
  - manual: <validated behavior>
- Go/No-Go: GO | NO-GO
- Notes: <optional>
```

### 9.3 Authoring-Stage Evidence (Addendum)
- Completion date: 2026-03-08
- Commit hash(es): pending
- Acceptance evidence:
  - authored addendum artifacts (`design.md`, `phase-task-plan.md`, `schema-proposal.md`)
  - aligned downstream specs to this footer-cell contract
- Go/No-Go: GO
- Notes:
  - narrow addendum scope; implementation-phase reviews can occur during execution.

### 9.4 H0 Evidence
- Completion date: 2026-03-08
- Commit hash(es): `5aa1640`
- Acceptance evidence:
  - manual: cross-checked `design.md`, `schema-proposal.md`, and this phase plan for H0 consistency after applying accepted contract clarifications (cell delimiter, status-vs-cell priority, and runtime/schema wording).
  - `npm run lint` => `NO-GO` for global JS/TS workspace baseline (missing node/module type dependencies in current environment; unrelated to H0 docs scope).
  - `npm test` => `NO-GO` for global JS workspace baseline (`vitest` binary unavailable in current environment; unrelated to H0 docs scope).
  - `cargo test -p sem-cli` (run in `crates/`) => PASS (69 passed).
- Review run IDs + triage outcomes:
  - `r_20260308194820428_3cd2c8d1` (`generic-gemini`): `accept` status-vs-cell priority lock, value-domain clarification wording, and explicit long-status contention coverage in H2; `defer` none; `reject` none.
  - `r_20260308194853933_c855769f` (`generic-pi`): `accept` canonical cell delimiter lock, enum-vs-runtime-leniency wording, rendered-field scope clarification, and controls-area contract note; `defer` fixed width-budget threshold and unknown-key/fallback dedicated tests to H1/H2; `reject` none.
- Go/No-Go: GO
- Notes:
  - external review completion was confirmed from live session stream terminal events (`result.completed`) for both runs.

### 9.5 H1 Evidence
- Completion date: 2026-03-08
- Commit hash(es): `f1ddf1a`
- Acceptance evidence:
  - manual: verified H1 renderer baseline in `crates/sem-cli/src/tui/render.rs`:
    - footer cell helper model (`FooterCell`, `FooterParts`)
    - `mode:` token migration to `m: <mode>`
    - dedicated status slot rendering separated from cell rail
    - list/detail footer builders using mode cell contract
  - `npm run lint` => `NO-GO` for global JS/TS workspace baseline (missing node/module type dependencies in current environment; unrelated to Rust H1 scope).
  - `npm test` => `NO-GO` for global JS workspace baseline (`vitest` binary unavailable in current environment; unrelated to Rust H1 scope).
  - `cargo test -p sem-cli` (run in `crates/`) => PASS (73 passed), including H1 footer tests:
    - `list_footer_parts_include_mode_cell`
    - `detail_footer_parts_include_cumulative_mode_cell`
    - `detail_footer_loading_status_keeps_mode_cell_value`
    - `footer_layout_widths_reserve_separator_for_status_slot`
- Review run IDs + triage outcomes:
  - `r_20260308195452782_e89fc013` (`generic-gemini`): `accept` H1 baseline completeness and mode/status contract alignment; `accept` spacing-separator defect between cell rail and status slot (fixed before commit); `defer` none; `reject` none.
  - `r_20260308195556545_cc834395` (`generic-pi`): `accept` H1 scope completeness and test coverage; `defer` additional narrow-width long-status balancing policy to H2 hardening; `reject` none.
- Go/No-Go: GO
- Notes:
  - external review completion was confirmed from live session stream terminal events (`result.completed`) for both runs.

## 10. Execution Handoff Contract
1. Required read order:
   1) `docs/implementation/diff-tui-footer-cell-layout/schema-proposal.md`
   2) `docs/implementation/diff-tui-footer-cell-layout/design.md`
   3) `docs/implementation/diff-tui-footer-cell-layout/phase-task-plan.md`
2. Start at `H0`.
3. Execute phases in strict order using this plan as source of truth for scope, gates, and Section 9 evidence updates.
