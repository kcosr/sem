# Phase Task Plan: Diff TUI Commit Navigation

## Status
Locked

## 1. Scope
Deliver in-TUI commit stepping for `sem diff --tui --commit <rev>` with asynchronous reload, commit metadata header context, deterministic boundary/error behavior, and complete execution evidence.

## 2. Global Rules
1. Execute phases in strict order (`H0` -> `H1` -> `H2` -> `H3`).
2. Do not expand scope to stacked-PR workflows.
3. Preserve existing non-TUI behavior and existing TUI entity/detail navigation.
4. Keep commit stepping disabled for unsupported source modes (`--stdin`, two-file, staged, range).
5. Use first-parent history for deterministic traversal.
6. Keep reload non-fatal: prior view remains on load failure.
7. Use independent reviews per phase when required by execution stream policy.
8. Every review finding must be triaged as `accept`, `defer`, or `reject`.
9. Section 9 evidence entries are mandatory at phase close.

## 3. Phases, Deliverables, Acceptance Criteria

### H0: Contract + Source-Mode Lock
Deliverables:
1. Final lock on stepping semantics and keybindings (`[`/`]`).
2. Source-mode compatibility matrix and unsupported-mode behavior lock.
3. Async/reload contract lock: request generation ID (`requestId`/`appliedRequestId`) semantics, coalesced pending request, stale-result rejection.
4. Rev-label derivation lock (`HEAD~N` relative to frozen session head when derivable).

Acceptance Criteria:
1. Contracts are explicit for boundaries, unsupported modes, and failure behavior.
2. No ambiguity around detached/arbitrary refs, dirty working tree semantics, or merge first-parent behavior.
3. H0 is contract verification/triage closure for locked docs, not feature implementation authoring.

Gate:
- `GO` only if design/schema docs are internally consistent and review-triaged.

### H1: Loader Refactor + Commit Cursor Model
Deliverables:
1. Refactor data loading to support runtime commit reload requests.
2. Add commit cursor/session model (`sha`, `subject`, `revLabel`, `hasOlder`, `hasNewer`).
3. Introduce worker-channel integration for non-blocking reload in synchronous TUI loop.
4. Add base tests for cursor transitions and unsupported-mode inert behavior.

Acceptance Criteria:
1. Existing one-shot flows remain deterministic for non-TUI outputs.
2. TUI can request and receive commit snapshot reloads without restart.

Gate:
- `GO` only if compile/tests pass and regression risk is controlled.

### H2: TUI Interactive Commit Stepping
Deliverables:
1. Wire `[`/`]` actions to async reload pipeline.
2. Add loading and status/error hints in UI.
3. Render commit metadata line in headers.
4. Ensure detail/list states reset deterministically on successful reload.
5. Add tests for boundary no-op, root-commit handling, and latest-request-wins behavior.

Acceptance Criteria:
1. Operator can step older/newer commits in one session.
2. UI remains responsive during reload.
3. Failure keeps previous snapshot visible and stable.

Gate:
- `GO` only if keyboard/visual contracts are verified and no fatal regressions exist.

### H3: Hardening, Docs, and Release Readiness
Deliverables:
1. Complete test matrix coverage including rapid keypress coalescing and quit-during-load behavior.
2. Add targeted performance evidence with explicit threshold notes.
3. Update user docs (`README.md`, `crates/README.md`) with new keys and scope limits.
4. Add changelog milestone entry.
5. If `docs/implementation/implementation-plan.md` exists, update milestone status; if absent/retired, record equivalent milestone status under this topic plan Section 9 with rationale.

Acceptance Criteria:
1. Tests pass for affected crates.
2. Documentation reflects runtime behavior and constraints.
3. Section 9 evidence complete for all phases.

Gate:
- `GO` only if all evidence is captured and unresolved findings are explicitly deferred/rejected.

## 4. Verification Matrix
| Area | Verification Type | Command / Evidence |
|---|---|---|
| contract lock clarity | doc review | `design.md` + `schema-proposal.md` consistency check |
| commit loader behavior | unit/integration | `cargo test -p sem-cli` targeted loader tests |
| app state transitions | unit tests | commit cursor + mode reducer tests |
| async reload responsiveness | integration/manual | interactive run evidence + event loop responsiveness notes |
| rapid step coalescing | unit/integration | simulated repeated `[`/`]` key sequences |
| stale result suppression | unit/integration | generation token tests |
| boundary handling | unit/manual | root commit and newer/older boundary no-op assertions |
| newer-step correctness | unit/integration | first-parent lineage stepping assertions (including merges) |
| rev-label edge cases | unit tests | `HEAD~N` derivation for head/off-lineage/large-`N` cases |
| quit during in-flight reload | unit/integration | quit action + worker result ignore assertions |
| header metadata rendering | render test/manual | header line includes SHA + subject + optional revLabel |
| unsupported mode behavior | unit/manual | stdin/two-file/staged/range inert keys |
| dirty working tree behavior | manual/integration | commit mode ignores worktree changes |
| semantic diff correctness | regression tests | `cargo test -p sem-core` |
| release docs consistency | manual review | README/changelog updates |

## 5. Milestone Commit Gate
1. Require one milestone commit per phase (`H0`..`H3`).
2. Commit message template:
   - `feat(sem): H0 commit-nav contracts lock`
   - `feat(sem): H1 commit loader + cursor model`
   - `feat(sem): H2 tui commit stepping UX`
   - `feat(sem): H3 commit-nav hardening + docs`
3. No phase close without Section 9 evidence entry.

## 6. Risks and Mitigations
1. Risk: UI stutter on reload for large commits.
- Mitigation: worker thread reload + loading indicator + coalesced pending request.
2. Risk: keybinding conflicts with existing navigation.
- Mitigation: keep `Left/Right` for entities; isolate commit stepping to `[`/`]`.
3. Risk: first-parent semantics may surprise users on merge-heavy history.
- Mitigation: explicit docs/help text; deterministic rule lock.
4. Risk: stale async results overwriting newer requests.
- Mitigation: request generation token; apply only latest completion.
5. Risk: external history mutation during session.
- Mitigation: retain previous snapshot on load failure and show non-fatal status.

## 7. Deferred Items (Explicit)
1. Stacked-branch/stacked-PR semantics.
2. Alternate traversal graphs beyond first-parent.
3. Persisted “last viewed commit” state.
4. Hard cancellation of in-flight diff computation (v1 uses stale-result suppression only).
5. Merge-commit specific visual badge in header.
6. Accessibility enhancements beyond current TUI baseline.

## 8. Review Findings Triage Ledger (Authoring Stage)

### Review Run A (generic-gemini)
- Run ID: `r_20260308154816062_dd7505d0`
- Findings summary:
  - initial `hasOlder/hasNewer` derivation ambiguity
  - dirty working tree behavior ambiguity for commit mode
  - rapid keypress async backpressure risk
  - external history mutation risk
  - edge-case test gaps (single commit/root, rapid stepping, invalidated target)
- Triage outcomes:
  - `accept`:
    - locked initial boundary derivation and root boundary rules
    - locked dirty-working-tree semantics (commit trees only)
    - added coalesced pending request + stale-result suppression contract
    - added history-mutation non-fatal failure contract
    - expanded test strategy/matrix for root and rapid stepping
  - `defer`:
    - merge-commit special visual indicator (tracked in Section 7)
  - `reject`:
    - none

### Review Run B (generic-pi)
- Run ID: `r_20260308154852348_2d5e6a7c`
- Findings summary:
  - vague optional extension and rev-label computation ambiguity
  - async architecture/cancellation semantics under-specified
  - root/quit-during-load/rapid-step test gaps
  - potential confusion around schema intent and summary field meanings
  - suggested read-order preference change
- Triage outcomes:
  - `accept`:
    - removed optional-extension ambiguity from design scope
    - locked rev-label semantics against frozen session head lineage
    - locked explicit sync-loop + worker-thread architecture
    - locked cancellation/backpressure policy (generation token + coalesced pending request)
    - added root boundary, rapid stepping, and quit-during-load test coverage targets
    - clarified schema intent and `fileCount` vs `total` meaning
  - `defer`:
    - strict numeric perf SLO threshold definition until H3 representative measurement
    - hard cancellation tokens for in-flight compute (tracked as deferred)
  - `reject`:
    - changing required read order from schema-first (kept aligned with stream skill default compact prompt)

## 9. Operator Checklist and Evidence Log Schema (Mandatory)

### 9.1 Operator Checklist Per Phase
1. Confirm prior phase is `GO`.
2. Execute only current phase deliverables.
3. Run required verification and capture results.
4. Run required independent review(s) and triage all findings.
5. Record Section 9 evidence before phase close.
6. Mark phase `GO` or `NO-GO` with rationale.

### 9.2 Evidence Log Schema
For each phase `H0..H3`, record:
1. Completion date (`YYYY-MM-DD`).
2. Commit hash(es).
3. Acceptance evidence (commands + result summaries, manual checks).
4. Review run IDs + triage outcomes.
5. Go/No-Go decision.

Template:

```md
### Hx Evidence
- Completion date: YYYY-MM-DD
- Commit hash(es): <hash list>
- Acceptance evidence:
  - `<command>` => `<result summary>`
  - manual: `<validated behavior>`
- Review run IDs + triage outcomes:
  - `<run-id>`: `accept|defer|reject` summary
- Go/No-Go: GO | NO-GO
- Notes: <optional>
```

### 9.3 H0 Evidence
- Completion date: 2026-03-08
- Commit hash(es): `ca3e726`
- Acceptance evidence:
  - doc review: `schema-proposal.md` + `design.md` + `phase-task-plan.md` consistency pass after lock clarifications (request schema, requestId terminology, lineage/newer-step policy, transient status lifetime, explicit H0 verification wording).
  - `npm run lint` => `NO-GO` for global TS workspace baseline (missing module/type dependencies in current environment; unrelated to H0 doc-only scope).
  - `npm test` => `NO-GO` for global JS workspace baseline (`vitest` binary unavailable in current environment; unrelated to H0 doc-only scope).
  - manual: confirmed H0 gate condition (contracts internally consistent + review-triaged) is satisfied.
- Review run IDs + triage outcomes:
  - `r_20260308162006991_79da9925` (`generic-gemini`): `accept` requestId terminology lock and lineage/test clarity updates; `defer` worker-disconnect/memory hardening details to H3; `reject` none.
  - `r_20260308162039986_3da9384a` (`generic-pi`): `accept` request schema and newer-step/rev-label/status-lifetime clarifications; `defer` numeric stall timeout/perf threshold specifics to H3; `reject` none.
- Go/No-Go: GO
- Notes:
  - both review runs completed via session stream terminal event `result.completed`.
  - global `npm` gate failures are pre-existing environment/workspace issues and were logged for traceability.

### 9.4 Authoring-Stage Evidence (Spec Plan)
- Completion date: 2026-03-08
- Commit hash(es): N/A (planning stream)
- Acceptance evidence:
  - drafted required artifacts under `docs/implementation/diff-tui-commit-navigation/`
  - executed 2 independent review runs with stream-confirmed terminal events
  - triaged all findings and applied accepted doc changes
- Review run IDs + triage outcomes:
  - `r_20260308154816062_dd7505d0`: accepted contract/test clarifications; deferred merge-visual indicator
  - `r_20260308154852348_2d5e6a7c`: accepted async/rev-label/test clarifications; deferred perf SLO + hard cancellation; rejected read-order change
- Go/No-Go: GO
- Notes:
  - both review runs completed via session stream `result.completed`; no fallback path required.

## 10. Execution Handoff Contract
1. Required read order:
   1) `docs/implementation/diff-tui-commit-navigation/schema-proposal.md`
   2) `docs/implementation/diff-tui-commit-navigation/design.md`
   3) `docs/implementation/diff-tui-commit-navigation/phase-task-plan.md`
2. Start point: `H0` only.
3. Boundaries and semantic-preservation constraints:
   - preserve existing non-TUI output contracts
   - preserve existing TUI entity/detail navigation semantics
   - do not expand to stacked-PR semantics
   - first-parent history only for v1
   - keep unsupported source modes inert for commit stepping
4. Review command policy:
   - use `agent-runner-review`
   - no timeout/reasoning-effort CLI overrides
   - completion must be confirmed from session stream terminal events (`result.completed|result.failed`)
5. Completion requirements:
   - update docs/reference only if behavior is stabilized and cross-cutting
   - update `docs/implementation/implementation-plan.md` milestone status if file exists; otherwise record equivalent milestone status under this topic plan Section 9 notes
   - add `CHANGELOG.md` milestone entry
   - complete Section 9 evidence for all phases
   - publish final phase summary with triage ledger closure

## 11. Default Compact Handoff Prompt
Use $agent-runner-spec-execution and $agent-runner-review.

Topic slug: diff-tui-commit-navigation.

Read:
1) docs/implementation/diff-tui-commit-navigation/schema-proposal.md
2) docs/implementation/diff-tui-commit-navigation/design.md
3) docs/implementation/diff-tui-commit-navigation/phase-task-plan.md

Execute all phases declared in phase-task-plan.md in strict order, using the plan as source of truth for scope, gates, review/triage policy, and Section 9 evidence updates.
