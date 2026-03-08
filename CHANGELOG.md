# Changelog

## 2026-03-08

### Added
- Rust `sem diff` interactive TUI mode behind `--tui`.
- `--diff-view <unified|side-by-side>` for TUI detail rendering.
- TUI list/detail navigation (`Enter`, `Esc`, `Tab`, `n/p`, `PageUp/PageDown`, `g/G`, `?`, `q`).
- Width-aware side-by-side fallback behavior.
- Commit-navigation mode for `sem diff --tui --commit <rev>` with `[`/`]` commit stepping.
- First-parent commit cursor metadata in TUI header (`HEAD~N` when derivable, short SHA, subject).
- Async reload worker with request coalescing and stale-result suppression (`latest request wins`).
- Non-fatal boundary/unsupported/error status hints with previous snapshot retention on load failure.

### Changed
- `sem-core` `SemanticChange` now includes optional line-range metadata:
  - `beforeStartLine`, `beforeEndLine`, `afterStartLine`, `afterEndLine` (JSON camelCase).
- `sem-cli diff` command refactored into input/compute/output phases.
- JSON formatter now serializes the full `SemanticChange` optional contract fields.
- Terminal formatter now displays line-range labels when available.
- TUI headers and help/footer text now include commit-navigation controls and mode-scope hints.

### Testing
- Added sem-core tests for line-range population and serialization behavior.
- Added sem-cli tests for CLI contracts, TUI state machine, hunk boundaries, and constrained-width fallback rendering.
- Added sem-cli commit-navigation tests for lineage stepping boundaries (including root), rapid request coalescing (`[ [ [ ] ]`), stale-result rejection, unsupported-mode inert behavior, detail-mode empty snapshot stability, and quit-during-load behavior.
