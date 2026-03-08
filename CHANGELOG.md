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

### Testing
- Added sem-core tests for line-range population and serialization behavior.
- Added sem-cli tests for CLI contracts, TUI state machine, hunk boundaries, and constrained-width fallback rendering.
- Added sem-cli commit-navigation tests for lineage stepping boundaries (including root), rapid request coalescing (`[ [ [ ] ]`), stale-result rejection, unsupported-mode inert behavior, detail-mode empty snapshot stability, and quit-during-load behavior.
- Added sem-cli unified-stepping tests for mode comparator selection, startup defaults/`--step-mode` (explicit range, commit, staged, and implicit/latest), pseudo-endpoint range bootstrap, startup refresh loading, JSON range compatibility for `HEAD..WORKING`, and `INDEX`/`WORKING` empty<->populated transition hardening.
