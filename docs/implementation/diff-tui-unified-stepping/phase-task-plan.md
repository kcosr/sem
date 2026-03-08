# Phase Task Plan: Diff TUI Unified Stepping

## Status
Locked

## 1. Scope
Deliver a unified TUI stepping system over commit and synthetic endpoints with selectable comparison modes (`pairwise`, `cumulative`) and deterministic defaults by invocation type.

## 2. Global Rules
1. Execute phases in strict order: `H0 -> H1 -> H2 -> H3`.
2. Preserve existing pairwise commit meaning at cursor (`previous -> current`).
3. Keep behavior non-fatal on load failures (retain previous snapshot).
4. Treat `INDEX`/`WORKING` as live endpoints (refetch each request).
5. Run 2 independent external reviews during authoring stage (Gemini + PI).
6. Triage every review finding as `accept`, `defer`, or `reject`.
7. Section 9 evidence updates are mandatory.

## 3. Phases, Deliverables, Acceptance

### H0: Contract Lock
Deliverables:
1. Lock endpoint model, mode semantics, startup defaults, and keybindings.
2. Lock `--commit` sugar behavior and `--from/--to` symmetry including pseudo-endpoints.
3. Lock cumulative base/anchor semantics.
4. Lock mode-indicator/header comparator contract.

Acceptance:
1. No ambiguity on pairwise orientation at cursor.
2. No ambiguity on cumulative base selection and re-anchor behavior.
3. No ambiguity on canonical mixed endpoint ordering and index direction.
4. No ambiguity on single-endpoint path behavior.

Gate:
- GO only when design/schema/task-plan are internally consistent and authoring-stage review triage (Section 8 + 9.3) is complete.

### H1: Endpoint + Cursor Foundation
Deliverables:
1. Endpoint type + path planner implementation.
2. Generic step cursor model replacing commit-only assumptions.
3. Loader support for commit/index/working endpoints.
4. Initial unit tests for endpoint resolution/path generation.

Acceptance:
1. Existing compile/test baseline remains green.
2. Reload pipeline can target unified endpoint IDs.

Gate:
- GO only on passing crate tests and no contract regressions.

### H2: Mode Toggle + Rendering
Deliverables:
1. Add `pairwise/cumulative` runtime mode state and `m` toggle.
2. Mode-specific comparator endpoint selection.
3. Header updates:
   - cumulative comparator labels use base/cursor endpoints.
   - pairwise comparator labels use previous/current endpoints.
4. Add mode and header rendering tests.

Acceptance:
1. Toggle redraw is deterministic and immediate.
2. Cursor stepping semantics match locked contract.

Gate:
- GO only if mode behavior verified by tests/manual checks.

### H3: Defaults, Hardening, Docs
Deliverables:
1. Startup defaults by invocation:
   - explicit range => cumulative
   - implicit/latest and `--commit` => pairwise
2. Add `--step-mode` explicit override.
3. Update user docs and changelog.
4. Hardening tests around live endpoint updates.

Acceptance:
1. Defaults behave as locked.
2. Docs match runtime behavior.
3. Section 9 evidence complete.

Gate:
- GO only with tests + docs + evidence complete.

## 4. Verification Matrix
| Area | Verification | Command / Evidence |
|---|---|---|
| contract consistency | doc review | design/schema/task-plan cross-check |
| endpoint planning | unit tests | `cargo test -p sem-cli` (planner tests) |
| live synthetic endpoints | integration/unit | repeated requests with changed staging/working |
| pairwise orientation | unit tests | cursor endpoint selection assertions |
| cumulative anchor behavior | unit tests | base selection/re-anchor assertions |
| base endpoint invariants | unit tests | `baseEndpointId` null/ignored semantics assertions by mode |
| mode toggle behavior | app/render tests | `m` toggle state + comparator header |
| startup defaults | CLI tests | explicit range vs implicit vs commit invocation |
| initial startup contract | unit/integration | initial mode/cursor/comparator assertions before first step |
| single-endpoint edge | unit/integration | one-endpoint path semantics |
| full interaction loop | integration test | keypress -> reload -> render path |
| stale-result rejection | unit/integration | monotonic request/applied request ordering assertions |
| live endpoint transitions | integration/unit | empty <-> populated INDEX/WORKING transitions |
| json compatibility | regression test | `--format json` golden/snapshot checks |
| regression safety | full tests | `cargo test -p sem-cli && cargo test -p sem-core` |

## 5. Milestone Commit Gate
1. One milestone commit per phase.
2. Commit template:
   - `feat(sem): H0 unified-step contracts lock`
   - `feat(sem): H1 endpoint cursor foundation`
   - `feat(sem): H2 mode toggle + rendering`
   - `feat(sem): H3 defaults + docs + hardening`

## 6. Risks and Mitigations
1. Risk: mode semantics confusion.
- Mitigation: explicit header comparator endpoints + footer mode indicator.
2. Risk: live local-state churn causes noisy diffs.
- Mitigation: deterministic non-fatal failures and explicit status hints.
3. Risk: backward behavior regression for commit stepping.
- Mitigation: lock pairwise orientation and add compatibility tests.
4. Risk: large cumulative ranges may be slower.
- Mitigation: phase-gated hardening and perf observation in H3.

## 7. Deferred Items
1. Dynamic in-session bound editing via additional bracket variants.
2. Persisted mode preference.
3. Graph/stack-aware traversal beyond linear ordered path.
4. Snapshot-freeze option for local synthetic endpoints.

## 8. Review Findings Triage Ledger (Authoring Stage)

### Review Run A (generic-gemini)
- Run ID: `r_20260308180008389_eb14fd00`
- Findings summary:
  - empty synthetic endpoint semantics unspecified
  - lower-bound pairwise ambiguity at `i=0`
  - symbolic-ref/internal endpoint-id handling ambiguity
  - null cumulative base fallback ambiguity
  - live endpoint performance/churn risk note
  - test gap for empty/populated synthetic transitions
- Triage:
  - `accept`:
    - locked empty synthetic endpoint behavior and single-endpoint semantics in design
    - locked pairwise boundary semantics and index direction
    - locked symbolic ref resolution to SHA endpoint IDs with display ref separate
    - locked null cumulative base behavior (toggle-on re-anchor)
    - expanded test strategy/matrix for synthetic empty<->populated transitions
  - `defer`:
    - stronger perf SLO/caching strategy for live endpoints to H3 hardening
  - `reject`:
    - none

### Review Run B (generic-pi)
- Run ID: `r_20260308180042404_23f0a5a6`
- Findings summary:
  - cumulative base re-anchor ambiguity
  - mixed endpoint ordering/index direction ambiguity
  - header-side wording ambiguity
  - missing mode-indicator contract
  - single-endpoint path behavior missing
  - `--step-mode` and `m` interaction ambiguity
  - missing edge-case/full-loop/json-compat test coverage detail
  - live-performance/churn risks highlighted
- Triage:
  - `accept`:
    - locked re-anchor semantics
    - locked canonical ordering and index semantics
    - clarified header comparator terminology
    - locked footer mode indicator requirement
    - locked single-endpoint behavior
    - locked `--step-mode` as startup default (not toggle lock)
    - expanded verification matrix and test strategy for edge/full-loop/json compatibility
    - updated schema summary structure and endpoint/request examples
  - `defer`:
    - optional staleness indicator + advanced performance policy to H3
  - `reject`:
    - none

## 9. Operator Checklist and Evidence Log Schema

### 9.1 Checklist Per Phase
1. Validate prior phase GO.
2. Execute only current phase deliverables.
3. Run verification commands and capture outcomes.
4. Run required reviews and triage all findings.
5. Record Section 9 evidence block before phase close.

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
  - drafted required artifacts:
    - `docs/implementation/diff-tui-unified-stepping/design.md`
    - `docs/implementation/diff-tui-unified-stepping/phase-task-plan.md`
    - `docs/implementation/diff-tui-unified-stepping/schema-proposal.md`
  - executed 2 independent review runs via `agent-runner-review` policy
  - triaged all findings and applied accepted clarifications
- Review run IDs + triage outcomes:
  - `r_20260308180008389_eb14fd00`: accept+defer, no rejects
  - `r_20260308180042404_23f0a5a6`: accept+defer, no rejects
- Go/No-Go: GO
- Notes:
  - review completion confirmed from live session stream terminal events (`result.completed`).

## 10. Execution Handoff Contract
1. Required read order:
   1) `docs/implementation/diff-tui-unified-stepping/schema-proposal.md`
   2) `docs/implementation/diff-tui-unified-stepping/design.md`
   3) `docs/implementation/diff-tui-unified-stepping/phase-task-plan.md`
2. Start at `H0`.
3. Execute phases in strict order using this plan as source of truth for scope, gates, review/triage policy, and Section 9 evidence updates.
4. Completion requirements:
   - update docs/user references if behavior stabilizes
   - changelog milestone entry
   - Section 9 evidence complete for all phases
