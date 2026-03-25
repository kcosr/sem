# Design: Diff TUI File-Scope Navigation

## Status
Locked

## 1. Purpose
Allow file names to be selectable navigation targets in TUI so operators can inspect diffs at file scope, not only entity scope.

## 2. Problem Statement
Current TUI selection model is entity-only. File headers are visual grouping labels but cannot be selected. This prevents file-level exploration without leaving semantic mode, especially when operators want:
1. all hunks in a file, or
2. whole-file expansion using the same `e` hunk/entity toggle concept.

## 3. Goals
1. Make file rows selectable in the main list.
2. Keep existing entity rows selectable.
3. Reuse `e` toggle semantics by applying it to selected scope:
   - hunk mode => grouped hunks for selected scope
   - entity mode => full selected scope
4. For file selection:
   - `hunk` mode shows all file hunks (navigable)
   - `entity` mode shows whole-file diff
5. Keep entity behavior unchanged when an entity row is selected.

## 4. Non-Goals
1. No parent/container hierarchy in this topic (no class-level selection yet).
2. No new CLI flags.
3. No new standalone persistence file; navigation mode is stored in the existing TUI review-state UI prefs.
4. No semantic diff algorithm changes.

## 5. Current Baseline
1. List rows are entity-only (`EntityRow`).
2. File names are non-selectable headers.
3. Detail renderer operates on selected entity content.
4. `e` toggles between grouped hunks and full-entity rendering.

## 6. Key Decisions
1. Introduce selectable scope kinds in list state:
   - `File`
   - `Entity`
2. File rows are synthetic UI rows built at TUI row-construction time from grouped entity changes.
3. File row appears before that file's entity rows in list order.
4. File row delta is aggregated line counts:
   - `added = sum(child.added_lines)`
   - `removed = sum(child.removed_lines)`
5. File row visual distinction is explicit:
   - file icon token + bold file label
   - entity rows remain one level indented under file row
6. `Enter` on file row opens file-scope detail.
7. `e` remains the same key and token values (`hunk`, `entity`) but semantics are scope-aware:
   - selected entity + `entity` => whole entity
   - selected file + `entity` => whole file
8. `n/p` navigates hunk anchors of selected scope.
9. File-scope hunk ordering is by file line order, not grouped by entity name.
10. Detail title shows selected scope type (`file` or `entity`).
11. File-scope rendering needs full file before/after content. Source of truth is `FileChange` from input collection; TUI receives a derived file snapshot map built from those `FileChange` records.
12. Selection behavior under filtering:
   - file row is shown only when at least one child entity row is visible under current review + annotation filters,
   - if selected row becomes hidden, selection advances to next visible row (wrapping) using existing deterministic selection policy.
13. Detail refresh remains explicit-entry from list (`Enter`) and continuous while already in detail as selection changes from in-detail navigation.
14. Navigation mode is tri-state and persisted in UI prefs:
   - `mixed` => keyboard navigation uses all visible scope rows
   - `entity` => keyboard navigation skips file rows
   - `file` => keyboard navigation skips entity rows
15. `Up/Down` in list/split and `Left/Right` in detail all use the active navigation mode.
16. File rows are reviewable as aggregate actions:
   - file row state is derived from child entity review state (`reviewed`, `unreviewed`, `mixed`)
   - toggling a file row clears all child review records when all are reviewed
   - otherwise toggling a file row marks all reviewable child entities reviewed
17. File rows are non-annotatable in v1; `a` and `D` are no-ops when a file row is selected.

## 7. Contract / Interface Semantics
This topic defines TUI runtime behavior only.

### 7.1 List Contract
1. File rows are selectable entries.
2. File rows preserve existing file-group ordering.
3. Entity rows remain under their file row.
4. Review/filter behavior applies consistently across row kinds with deterministic visibility/selection fallback.
5. Review state is mixed-scope:
   - entity rows own persisted review identities,
   - file rows derive review state from child entities and never persist separate file-level review records.
6. Annotation visibility is entity-scoped:
   - file rows do not own annotation keys,
   - file row visibility under `A` is derived from child entity visibility,
   - file rows render annotation state only indirectly through surviving child rows.

### 7.2 Detail Contract
1. Selected entity row:
   - unchanged behavior from current system.
2. Selected file row:
   - `hunk` mode: grouped hunks for file diff
   - `entity` mode: whole-file diff
3. `n/p` and scroll semantics apply to active scope renderer.
4. `Left/Right` moves across scope rows allowed by the active navigation mode while remaining in detail mode.

### 7.3 Footer / Help Contract
1. Keep existing `e` cell token values.
2. Help text clarifies scope-aware meaning:
   - `e toggle hunk/full scope`
3. Help text includes navigation mode toggle:
   - `f cycle mixed/entity/file nav`
4. Help text for `a` / `D` remains entity-worded because file rows are non-annotatable.

## 8. Service / Module Design
1. `commands/diff.rs`
   - pass file-change content snapshots into TUI startup payload.
   - include file snapshot maps in step/refresh snapshot payloads so file rows keep working after commit navigation and async reloads.
2. `tui/app.rs`
   - replace entity-only row model with scope row model.
   - support file row selection and scope-aware detail refresh.
   - keep annotation identity generation entity-scoped.
   - derive file-row review state from child entity identities and persist only entity review records.
3. `tui/detail.rs`
   - add file-scope render path using full file before/after content.
4. `tui/render.rs`
   - render selectable file rows and nested entity rows.
   - enforce visual distinction and indentation contract.
   - update footer/help copy from entity-only wording to scope-aware wording where applicable.
   - render spacing between compact scope icon and label consistently.

## 9. Error Semantics
1. Missing file content for selected file row => non-fatal placeholder detail view.
2. Empty file hunks in hunk mode => deterministic no-hunks state.
3. Added/deleted file edge cases:
   - added file: whole-file mode renders after-only content
   - deleted file: whole-file mode renders before-only content
4. Binary/non-UTF8 file content in file scope => deterministic unavailable-content placeholder.
5. Scope toggle on file row never panics; it always switches render mode.

## 10. Migration Strategy
1. Additive TUI behavior only.
2. Existing entity workflows remain valid.
3. No output format changes for non-TUI command modes.
4. Row indices are treated as ephemeral UI positions only; no persisted index contract.

## 11. Test Strategy
1. App-state tests:
   - file rows appear and are selectable
   - list navigation includes file + entity rows
   - boundary navigation at top/bottom across mixed row kinds
2. Scope behavior tests:
   - file row + hunk mode renders grouped file hunks
   - file row + entity mode renders full-file diff
   - entity rows preserve prior behavior
3. Toggle/navigation tests:
   - `e` works on both scope types
   - `n/p` hunk traversal works for file scope and is line-order deterministic
4. Data plumbing tests:
   - file snapshot map reaches TUI from diff command path
5. Filter/review tests:
   - file row hidden when no visible child rows remain
   - file row hidden when annotation filter removes all child entity rows
   - selected-row fallback is deterministic when filters hide current row
   - file row aggregate review toggle correctly handles `mixed` child state
6. Safety tests:
   - missing file content path is non-fatal
   - added/deleted/binary file paths render deterministic placeholders or one-sided content
7. Performance smoke:
   - large multi-file diff startup stays within documented baseline threshold (tracked in evidence).

## 12. Acceptance Criteria
1. Operators can select file rows.
2. File selection supports hunk and full-file rendering via existing `e` toggle.
3. Entity selection behavior remains backward compatible.
4. Navigation/hunk stepping remains stable.

## 13. Constraints and Explicit User Preferences
1. Keep this topic file-only; do not add class/parent level in this scope.
2. Reuse existing toggle concept (`e`) instead of introducing new mode keys.
3. Keep interaction model consistent with current TUI mental model.
