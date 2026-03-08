# Architecture Reference

## Status
Active

## Diff TUI Entity Navigation (Rust CLI)

### Scope
- Applies to Rust workspace crates only: `crates/sem-core`, `crates/sem-cli`.
- TypeScript CLI is out of scope for this architecture slice.

### Sem-Core Contract
- `SemanticChange` includes optional line-range metadata:
  - `before_start_line`, `before_end_line`
  - `after_start_line`, `after_end_line`
- All change emission paths in `model/identity.rs` populate these fields through a single helper to keep behavior consistent for:
  - modified,
  - renamed/moved (hash and similarity paths),
  - added,
  - deleted.

### Sem-CLI Diff Pipeline
- `diff` command is split into explicit phases:
  1. input acquisition (`git`, `--stdin`, or two-file mode),
  2. semantic compute (`sem-core` registry + matcher),
  3. output execution (terminal/json/tui).
- `--tui` is mutually exclusive with `--format`.
- `--diff-view` accepted values: `unified`, `side-by-side` and requires `--tui`.

### TUI Modules
- `src/tui/app.rs`: state machine, mode-scoped key handling, hunk navigation state.
- `src/tui/detail.rs`: diff adapter from `SemanticChange` content into unified + side-by-side view models.
- `src/tui/render.rs`: list/detail/help drawing and width-aware fallback UX.
- `src/tui/mod.rs`: terminal lifecycle + event loop integration.

### Rendering Behavior
- List mode: file-grouped sorted entities with type/name/change and optional range labels.
- Detail mode:
  - Enter/Esc open/close,
  - Tab toggles renderer,
  - `n/p` hunk navigation,
  - `PageUp/PageDown` scrolling,
  - `g/G` top/bottom jumps,
  - `?` help overlay,
  - `q` quit.
- Side-by-side fallback:
  - If terminal width is below threshold, effective view falls back to unified,
  - fallback is reversible when width increases or view toggles.

### JSON/Terminal Output
- JSON formatter serializes full `SemanticChange` contract in camelCase, including optional existing and line-range fields.
- Terminal formatter includes optional range labels when available.

### Dependencies (TUI)
- `ratatui` for layout and rendering,
- `crossterm` for terminal events/raw mode,
- `similar` for line-level diff grouping and hunk models.
