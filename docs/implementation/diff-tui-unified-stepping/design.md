# Design: Diff TUI Unified Stepping

## Status
Locked

## 1. Purpose
Unify commit stepping, range stepping, and local synthetic states (`INDEX`, `WORKING`) under one TUI stepping model with two comparison modes (`pairwise`, `cumulative`).

## 2. Problem Statement
Current behavior splits mental models:
1. `--commit` stepping is commit-local (`parent -> commit`).
2. `--from/--to` is range diff but lacks integrated stepping semantics.
3. Local unstaged/staged state is separate from commit stepping.

Operators want one consistent path model and mode toggling while keeping existing stepping intuition.

## 3. Goals
1. Define a single ordered-step model across commit refs plus synthetic endpoints.
2. Support `pairwise` and `cumulative` stepping modes in TUI with a toggle key (`m`).
3. Preserve current pairwise commit semantics (`previous -> current`) for compatibility.
4. Support `INDEX` and `WORKING` as live synthetic endpoints.
5. Improve range headers to show `from` and `to` comparator metadata clearly.

## 4. Non-Goals
1. No branch-graph/stacked-PR orchestration.
2. No persisted mode preference/config in v1.
3. No dynamic in-session bound editing in v1 (`[]` moves cursor only; bounds fixed at launch).
4. No API shape changes for existing `--format json` output.

## 5. Current Baseline
1. Commit stepping is enabled only in `--tui --commit <rev>` mode.
2. `[`/`]` are inert in `--from/--to`, `--staged`, `--stdin`, and two-file modes.
3. Header supports range context (`from` left, `to` right) for `--from/--to`.
4. Disabled stepping no longer triggers loading state.

## 6. Key Decisions
1. Canonical model is `diff(endpoint_from, endpoint_to)` where endpoint is:
   - commit ref
   - `INDEX`
   - `WORKING`
2. `--commit C` remains user-facing and acts as sugar for:
   - `--from C~1 --to C` with default `pairwise` behavior.
3. Canonical mixed endpoint ordering is deterministic:
   - commits in chronological path order from `from` to `to`, then `INDEX`, then `WORKING` when included.
4. Cursor index semantics:
   - index `0` is oldest/leftmost endpoint in active path.
   - `stepOlder` decrements index; `stepNewer` increments index.
5. Mode definitions:
   - `pairwise`: compare `S[i-1] -> S[i]` at cursor `i`.
   - `cumulative`: compare `base -> S[i]`.
6. Single-endpoint path behavior:
   - both modes render stable no-op comparison (`from == to`) with no crash.
7. Base selection for `cumulative`:
   - explicit `--from/--to`: base is fixed `from`.
   - implicit/no explicit range: base is anchored to current cursor when cumulative is toggled on.
   - each toggle-on re-anchors base to then-current cursor.
8. Startup defaults:
   - explicit `--from/--to`: default mode `cumulative`, cursor at `to` (whole range shown initially).
   - `--commit` or implicit/latest mode: default mode `pairwise`, cursor at newest endpoint.
9. Range-mode header comparator behavior:
   - cumulative: comparator `from` is current base and comparator `to` is cursor endpoint.
   - pairwise: comparator endpoints are effective `S[i-1]` and `S[i]`.
   - "left/right" refers to comparator labels, not side-by-side content panes.
10. `INDEX`/`WORKING` are live (re-evaluated per step request), not frozen snapshots.
11. Step-mode CLI flag and key interaction:
   - `--step-mode` sets startup mode only.
   - `m` remains available to toggle mode during session.
12. Backpressure/cancellation contract in v1:
   - latest-request-wins, coalesced single pending request, stale-result rejection.

## 7. Contract / Interface Semantics
This is CLI/TUI contract design (not HTTP).

### 7.1 CLI Surface
1. Keep existing flags.
2. Add step-mode flag for explicit startup control:
   - `--step-mode pairwise|cumulative`
3. Add pseudo-endpoint support in `--from/--to` for `INDEX` and `WORKING`.
4. Symbolic refs (`HEAD~1`, branch names) are resolved to immutable endpoint IDs (SHA-based) for runtime state.

### 7.2 TUI Keyboard
1. `[` older/left in ordered path.
2. `]` newer/right in ordered path.
3. `m` toggles `pairwise` <-> `cumulative`.

### 7.3 Mode Indicator Contract
1. Footer must include explicit mode token: `mode: pairwise` or `mode: cumulative`.
2. Header comparator line always reflects effective comparison endpoints for current mode/cursor.

### 7.4 Endpoint Resolution
1. Commits resolve through git revision resolution.
2. `INDEX` resolves to staged snapshot.
3. `WORKING` resolves to unstaged+untracked snapshot.
4. For live semantics, synthetic endpoints are recomputed each request.
5. Outside valid git context, synthetic endpoint resolution fails non-fatally via normal load-failure semantics.

## 8. Service / Module Design
1. Introduce unified endpoint + path planner in `commands/diff.rs`.
2. Replace commit-only cursor with generic step cursor (endpoint id, index, boundaries).
3. Extend reload request/response with mode and endpoint ids.
4. Keep async worker + latest-request-wins handling with coalesced pending request.
5. Update renderer for dual-endpoint comparator headers and mode indicator.

## 9. Error Semantics
1. Invalid endpoint ref: non-fatal load failure with retained prior snapshot.
2. Boundary step: no-op with deterministic status.
3. Endpoint disappeared/changed mid-session (live local state): non-fatal load failure, retry allowed.
4. Unsupported source mode remains non-fatal and deterministic.

## 10. Migration Strategy
1. Backward-compatible CLI:
   - `--commit` continues to work.
2. Preserve existing pairwise commit intuition.
3. Introduce new mode behavior incrementally behind explicit phase gates.

## 11. Test Strategy
1. Endpoint path planning tests (`commit`, `range`, `INDEX`, `WORKING`).
2. Pairwise/cumulative comparator endpoint selection tests.
3. Mode toggle tests (including base re-anchor on toggle-on).
4. Live synthetic endpoint refresh tests:
   - unchanged state
   - state changed between requests
   - state transitions empty <-> populated
5. Render tests for dual-endpoint headers and mode indicator.
6. End-to-end keypress -> reload -> render flow test.
7. Regression test proving `--format json` external shape remains unchanged.

## 12. Acceptance Criteria
1. Operator can step with `[`/`]` across unified endpoint path.
2. `m` toggles step mode and redraws according to mode semantics.
3. Explicit range launches cumulative by default with full-range initial view.
4. Implicit and `--commit` launches pairwise by default.
5. `INDEX` and `WORKING` are supported as endpoints and participate in stepping.
6. Header/footer mode/comparator labels are deterministic and contract-compliant.

## 13. Constraints and Explicit User Preferences
1. Keep pairwise commit behavior consistent with current implementation (`A->B` at `B`).
2. Add cumulative mode as explicit alternate comparison mode.
3. Keep v1 simple: no dynamic bound editing via additional bracket variants.
4. Use `m` as mode toggle key.
