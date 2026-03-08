# Dependency Audit: Diff TUI Entity Navigation

## Status
Completed (2026-03-08)

## Direct Dependencies Added

### `ratatui = "0.29"`
- Purpose: terminal UI layout/rendering.
- Usage scope: `crates/sem-cli/src/tui/render.rs`, `tui/mod.rs`.
- Notes: no file mutation/staging behavior introduced; read-only rendering surface.

### `crossterm = "0.29"`
- Purpose: terminal event handling and raw/alternate-screen lifecycle.
- Usage scope: `crates/sem-cli/src/tui/mod.rs`, `tui/app.rs` key events.
- Notes: terminal state restoration guarded by RAII drop path.

### `similar = "2"`
- Purpose: line-level diff and hunk grouping for unified/side-by-side detail views.
- Usage scope: `crates/sem-cli/src/tui/detail.rs`.
- Notes: grouped ops with context radius 3 for deterministic hunk rendering.

## Risk Notes
- Width fallback keeps side-by-side rendering from failing on narrow terminals.
- UTF-8-safe truncation is enforced in side-by-side column rendering.
- No external pager or filesystem mutation dependency introduced.

## Additional Audit Notes
- License compatibility:
  - `ratatui`, `crossterm`, and `similar` are MIT-compatible and acceptable under repository licensing (`MIT OR Apache-2.0`).
- MSRV impact:
  - No explicit MSRV bump was required for this topic; dependencies resolve under the existing workspace toolchain used in this execution stream.
- Transitive footprint:
  - `cargo tree -p sem-cli` confirms direct inclusion of `ratatui`, `crossterm`, and `similar` in the CLI graph.
- Binary/runtime impact:
  - New dependencies are exercised only on `--tui` paths; default non-TUI CLI behavior remains unchanged.
