# Design: Diff TUI View Cycle + Compact Split

## Status
Locked

## 1. Purpose
Add a third TUI mode that keeps entity navigation and diff content visible at the same time: a compact split layout with a narrow entity sidebar and a live diff pane, plus a single `v` key to cycle across all views.

## 2. Problem Statement
Current TUI flow is binary:
1. `List` mode for scanning changed entities.
2. `Detail` mode for reading one entity diff.

Operators must switch back and forth to keep list context while inspecting diffs. This increases navigation churn and weakens scanning speed for dense review sessions.

## 3. Goals
1. Introduce a third mode, `Split`, alongside existing `List` and `Detail`.
2. Add `v` key to cycle views in deterministic order (`list -> split -> detail -> list`).
3. Keep split sidebar compact:
   - file grouping header
   - entity icon (type glyph)
   - entity name
   - inline delta (`+/-`)
4. In split sidebar, remove verbose columns (no textual type label, no long change tags).
5. Preserve existing diff pane behavior (unified/side-by-side) while in `Split`.
6. Add footer state cell `v: <view>` in the same compact cell rail style as existing mode/filter cells.

## 4. Non-Goals
1. No new CLI flags in this topic.
2. No persistence of view mode across sessions.
3. No class/entity hierarchy nesting in sidebar (still file-grouped flat entity rows).
4. No changes to semantic diff computation or entity extraction.
5. No changes to JSON output contract for non-TUI `sem diff --format json`.

## 5. Current Baseline
1. `AppState::Mode` has `List` and `Detail` only.
2. Renderer has separate `draw_list` and `draw_detail` paths.
3. List view shows expanded columns: type, entity, change tag, `+/-`.
4. Footer cell rail currently includes `m`, `r`, and `e`.
5. Detail pane already supports unified/side-by-side and hunk/entity context toggles.

## 6. Key Decisions
1. Extend runtime mode enum to three values:
   - `List`
   - `Split`
   - `Detail`
2. `v` is the canonical view-cycle key and works from any mode.
3. Cycle order is strict and wraps: `list -> split -> detail -> list`.
4. `Enter` remains a direct transition to `Detail` when in `List` or `Split`.
5. `Esc` from `Detail` returns to the last non-detail mode (`List` or `Split`) to preserve user context.
6. `last_non_detail_mode` initializes as `List` at session start.
7. Split left pane reuses existing selection/filter state and row ordering; only rendering density changes.
8. Split left pane columns are locked to: marker+change icon, entity icon+name, inline `+/-`.
9. Footer cell ordering becomes: `m`, `r`, `e`, `v`.
10. `v` footer values are lowercase tokens: `list`, `split`, `detail`.
11. Split input focus model in v1 is single-focus-left:
   - list-navigation keys (`j/k`, `Up/Down`, `g/G`) move sidebar selection.
   - right pane is passive preview (no independent cursor/focus mode).
12. `Tab` is available in `Split` and toggles preview renderer between unified and side-by-side.
13. In `Split`, `n/p`, `PageUp/PageDown`, and line-scroll keys do not scroll the preview pane; use `Enter` to open `Detail` for hunk/scroll navigation.
14. `Left/Right` entity navigation remains detail-only; in split mode it is a no-op.
15. Split preview state is derived from the currently selected sidebar row; returning `Detail -> Esc -> Split` re-renders preview from selection and does not preserve detail hunk/scroll cursor.
16. `Tab` in `Split` only toggles preview renderer mode; it does not mutate detail-mode hunk index or scroll state.
17. Split width contract:
   - target ratio `30/70` (left/right),
   - left pane minimum `28` columns,
   - right pane minimum `52` columns,
   - entity names and file labels use single-line ellipsis truncation (no wrap).
18. If viewport width cannot satisfy both minimums, split mode degrades to compact-list-only with a body-level inline notice plus footer status hint, while keeping mode state as `split`.

## 7. Contract / Interface Semantics
This topic defines TUI runtime contracts only.

### 7.1 Keyboard Contract
1. `v` cycles view mode in fixed order (`list -> split -> detail -> list`).
2. `Enter` opens `Detail` from `List` or `Split`.
3. `Esc` in `Detail` returns to prior non-detail mode.
4. In `Split`, `j/k`, arrows, and `g/G` act on left-pane selection only.
5. In `Split`, `Tab` toggles right-pane diff view (unified/side-by-side) without changing detail-mode hunk/scroll cursor state.
6. In `Split`, `Left/Right` do not perform entity stepping.
7. In `Split`, `n/p`, `PageUp/PageDown`, and line-scroll keys are no-ops; diff scrolling remains detail-only.
8. Existing keys (`m`, `r`, `e`, `?`, `q`) remain available.

### 7.2 Split Layout Contract
1. Left pane is compact and file-grouped.
2. Each entity row includes:
   - selection/review marker + change icon
   - entity icon + entity name
   - inline `+/-` counts
3. Row omits full textual `entity_type` and textual change tag (`[modified]`, etc.).
4. Right pane shows live diff for selected entity (same semantic source as `Detail`) and recomputes from current selection (no split-local scroll cursor).
5. Right pane title remains concise (`Diff` + selected entity identity).
6. Labels and names truncate with ellipsis and never soft-wrap in split rows.
7. Under narrow-width fallback, split mode keeps compact list rendering and replaces right-pane content with explicit in-body notice; footer status can repeat the notice.

### 7.3 Footer + Help Contract
1. Footer must show `v: <view>` in list, split, and detail.
2. Footer controls text must advertise `v` as view-cycle action.
3. Help overlay must include the `v` view-cycle line.
4. Footer uses existing compact cell rail delimiter and status-slot behavior.

## 8. Service / Module Design
1. `tui/app.rs`
   - add split mode to mode enum
   - add `cycle_view_mode()` reducer for `v`
   - add `last_non_detail_mode` with default `List`
   - keep selected row and review/filter state shared across all modes
2. `tui/render.rs`
   - add `draw_split()` path
   - add compact sidebar renderer with file headers + dense rows + ellipsis
   - extract/reuse detail-content render helpers for split right pane
   - update footer cells/controls/help for `v` state
   - add narrow-width degraded split rendering path
3. `tui/mod.rs`
   - no protocol changes; event loop remains single-threaded UI + async reload coordinator
4. docs
   - update user keybinding docs (`README.md`, `crates/README.md`) and changelog entry

## 9. Error Semantics
1. Empty visible rows under review filter: split left pane shows same no-match state as list mode.
2. Missing selected diff content: right pane shows existing non-fatal unavailable placeholder.
3. Narrow terminal width below split minimums triggers compact-list-only degraded split render, with explicit status/notice text.
4. Split no-op keys (`Left/Right`, `n/p`, `PageUp/PageDown`, line-scroll keys) must not mutate selection, mode, or cursor state.
5. Any mode transition must be total and non-panicking.

## 10. Migration Strategy
1. Additive runtime feature only; no schema migration.
2. Startup default remains existing behavior (`List`).
3. Existing `Enter` to detail workflow is preserved.
4. View cycling is optional and does not change default non-TUI command behavior.

## 11. Test Strategy
1. App-state tests:
   - `v` cycle order and wrap behavior, including `detail -> list`
   - `Enter` from list/split to detail
   - `Esc` return path for both `List -> Enter -> Esc` and `Split -> Enter -> Esc`
2. Render tests:
   - split view renders compact rows and file headers
   - split view right pane renders selected entity diff
   - split row truncation with long file/entity names
   - footer includes `v: <view>` cell
3. Input-routing tests:
   - split `j/k` updates left selection and right preview entity
   - split `Left/Right` is no-op
   - split `n/p` is no-op
   - split `PageUp/PageDown` and line-scroll keys are no-op
4. Filter/review integration tests:
   - split pane respects hidden rows and reviewed markers
   - when selected row becomes filtered out, deterministic fallback selection behavior remains stable
5. Diff-view tests:
   - split `Tab` toggles unified/side-by-side preview
   - split side-by-side narrow-width degraded behavior remains non-panicking
6. Safety tests:
   - `List -> Enter -> Detail -> v -> List -> v -> Split` round-trip remains deterministic
   - narrow width split render does not panic
   - placeholder/missing-content split render remains stable

## 12. Acceptance Criteria
1. Operator can press `v` to cycle across all three views.
2. Split mode presents a compact sidebar (file + icon/name + `+/-`) and a concurrent diff pane.
3. Footer shows stable `v: list|split|detail` state.
4. Existing detail diff behavior remains intact.
5. Existing list/detail workflows remain backward compatible.

## 13. Constraints and Explicit User Preferences
1. Compact sidebar should stay as narrow as practical while retaining entity name readability.
2. Split mode should look and feel like current TUI, not a redesign.
3. View cycling key is `v` (not `s`).
4. Keep semantics similar to current layout and controls, with minimal conceptual overhead.
