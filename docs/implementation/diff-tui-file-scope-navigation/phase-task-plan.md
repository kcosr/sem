# Phase Task Plan: Diff TUI File-Scope Navigation

## Status
Locked

## 1. Scope
Add selectable file rows and scope-aware diff rendering so file selection supports hunk and full-file views via existing `e` toggle semantics.

## 2. Global Rules
1. Execute phases in strict order: `H0 -> H1 -> H2 -> H3`.
2. Preserve existing entity-level behavior.
3. Keep topic file-only (no parent/class scope level).
4. No CLI surface expansion.
5. Require two independent authoring reviews (Gemini + PI).
6. Triage every finding as `accept`, `defer`, or `reject`.
7. Section 9 evidence updates are mandatory.

## 3. Phases, Deliverables, Acceptance

### H0: Contract Lock
Deliverables:
1. Lock row-scope model (`File`, `Entity`).
2. Lock file-row rendering/ordering/visual distinction rules.
3. Lock scope-aware semantics for `e`, `n/p`, and detail title.
4. Lock file snapshot source and TUI payload contract (`FileChange` -> snapshot map).
5. Lock filter-selection behavior (file row hidden when no visible children; deterministic fallback selection).

Acceptance:
1. No ambiguity in file-vs-entity selection behavior.
2. No ambiguity in file hunk/full rendering semantics.
3. No ambiguity in snapshot sourcing.

Gate:
- GO only when authoring reviews complete and triaged.

### H1: Data Plumbing + Scope Row Model
Deliverables:
1. Extend diff output phase and `run_tui` signature to pass file snapshots.
2. Implement scope row model and list building (`File` + `Entity` rows).
3. Implement file-row aggregate delta fields (sum of child added/removed lines).
4. Keep selection/filter/review state deterministic across row kinds.
5. Add app-state + plumbing tests.

Acceptance:
1. File rows appear and are selectable.
2. Existing entity rows still render/navigate correctly.
3. Aggregate file delta values match child entity sums.

Gate:
- GO only with passing `sem-cli` tests for row/scope plumbing.

### H2: File-Scope Detail Rendering
Deliverables:
1. Implement file-scope hunk renderer (grouped file hunks, line-ascending order).
2. Implement file-scope full renderer (whole-file expansion).
3. Wire scope-aware `e` and `n/p` behavior.
4. Add deterministic handling for added/deleted/binary/missing-content file cases.
5. Add renderer/navigation tests for file scope.

Acceptance:
1. File + hunk mode shows file hunks and supports navigation.
2. File + entity mode shows full-file diff.
3. Entity scope behavior remains unchanged.
4. Edge cases are non-fatal and deterministic.

Gate:
- GO only with passing renderer and navigation tests.

### H3: UX/Help/Docs/Hardening
Deliverables:
1. Update help/footer text to clarify scope-aware `e` behavior (`hunk/full scope`).
2. Update README docs and changelog.
3. Add hardening tests for:
   - filter-selection fallback across mixed row kinds,
   - row-boundary navigation,
   - edge file states,
   - regression of entity behavior.
4. Add performance smoke evidence for large multi-file diffs.
5. Complete Section 9 evidence entries.

Acceptance:
1. Docs and help match runtime behavior.
2. Missing-data and edge-file paths are non-fatal and deterministic.
3. Performance smoke evidence is recorded.

Gate:
- GO only with docs + tests + evidence complete.

## 4. Verification Matrix
| Area | Verification Type | Command / Evidence |
|---|---|---|
| contract lock | doc review | design/schema/plan consistency pass |
| row model | app test | file and entity rows both selectable |
| ordering | app/render test | file row precedes its entity rows |
| visual distinction | render test | file rows rendered with locked file styling/indent contract |
| aggregate deltas | app/render test | file row `+/-` equals sum of child entity deltas |
| data plumbing | unit/integration test | snapshot map passed from `FileChange` to TUI |
| file-hunk mode | render test | selected file shows grouped file hunks |
| file-full mode | render test | selected file shows whole-file diff |
| hunk ordering | render test | file-scope hunks are line-ascending |
| scope toggle | app/render test | `e` toggles hunk/full for both row kinds |
| hunk navigation | app/render test | `n/p` navigates file-scope hunks |
| filter fallback | app/render test | hidden selected row reselects deterministic visible row |
| file row visibility | app/render test | file row hidden when no visible child rows remain |
| boundary navigation | app test | deterministic top/bottom behavior across mixed row kinds |
| edge file states | safety tests | added/deleted/binary/missing snapshot handling |
| entity regression | regression test | entity rows keep existing behavior |
| performance smoke | evidence command | large diff startup/memory baseline captured |
| full regression | suite | `cargo test -p sem-cli && cargo test -p sem-core` |

## 5. Milestone Commit Gate
1. One milestone commit per phase.
2. Commit template:
   - `feat(sem): H0 file-scope contract lock`
   - `feat(sem): H1 file scope row model + data plumbing`
   - `feat(sem): H2 file-scope diff rendering`
   - `feat(sem): H3 file-scope docs + hardening`

## 6. Risks and Mitigations
1. Risk: adding file rows can break selection semantics.
- Mitigation: strict row-kind tests and explicit ordering assertions.
2. Risk: file-level rendering diverges from entity rendering behavior.
- Mitigation: shared render adapters and parity tests.
3. Risk: missing or non-UTF8 file content in diff inputs.
- Mitigation: explicit placeholder and non-fatal error semantics.
4. Risk: memory/startup cost from full snapshot preload.
- Mitigation: baseline performance smoke checks; deeper lazy loading/caching deferred.

## 7. Deferred Items
1. Parent/class/container selection layer.
2. Multi-level tree expansion controls.
3. Scope-specific key customization beyond current bindings.
4. Lazy snapshot loading/caching beyond baseline safeguards.

## 8. Review Findings Triage Ledger (Authoring Stage)

### Review Run A (generic-gemini)
- Run ID: `r_20260310021750743_35a3693b`
- Findings summary:
  - terminology overload around `entity` mode name at file scope
  - missing clarity for visual distinction, edge file states, snapshot source
  - memory/performance concerns from full snapshot preload
  - test gaps for edge files and large-diff behavior
- Triage:
  - `accept`:
    - locked visual distinction rules for file rows,
    - locked snapshot source relationship (`FileChange` -> TUI snapshot map),
    - added explicit edge file state behavior and verification rows,
    - added performance smoke evidence requirement.
  - `defer`:
    - lazy loading/caching optimization policy to future hardening scope.
  - `reject`:
    - renaming `e` mode token from `entity` to `full`; token remains for compatibility, with clarified help text semantics.

### Review Run B (generic-pi)
- Run ID: `r_20260310021828435_e9f0b9f2`
- Findings summary:
  - ambiguity around synthetic rows, aggregate delta meaning, and scope contract symmetry
  - missing requirements for filter-selection behavior and file-row visibility with hidden children
  - risk emphasis on row-index stability and snapshot memory footprint
  - test gaps on aggregate delta correctness and boundary/navigation behavior
- Triage:
  - `accept`:
    - clarified synthetic row definition and aggregate delta formula,
    - added symmetric schema behavior blocks for file + entity scope,
    - locked filter-selection/file-row visibility behavior,
    - expanded verification matrix for aggregate delta, row boundaries, and contract-violation coverage,
    - documented row indices as ephemeral UI positions (non-persisted).
  - `defer`:
    - advanced memory optimization (lazy snapshot loading) to future scope.
  - `reject`:
    - none.

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
  - `r_20260310021750743_35a3693b`: accept + defer + reject
  - `r_20260310021828435_e9f0b9f2`: accept + defer, no rejects
- Go/No-Go: GO
- Notes:
  - both runs completed via stream terminal event `result.completed`; no fallback retry required.

## 10. Execution Handoff Contract
1. Required read order:
   1) `docs/implementation/diff-tui-file-scope-navigation/schema-proposal.md`
   2) `docs/implementation/diff-tui-file-scope-navigation/design.md`
   3) `docs/implementation/diff-tui-file-scope-navigation/phase-task-plan.md`
2. Start point:
   - start at `H0` only.
3. Boundaries and semantic-preservation constraints:
   - preserve existing entity behavior,
   - keep file-only scope expansion,
   - no parent/class level in this topic,
   - no CLI flag additions.
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

Topic slug: `diff-tui-file-scope-navigation`.

Read:
1) `docs/implementation/diff-tui-file-scope-navigation/schema-proposal.md`
2) `docs/implementation/diff-tui-file-scope-navigation/design.md`
3) `docs/implementation/diff-tui-file-scope-navigation/phase-task-plan.md`

Execute all phases declared in `phase-task-plan.md` in strict order, using the plan as source of truth for scope, gates, review/triage policy, and Section 9 evidence updates.
