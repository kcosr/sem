# Changelog

## 2026-03-08

### Added
- Rust `sem diff` interactive TUI mode behind `--tui`.
- `--diff-view <unified|side-by-side>` for TUI detail rendering.
- TUI list/detail navigation (`Enter`, `Esc`, `Tab`, `n/p`, `PageUp/PageDown`, `g/G`, `?`, `q`).
- Width-aware side-by-side fallback behavior.

### Changed
- `sem-core` `SemanticChange` now includes optional line-range metadata:
  - `beforeStartLine`, `beforeEndLine`, `afterStartLine`, `afterEndLine` (JSON camelCase).
- `sem-cli diff` command refactored into input/compute/output phases.
- JSON formatter now serializes the full `SemanticChange` optional contract fields.
- Terminal formatter now displays line-range labels when available.

### Testing
- Added sem-core tests for line-range population and serialization behavior.
- Added sem-cli tests for CLI contracts, TUI state machine, hunk boundaries, and constrained-width fallback rendering.
