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
6. Preserve step-mode footer token while adding review filter indicator.

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
| reviewed identity carryover | unit/integration | identity+hash match/mismatch tests |
| stepping compatibility | unit/integration | pairwise/cumulative carryover invariants |
| endpoint-kind compatibility | unit/integration | comparator target endpoint commit/index/working coverage |
| fallback identity key | unit tests | fallback key grammar + ordinal tests |
| deleted entity handling | unit/integration | deleted-content hash + carryover tests |
| persistence behavior | unit tests | missing/corrupt/version/repo mismatch + atomic write tests |
| persistence compaction | unit tests | dedupe + max-record cap assertions |
| filtered rendering | render tests | reviewed markers + file header suppression |
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
  - amended design/plan/schema to preserve stepping semantics and define comparator-target hash source across endpoint kinds.
- Review run IDs + triage outcomes:
  - N/A (alignment-only amendment; no contract-scope expansion beyond compatibility clarifications)
- Go/No-Go: GO
- Notes:
  - changes are compatibility clarifications and do not alter review-state feature scope or persistence schema version.

## 10. Execution Handoff Contract
1. Required read order:
   1) `docs/implementation/diff-tui-entity-review-state/schema-proposal.md`
   2) `docs/implementation/diff-tui-entity-review-state/design.md`
   3) `docs/implementation/diff-tui-entity-review-state/phase-task-plan.md`
2. Start at `H0`.
3. Execute phases in strict order using this plan as source of truth for scope, gates, review/triage policy, and Section 9 evidence updates.
