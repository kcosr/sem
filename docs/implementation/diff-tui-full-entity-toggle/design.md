# Design: Diff TUI Full-Entity Toggle

## Status
Locked

## 1. Purpose
Add a detail-context toggle that switches entity diff rendering between focused hunk context and full-entity context, with a compact footer indicator that doubles as discoverable key help.

## 2. Problem Statement
Current detail rendering is hunk-grouped (`grouped_ops(3)`), which is fast to scan but can hide broader surrounding entity context. Operators need a way to see the complete entity while preserving hunk navigation. Current footer messaging is also overloaded and does not expose this mode cleanly.

For this topic, "entity" means the selected semantic entity row (`function`, `class`, etc.) and its full before/after content payload already present in `SemanticChange`.

## 3. Goals
1. Add a runtime detail-context mode: `hunk` and `entity`.
2. Add `e` keyboard toggle (lowercase) to switch context mode.
3. Keep `n/p` hunk navigation working in both modes.
4. Render a stable footer cell `e: <mode>` in list and detail views.
5. Keep default startup behavior as `hunk` mode.

## 4. Non-Goals
1. No file-aggregate full-file render mode in this topic.
2. No persistence of entity-context preference across sessions.
3. No hunk reviewed/annotation state changes.
4. No keybinding rework beyond adding `e` and footer cell layout updates.

## 5. Current Baseline
1. Detail diff uses grouped hunks with fixed context radius and does not expose full-entity view.
2. `n/p` jump between grouped hunks.
3. Footer already shows controls and step mode but not entity-context mode.
4. List/detail views are stable and keyboard-driven.

## 6. Key Decisions
1. Introduce `EntityContextMode` enum with tokens: `hunk`, `entity`.
2. `e` toggles mode in both list and detail views.
3. Toggle is session-local (non-persistent), defaulting to `hunk` on startup.
4. Footer includes explicit mode cell rendered as lowercase value token only with key prefix:
   - `e: hunk`
   - `e: entity`
5. Mode cell is present in both list and detail views.
6. Hunk mode keeps current grouped rendering behavior.
7. Entity mode renders full entity diff line stream (all lines in scope), not grouped snippets.
8. Entity mode computes hunk anchors from changed regions in that full stream; a changed region is a contiguous run of non-equal diff operations.
9. Anchors are ordered and deduplicated per active view renderer.
10. Example anchor semantics:
   - change runs at logical lines 12-14 and 30 produce anchor list `[12, 30]` (first line index per changed region).
11. On `e` toggle while in detail view:
   - `detail_hunk_index` resets to `0`
   - `detail_scroll` resets to `0`
12. Render pipeline computes both mode artifacts from a single diff pass per detail refresh and keeps them in memory for fast toggling.
13. Help overlay line text is locked as:
   - `e toggle hunk/entity context`
14. `n/p` uses active mode anchors:
   - hunk mode: grouped hunk anchors
   - entity mode: changed-region anchors in full stream
15. Anchor behavior is defined for all four combinations:
   - hunk + unified
   - hunk + side-by-side
   - entity + unified
   - entity + side-by-side
16. No top-of-screen status banner is added for this mode.

## 7. Contract / Interface Semantics
This is a CLI/TUI runtime contract (not HTTP).

### 7.1 Keyboard Contract
1. `e` toggles entity context mode (`hunk <-> entity`) in list and detail.
2. Existing keys remain unchanged (`n/p`, `Tab`, `[`/`]`, `m`, etc.).
3. In list mode, `e` updates mode state immediately and applies when entering detail.
4. In detail mode, toggle resets hunk index and scroll to the top before next render.

### 7.2 Footer Contract
1. Footer includes a dedicated cell `e: <mode>` where mode is lowercase token.
2. Cell is always shown in list and detail views.
3. Footer layout remains single-line and non-modal.

### 7.3 Detail Rendering Contract
1. `hunk` mode behavior remains equivalent to existing grouped-hunk rendering.
2. `entity` mode renders full entity scope from before/after content.
3. Changed-line styling remains intact for unified and side-by-side views.
4. If content is unavailable, existing non-fatal placeholder behavior remains.
5. Full-entity mode with identical before/after content is valid and yields:
   - unchanged render lines
   - empty anchor list
   - deterministic `n/p` boundary no-op behavior.

## 8. Service / Module Design
1. `tui/app.rs`
   - add `entity_context_mode` state
   - add toggle action and key handlers for list/detail
   - expose current mode for renderer/footer and detail generation
2. `tui/detail.rs`
   - support two rendering paths: grouped-hunk and full-entity
   - emit mode- and view-specific hunk anchors with deterministic ordering
   - define changed-region anchors as contiguous non-equal diff-op runs
3. `tui/render.rs`
   - footer format update with dedicated `e: <mode>` cell
   - help overlay includes `e toggle hunk/entity context`
4. tests
   - app-state toggle tests (list/detail)
   - detail render tests for full-entity path and anchors
   - footer render tests for `e: hunk|entity`

## 9. Error Semantics
1. Missing entity content remains non-fatal and shows existing placeholder.
2. Toggle action is always safe; no-op is forbidden (must always flip mode).
3. Anchor list empty is valid; `n/p` becomes deterministic boundary no-op.
4. Toggle in detail always resets `detail_hunk_index` and `detail_scroll` to avoid cross-mode anchor drift ambiguity.

## 10. Migration Strategy
1. Behavior is additive and TUI-only.
2. Default mode preserves current user-visible behavior (`hunk`).
3. Existing non-TUI outputs and JSON contracts are unchanged.

## 11. Test Strategy
1. App-state tests for `e` in list/detail and startup default.
2. Detail renderer tests:
   - hunk mode parity with existing baseline
   - entity mode includes unchanged context outside grouped hunks
   - entity-mode anchor dedupe/order
3. Navigation tests proving `n/p` works against active mode anchors.
4. Footer tests for mode cell in list and detail.
5. Matrix tests for `(hunk|entity) x (unified|side-by-side)` anchor behavior.
6. Mode-toggle-in-detail tests at non-zero hunk index (index/scroll reset behavior).
7. Entity-mode test for identical before/after content (empty anchor list).
8. Render snapshot/frame tests for entity mode in both views.
9. Regression tests ensuring stepping/view toggles still function.

## 12. Acceptance Criteria
1. Operator can press `e` to switch between `hunk` and `entity` mode.
2. Footer displays `e: hunk` or `e: entity` in list and detail.
3. Entity mode shows complete entity context while preserving diff highlighting.
4. `n/p` hunk stepping works in both modes without crashes.
5. Startup default remains `hunk`.

## 13. Constraints and Explicit User Preferences
1. File-level aggregate mode is deferred.
2. Key for this toggle is lowercase `e`.
3. Footer mode cell should be concise and value-oriented.
4. No additional modal/banner UX for this feature.
