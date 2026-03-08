# Design: Diff TUI Commit Navigation

## Status
Locked

## 1. Purpose
Define a reloadable `sem diff --tui` commit-navigation mode that lets operators step backward/forward through commit-level semantic diffs from inside one TUI session.

## 2. Problem Statement
Current TUI flow is single-shot: one `DiffResult` is computed before launch and remains immutable for the life of the session. Reviewing a sequence of commits requires exiting and relaunching with a different `--commit` ref each time.

## 3. Goals
1. Support keyboard commit stepping (older/newer) without relaunching the TUI.
2. Keep existing entity-level navigation and detail rendering behavior.
3. Show commit context in the list/detail header: rev label (`HEAD~N` when derivable), short SHA, and subject.
4. Keep UI responsive during commit reload.
5. Preserve existing non-TUI and non-commit TUI contracts.

## 4. Non-Goals
1. No stacked-PR/branch-stack semantics (Graphite-style dependent diffs).
2. No changes to semantic diff classification (`added/modified/deleted/moved/renamed`).
3. No new persisted config/preferences.
4. No rewrite of parser/diff engine in `sem-core`.
5. No support for commit stepping in `--stdin` or two-file comparison modes.
6. No graph traversal beyond first-parent lineage in v1.

## 5. Current Baseline
1. `diff_command` computes one `DiffResult` and then enters TUI.
2. `run_tui` receives only an immutable `&DiffResult` and has no data-reload callback.
3. `AppState` stores entity rows and view state, but no source cursor or loading state.
4. `GitBridge` already supports commit-scoped file loading and log retrieval.

## 6. Key Decisions
1. Add commit stepping only when TUI is launched with explicit commit source (`--tui --commit <rev>`).
2. Commit stepping keys:
   - `[` => older commit (first parent of current commit)
   - `]` => newer commit (nearest descendant toward session head on first-parent chain)
3. Keep `Left/Right` bound to entity navigation in detail mode.
4. Commit stepping reloads semantic diff context in-place; no TUI restart.
5. TUI loop remains synchronous (`crossterm` loop); reload runs in worker thread(s) with `std::sync::mpsc` channel back into main loop.
6. Cancellation policy in v1: no hard cancellation of in-flight git/diff compute; use request generation IDs (`requestId` in schema) and apply only latest completion (`latest request wins`), dropping stale results.
7. Backpressure policy in v1: coalesce repeated step requests to at most one pending target while one worker is in flight.
8. When stepping is unavailable (stdin/two-file/staged/range mode), keys are inert and help text indicates disabled state.
9. Header includes command line plus commit metadata line with fallback text for unavailable metadata.
10. History walk semantics are first-parent linear history for deterministic navigation.
11. Dirty working tree does not affect commit stepping snapshots; commit-mode stepping reads commit trees only.
12. Merge commits compare against first parent only; behavior is deterministic and documented.
13. Newer-step resolution uses a frozen first-parent lineage cache captured at TUI startup (session-head to root). `[`/`]` are index moves in that cache when current commit is on lineage; if not on lineage, `revLabel=null` and `hasNewer=false` until a lineage commit is selected.
14. Status/error hints are transient: retained while loading, then replaced on next status event; boundary/unsupported hints clear on the next successful reload or next step-key attempt.

## 7. Contract / Interface Semantics
This feature is a CLI/TUI contract (not HTTP).

### 7.1 CLI Inputs
1. Existing `sem diff --tui --commit <rev>` remains valid.
2. No required new flag in v1.
3. Existing argument validation remains unchanged for incompatible options.

### 7.2 TUI Keyboard Contract Additions
1. `[` attempts to move to next older commit.
2. `]` attempts to move to next newer commit.
3. On boundary:
   - older boundary at root commit (no parent)
   - newer boundary at session head lineage ceiling
   - action is no-op with status hint.

### 7.3 Header Contract
Top header must show:
1. Invoked command.
2. Commit context line:
   - preferred: `<rev-label>  <short-sha>  <subject>`
   - fallback: `<short-sha>  <subject>`
   - unavailable path: `Commit navigation unavailable for current input mode`.
3. Double-space separation in examples is presentation guidance, not a strict parser contract.

Rev-label lock:
1. `rev-label` is computed against frozen session head SHA captured at TUI startup after resolving the initial commit ref.
2. If current commit is on frozen session-head first-parent chain, show `HEAD~N`.
3. If not on that lineage (detached/arbitrary ref), `rev-label` is omitted (null internally).

### 7.4 Behavioral Contract
1. Successful reload resets selected row to first selectable entity and clears detail scroll/hunk index.
2. If reloaded commit has no semantic changes, show deterministic empty-state list (no crash, no forced exit).
3. If loader fails (invalid ref, repo/history mutation, git errors), retain prior commit view and render non-fatal status message.
4. If user quits during in-flight reload, app exits immediately; worker result is ignored.

## 8. Service / Module Design
1. `commands/diff.rs`
   - Extract reusable diff-loading service that can load from explicit commit target at runtime.
   - Keep current static path unchanged for non-TUI outputs.
2. `tui/mod.rs`
   - Replace one-shot `run_tui(&DiffResult, ...)` path with session controller + worker-channel integration.
3. `tui/app.rs`
   - Add commit cursor state, capability flag, loading state, transient status message, and actions for step-older/step-newer.
4. `tui/render.rs`
   - Render commit metadata line and loading/status hints.
   - Update help/footer with commit keys and capability note.
5. `sem-core/git/bridge.rs`
   - Reuse existing commit retrieval and metadata, adding helper(s) only if needed for direct first-parent/child lineage resolution.

## 9. Error Semantics
1. Non-commit source mode + `[`/`]`: no-op, non-fatal status hint.
2. Commit reload failure: retain previous snapshot; show error hint.
3. Empty diff on target commit: show stable empty-state list panel.
4. Worker timeout/stall: surface status hint and allow retry keypress.
5. Repo history changed during session: failed target load is non-fatal and does not clear current view.

## 10. Migration Strategy
1. No breaking CLI contract changes.
2. Existing TUI sessions without commit source behave exactly as today.
3. Roll out behind source-mode gating first.

## 11. Test Strategy
1. App state tests:
   - commit cursor stepping boundaries (including root commit)
   - newer-step correctness across first-parent lineage (including merge commits)
   - rev-label derivation edges (session head, off-lineage commit, large `N`)
   - single-commit repository boundary behavior (`hasOlder=false`, `hasNewer=false`)
   - loading/state/status transitions
   - unsupported mode inert behavior
2. TUI loop tests:
   - async reload result application
   - stale worker result rejection (generation token)
   - rapid keypress coalescing (`[ [ [ ] ]` patterns)
   - quit during in-flight reload
3. Rendering tests:
   - commit metadata header line
   - unavailable navigation hint
   - loading indicator visibility
4. CLI behavior tests:
   - commit mode enters navigation-capable state
   - stdin/two-file/staged/range modes disable commit stepping
   - dirty working tree does not alter commit-mode semantics
5. Regression tests:
   - existing entity navigation and detail toggles remain unchanged.

## 12. Acceptance Criteria
1. Operator can step `[`/`]` across first-parent commit lineage in one TUI session when launched in commit mode.
2. Header displays commit short SHA and subject for current snapshot.
3. `HEAD~N` label is shown only when derivable from session-head lineage.
4. UI remains responsive while reloading commit snapshots.
5. Boundary, failure, and unsupported-mode states are non-fatal and deterministic.
6. Non-commit input modes preserve existing behavior and do not enable accidental commit stepping.

## 13. Constraints and Explicit User Preferences
1. Preserve navigation-centric TUI UX already in place.
2. Show commit identity context (`HEAD~N` when derivable, short hash, message) at top.
3. Avoid requiring TUI relaunch for commit-to-commit review.
