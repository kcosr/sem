# Design: Diff TUI Footer Cell Layout

## Status
Locked

## 1. Purpose
Define and implement a shared footer cell layout so mode/filter/context state is rendered as compact key-value cells instead of free-form status strings.

## 2. Problem Statement
Current footers are assembled as long text strings with ad-hoc separators. This makes extension brittle and causes drift across features that need persistent state indicators.

## 3. Goals
1. Introduce a deterministic footer cell rail contract.
2. Implement `m: <pairwise|cumulative>` as concrete baseline behavior.
3. Reserve compatible slots for review filter (`r`) and entity context (`e`).
4. Preserve transient loading/status messaging in a dedicated right-side status slot.

## 4. Non-Goals
1. No review-filter behavior implementation in this topic.
2. No entity-context toggle behavior implementation in this topic.
3. No keyboard map changes.

## 5. Current Baseline
1. Footer strings are concatenated using ` | ` separators.
2. Unified stepping currently exposes mode via `mode: ...` token.
3. Loading/status messages are mixed into the same free-form footer string.

## 6. Key Decisions
1. Footer is rendered as two logical areas:
   - controls text area (left)
   - cell/status area (right)
   - controls area retains existing discoverability/help text and non-state hints.
2. Cell rendering format is lowercase: `<key>: <value>`.
3. Cell delimiter is canonical: ` | `.
4. Canonical cell key order is: `m`, `r`, `e`.
5. This topic implements `m` now and leaves `r`/`e` for later topics.
6. Step mode token migration is locked:
   - from `mode: pairwise|cumulative`
   - to `m: pairwise|cumulative`
7. Rightmost status slot is ephemeral and separate from state cells.
8. Loading text uses default footer text color; no full-footer color swap.
9. Narrow-width behavior prioritizes preserving state cells over full controls text.
10. When state cells and status text compete, status text truncates/omits before state-cell eviction.

## 7. Contract / Interface Semantics
This is a TUI runtime contract (not HTTP).

### 7.1 Cell Contract
1. `m` values are exactly `pairwise` or `cumulative`.
2. `r` reserved values are `all`, `unreviewed`, `reviewed`.
3. `e` reserved values are `hunk`, `entity`.
4. Cells are stable, compact, and order-preserving.
5. Runtime may ignore unknown/unmodeled keys without reordering known keys.

### 7.2 Status Slot Contract
1. Loading/status text appears only in the status slot.
2. Status slot is omitted when no active message exists.
3. Status slot does not alter state-cell colors or values.

## 8. Service / Module Design
1. `tui/render.rs`
   - add footer cell model and renderer helper
   - split footer into controls area and right-side cell/status area
   - emit `m` cell from app state
2. `tui/app.rs`
   - no mode-semantic changes; expose mode token for new cell renderer
3. tests
   - footer render test for `m: pairwise` and `m: cumulative`
   - narrow-width test proving cell visibility priority
   - loading/status test proving state-cell color/value stability

## 9. Error Semantics
1. Missing mode token falls back to `pairwise` token in UI only (non-fatal guard).
2. Missing status message is a normal no-status state.

## 10. Migration Strategy
1. Additive renderer refactor.
2. Preserve existing behavior except mode token presentation format change (`mode:` -> `m:`).
3. Downstream topics can append `r` and `e` cells without reworking footer layout.

## 11. Test Strategy
1. Footer cell formatting tests for `m` values.
2. Footer layout tests for list and detail views.
3. Loading/status rendering tests ensuring no full-footer color mutation.
4. Narrow-width tests include long-status contention to prove state-cell priority.
5. Regression tests for existing key/stepping behavior.

## 12. Acceptance Criteria
1. Footer shows `m: pairwise` or `m: cumulative` in list/detail views.
2. Controls text remains visible (or predictably truncated on narrow widths).
3. Loading/status appears in dedicated right-side slot.
4. Existing stepping behavior is unchanged.

## 13. Constraints and Explicit User Preferences
1. Footer should be cell-based and extensible.
2. Mode should be displayed as `m: <mode>`.
3. Loading indicator must not recolor the whole footer.
