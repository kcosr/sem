# Changelog

## 2026-03-08

### Added
- Rust `sem diff` interactive TUI mode behind `--tui`.
- `--diff-view <unified|side-by-side>` for TUI detail rendering.
- TUI list/detail navigation (`Enter`, `Esc`, `Tab`, `n/p`, `PageUp/PageDown`, `g/G`, `?`, `q`).
- Width-aware side-by-side fallback behavior.
- Commit-navigation mode for `sem diff --tui --commit <rev>` with `[`/`]` commit stepping.
- Unified endpoint stepping in TUI across commit/index/working paths.
- Runtime step-mode toggle (`m`) with comparator-aware headers (`previous/current` vs `base/cursor`) and footer mode token.
- `--step-mode <pairwise|cumulative>` startup override for TUI mode.
- Pseudo-endpoint support in `--from/--to` (`INDEX`, `WORKING`) for TUI and loader paths.
- First-parent commit cursor metadata in TUI header (`HEAD~N` when derivable, short SHA, subject).
- Async reload worker with request coalescing and stale-result suppression (`latest request wins`).
- Non-fatal boundary/unsupported/error status hints with previous snapshot retention on load failure.
- Diff TUI reviewed-state controls: `Space` to toggle reviewed on focused/opened entity and `r` to cycle filter (`all`/`unreviewed`/`reviewed`).
- Local review-state persistence at `.sem/tui-review-state.json` with per-repo filter preference and reviewed carryover based on identity + target hash.
- Diff TUI entity-context toggle: `e` switches detail rendering between `hunk` context and full `entity` context.
- Diff TUI optional local HTTP state endpoint (`--tui-http`, `--tui-http-port`, default `127.0.0.1:7778`) with deterministic `GET /state` / `404` / `405` JSON semantics.
- Diff TUI impact summary + expandable detail panel (`i` in detail mode) with bounded dependency/dependent/impact sections and overflow indicators.

### Changed
- `sem-core` `SemanticChange` now includes optional line-range metadata:
  - `beforeStartLine`, `beforeEndLine`, `afterStartLine`, `afterEndLine` (JSON camelCase).
- `sem-cli diff` command refactored into input/compute/output phases.
- JSON formatter now serializes the full `SemanticChange` optional contract fields.
- Terminal formatter now displays line-range labels when available.
- TUI headers and help/footer text now include commit-navigation controls and mode-scope hints.
- TUI startup defaults are now deterministic by invocation type:
  - explicit `--from/--to` => `cumulative`
  - implicit/latest and `--commit` => `pairwise`
- TUI list/detail footer cells now include review filter state (`r: <all|unreviewed|reviewed>`) alongside step mode (`m: <pairwise|cumulative>`).
- TUI list/detail footer cells now include entity-context state (`e: <hunk|entity>`) with shared `m | r | e` ordering.
- TUI detail rendering now supports full-entity mode changed-region anchors for deterministic `n/p` traversal in unified and side-by-side views.
- Filtered TUI list rendering now hides file headers with zero visible entities and shows an explicit no-match row when filter output is empty.
- Diff TUI `/state` payload now tracks panel expansion state and compact impact summary across list/detail transitions.

### Testing
- Added sem-core tests for line-range population and serialization behavior.
- Added sem-cli tests for CLI contracts, TUI state machine, hunk boundaries, and constrained-width fallback rendering.
- Added sem-cli commit-navigation tests for lineage stepping boundaries (including root), rapid request coalescing (`[ [ [ ] ]`), stale-result rejection, unsupported-mode inert behavior, detail-mode empty snapshot stability, and quit-during-load behavior.
- Added sem-cli unified-stepping tests for mode comparator selection, startup defaults/`--step-mode` (explicit range, commit, staged, and implicit/latest), pseudo-endpoint range bootstrap, startup refresh loading, JSON range compatibility for `HEAD..WORKING`, and `INDEX`/`WORKING` empty<->populated transition hardening.
- Added sem-cli review-state coverage for filtered navigation/no-match rendering, detail-mode review toggle/filter behavior, reviewed marker rendering, file-header suppression under filters, and carryover/non-carryover behavior across snapshot reloads.
- Added sem-cli H3 hardening coverage for entity-context mode: identical-content empty-anchor boundary no-op behavior, non-zero index/scroll reset on `e` toggles, and footer/help render assertions for `e: hunk|entity`.
- Added sem-cli H4 hardening coverage for HTTP impact state: truncation semantics, zero-impact snapshots, bind-failure continuation, method-mismatch handling, panel state transition sync, and expanded-panel render bounds/empty-state behavior.
