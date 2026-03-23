# sem-core library integration

## goal

Use `sem-core` as a semantic engine and keep application-specific orchestration/UI logic in the app, without maintaining a fork.

## what to use sem-core for

- Language-aware entity extraction (tree-sitter plugin stack).
- Semantic identity primitives (content hash + structural hash behavior).
- Entity matching/classification (`added`, `modified`, `deleted`, `moved`, `renamed`).
- Baseline semantic diff computation from file snapshots.

## what to keep in app code

- Git endpoint model and stepping:
  - commit/index/working endpoint graph
  - pairwise/cumulative stepping
  - range/bootstrap behavior
- TUI state machine:
  - view modes, focus, keybindings, mouse handling
  - review-state persistence and app-specific UX state
- Output contracts:
  - JSON shape
  - terminal rendering semantics
  - any app API payload model

## recommended boundary

Create an internal adapter layer so `sem-core` types do not leak through app layers.

- input to adapter:
  - app `FileSnapshot` or equivalent (before/after content + path/status)
- adapter responsibilities:
  - map app snapshot -> `sem_core::git::types::FileChange`
  - call `compute_semantic_diff` (or lower-level matching where needed)
  - map `DiffResult`/`SemanticChange` -> app domain model
- output from adapter:
  - app-owned `AppDiffResult` and `AppChange`

This keeps future `sem-core` upgrades isolated to one integration surface.

## no-fork accommodation strategy

If upstream `sem-core` does not expose every field the app wants:

1. Derive additional app metadata locally from extracted entities and snapshots.
2. Keep commit lineage/subject traversal in app code via `git2`.
3. Store app-only fields in `AppChange`, not in `sem-core` structs.

## migration plan (high-level)

1. Introduce `sem_adapter` module in app.
2. Move endpoint/navigation loading code into app service layer.
3. Point TUI/rendering to app-owned models only.
4. Keep `sem` CLI behavior out of runtime path (library call only).
5. Add parity tests:
   - classification parity (`added/modified/deleted/moved/renamed`)
   - line/range behavior parity expected by TUI
   - endpoint stepping parity for commit/index/working

## rationale

- Avoids fork maintenance burden.
- Preserves flexibility for custom TUI and product-specific behavior.
- Keeps language parsing/matching delegated to a focused engine.
