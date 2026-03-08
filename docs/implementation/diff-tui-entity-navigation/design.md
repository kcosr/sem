# Design: Diff TUI Entity Navigation

## Status
Locked

## 1. Purpose
Define and lock the design for a Rust-only interactive TUI mode for `sem diff` that lets operators navigate semantic entities, open full entity-level diffs, and inspect multi-hunk changes in unified or side-by-side form.

## 2. Problem Statement
Current `sem diff` terminal output is static and optimized for summary scanning. Inline snippets appear only for modified entities where both sides are at most 3 lines, which suppresses most useful diffs in real code. Operators cannot interactively inspect one entity at a time, jump hunks, or view richer context without rerunning external tools.

## 3. Goals
1. Add interactive TUI mode to Rust `sem` CLI only.
2. Support keyboard navigation over semantic changes.
3. Open selected entity diff with `Enter` and close with `Esc`.
4. Support unified and side-by-side diff views.
5. Surface entity span labels (`[Lstart-Lend]`) from model-level line ranges.
6. Preserve existing non-TUI behavior by default.
7. Keep deterministic behavior for no-change and non-repo error paths.

## 4. Non-Goals
1. No TypeScript CLI parity work in this topic.
2. No live watch mode integration in TUI (`sem watch` remains separate).
3. No persistent user preferences/config file in first delivery.
4. No mouse interaction requirement.
5. No external pager dependency.
6. No syntax highlighting in v1.

## 5. Current Baseline
1. `sem diff` computes semantic changes and formats static terminal or JSON output.
2. `SemanticEntity` already carries `start_line` and `end_line`.
3. `SemanticChange` currently omits line-range fields, so final output cannot show absolute entity ranges.
4. Terminal formatter shows inline snippet only for short modified entities (`<=3` lines per side).
5. No TUI dependencies or event loop exist in `sem-cli` today.

## 6. Key Decisions
1. Implement TUI only in Rust `crates/sem-cli`.
2. Add `--tui` flag to `sem diff`.
3. Add `--diff-view <unified|side-by-side>` with default `unified`.
4. Keep `--format` for non-TUI flow; with `--tui`, `--format` is invalid.
5. Add optional line-range fields to `SemanticChange`:
   - `before_start_line`, `before_end_line`
   - `after_start_line`, `after_end_line`
6. Populate line ranges at all `SemanticChange` emission sites in `match_entities` (modified, renamed/moved by hash, renamed/moved by similarity, added/deleted). A local helper may be introduced during execution to avoid duplication errors.
7. Use `ratatui` + `crossterm` for UI/runtime events.
8. Use `similar` for diff generation (multi-hunk, unified, side-by-side mapping).
9. Keep TUI read-only; no file edits or git staging actions.
10. Flatten entity list (no tree nesting in v1), grouped by file.
11. Sort files lexicographically by path; within file preserve semantic diff order.
12. Use 3 context lines for unified hunks in v1.
13. In side-by-side, use line-level coloring only (added/removed/unchanged); no intra-line color diff in v1.

## 7. Contract / Interface Semantics
This feature is CLI contract, not HTTP.

### 7.1 CLI Inputs
1. `sem diff --tui` launches interactive mode.
2. `sem diff --tui --diff-view unified|side-by-side` sets initial diff renderer.
3. `--tui` is mutually exclusive with `--format`.
4. `--tui` is allowed with git-derived diff, `--stdin`, and two-file compare mode, as long as a valid `DiffResult` is produced.

### 7.2 CLI Outputs
1. TUI main view: navigable, file-grouped entity list.
2. Entity row includes type/name/change tag and range label when available.
3. Entered detail view renders full entity diff with hunk navigation.
4. If no changes: print existing no-change message and do not launch TUI.

### 7.3 Keyboard Contract (v1)
1. `Up/Down` or `k/j`: selection move in list.
2. `Enter`: open selected entity detail view.
3. `Esc`: close detail view only.
4. `Tab`: unified <-> side-by-side toggle in detail view.
5. `n/p`: next/previous hunk in detail view.
6. `PageUp/PageDown`: scroll detail view by page.
7. `g/G`: top/bottom jump in list or detail (active pane scope).
8. `q`: quit app from current mode.
9. `?`: help overlay.

Input handling rule: key actions are scoped to active mode/pane to prevent accidental global quits.

### 7.4 JSON Contract Extension
When using `--format json` (non-TUI), each change may include optional:
1. `beforeStartLine`, `beforeEndLine`, `afterStartLine`, `afterEndLine`
2. Existing `id`, `timestamp`, and `structuralChange` remain part of JSON contract.

Naming convention lock:
1. Rust model fields remain snake_case.
2. JSON output remains camelCase (`serde rename_all = "camelCase"`).

## 8. Module / Service Design
1. `sem-core`
   - Extend `SemanticChange` model with optional line fields.
   - Populate fields in every change-emission path in `model/identity.rs`.
2. `sem-cli`
   - Extend diff command args/parsing with `--tui` and `--diff-view`.
   - Refactor `diff_command` into data phase + output phase so TUI and static formatters share the same `DiffResult` and deterministic error mapping.
   - Add `src/tui/` module set:
     - app state (`AppState`, selection, mode)
     - input handling (mode-scoped key maps)
     - renderers (list pane + detail pane/modal)
     - diff adapter (convert `SemanticChange` into hunk view model with file-absolute labels)
3. Formatters
   - Terminal formatter may include `[Lx-Ly]` labels when fields are present.
   - JSON formatter passes through new fields and existing optional fields.
4. Dependency audit
   - Record new crates (`ratatui`, `crossterm`, `similar`) and verify no breakage in build/test footprint.

## 9. Error Semantics
1. Not in repo and no `--stdin` / file pair input:
   - same deterministic failure behavior as today (error + non-zero).
2. `--tui` with any `--format`:
   - deterministic argument validation error and non-zero exit.
3. Invalid `--diff-view`:
   - deterministic clap validation error and non-zero exit.
4. Narrow terminal for side-by-side:
   - fallback to unified with status notice.
   - on resize wider, user can toggle back to side-by-side; app does not lock permanently.
5. Missing before/after content for selected row:
   - show non-fatal "content unavailable" panel.
6. Non-UTF8/binary-content decode failures in diff material:
   - non-fatal placeholder message; app remains stable.

## 10. Migration Strategy
1. Add model fields as optional and backward-compatible.
2. Ensure existing serialized JSON consumers are unaffected by absent new fields.
3. Introduce TUI behind explicit `--tui` flag.
4. Keep default `sem diff` static output unchanged except optional range labels when available.

## 11. Test Strategy
1. `sem-core` unit tests:
   - verify new line fields for modified/added/deleted/moved/renamed, including cross-file moved/renamed cases.
2. `sem-cli` argument tests:
   - reject `--tui --format ...`.
   - accept `--tui --diff-view side-by-side`.
   - verify explicit default of `--diff-view unified`.
3. TUI state-machine tests:
   - navigation keys, enter/esc transitions, hunk index boundaries (`n` at last, `p` at first), and mode-scoped key handling.
4. TUI rendering tests with test backend:
   - unified view rendering.
   - side-by-side rendering.
   - width fallback behavior.
5. Behavioral tests:
   - no-change path with `--tui`.
   - `--tui` with `--stdin` and two-file mode.
   - non-UTF8/binary fallback stability.
6. Performance smoke:
   - verify responsiveness on large entity lists (defined threshold in H3 evidence).

## 12. Acceptance Criteria
1. `sem diff --tui` opens interactive UI when changes exist.
2. Operator can select an entity and inspect full diff with Enter/Esc.
3. Multi-hunk entity diffs are rendered and navigable.
4. Side-by-side mode is available and toggleable.
5. Change rows can display entity range labels.
6. JSON output includes optional line fields while retaining existing optional fields.
7. Existing non-TUI diff behavior remains stable.

## 13. Constraints and Explicit User Preferences
1. Rust-only implementation is acceptable.
2. Operator wants "real" diff inspection rather than tiny snippets.
3. Enter/Esc navigation flow is expected.
4. Side-by-side support is required.
