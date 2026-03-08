# Design: Diff TUI HTTP State + Impact Panel

## Status
Locked

## 1. Purpose
Add an opt-in local HTTP endpoint for Diff TUI that exposes current viewing context plus graph/impact metadata, and add an in-TUI compact summary that can expand into detailed lists.

## 2. Problem Statement
Diff TUI currently shows context in the terminal (selected entity, view/mode, hunk anchors) but does not expose machine-readable live state for companion tools. Existing CLI `graph` and `impact` commands are disconnected from current TUI selection.

Operators need to:
1. query current TUI state over HTTP,
2. receive graph and impact data tied to the current selection,
3. keep compact summary visible in TUI,
4. expand details inline when needed.

## 3. Goals
1. Add opt-in local HTTP server for TUI state snapshots.
2. Include selected-entity graph + impact data in `/state` payload.
3. Show compact summary counts in detail mode.
4. Add expandable details panel in TUI for dependencies, dependents, impact.
5. Preserve existing behavior when feature is disabled.

## 4. Non-Goals
1. Remote bind support (localhost only in this topic).
2. Write/mutation HTTP operations.
3. Authn/authz for HTTP requests.
4. Replacing existing `graph`/`impact` CLI commands.
5. New health/readiness endpoint (`/health`) in this topic.

## 5. Current Baseline
1. `AppState` tracks selection, mode/view, context mode, hunk index, and scroll.
2. No TUI HTTP service exists.
3. Graph/impact are separate CLI commands today.
4. Footer already uses compact right-side status cells with constrained layout.

## 6. Key Decisions
1. Add opt-in HTTP startup via `--tui-http`.
2. Add optional `--tui-http-port <u16>`; default port is `7778`.
3. Server binds to `127.0.0.1:<port>` only.
4. Port bind failure is non-fatal: TUI continues and feature is marked unavailable.
5. HTTP contract is read-only and JSON-only.
6. `GET /state` returns full snapshot shape on success (all top-level sections present).
7. Unknown route returns deterministic `404` JSON error.
8. Non-GET on `/state` returns deterministic `405` JSON error.
9. No CORS headers are emitted in this topic.
10. `session.sourceMode` enum is locked to `repository | stdin | twoFile`.
11. Internal runtime source mapping is explicit:
   - repository-backed TUI modes (`Unified`, `Commit`) -> `repository`,
   - stdin-driven input -> `stdin`,
   - two-file input -> `twoFile`.
12. `selection` is always present and includes `selected: boolean`; when `false`, entity fields are `null`.
13. `selection.ui` shape is locked (`mode`, `view`, `contextMode`, `hunkIndex`, `scroll`, `anchors`) with deterministic value domains.
14. Graph entity `id` is an opaque stable string from graph engine; clients must not parse it.
15. Selection-to-graph matching order:
   - direct graph `id` when selected row carries one,
   - fallback `file + entityType + entityName + line overlap`,
   - on multi-match tie, choose highest overlap then lowest start line.
16. Graph availability reason tokens are locked: `unsupportedSourceMode | graphBuildFailed | selectionNotResolvable`.
17. `graph.reason` is `null` iff `graph.graphAvailable=true`.
18. Snapshot updates synchronously on main-loop state mutations (selection, mode/view, context mode, hunk index, scroll, panel toggle) and readers consume complete immutable snapshots.
19. Impact response cap (`impact.cap`) is independent of panel display cap.
20. `impact.total` is `min(fullTransitiveDependentCount, impact.cap)`; `impact.truncated=true` iff full transitive count exceeds `impact.cap`.
21. Response impact cap default: `10000`; panel display cap default: `25` rows per section.
22. Expanded panel state is session-local and resets when leaving detail mode.
23. HTTP server shuts down with TUI process teardown; no graceful-drain contract beyond process lifetime.

## 7. Contract / Interface Semantics

### 7.1 HTTP Contract
1. Route: `GET /state` -> `200 application/json`.
2. Route: unknown path -> `404 application/json` with `error=notFound`.
3. Route: non-GET `/state` -> `405 application/json` with `error=methodNotAllowed`.
4. Success payload always includes: `session`, `selection`, `graph`, `impact`, `panel`.
5. `session` includes `startedAt`, `sourceMode`, and HTTP runtime metadata (`enabled`, `bound`, `host`, `port`).
6. `selection` includes selection identity and UI anchors; when no resolvable entity exists, `selected=false` and entity fields are `null`.
7. `graph` includes direct dependencies and dependents, plus availability token and reason.
8. `impact` includes transitive dependent total, cap, truncation flag, and bounded entities.
9. `panel` includes `expanded` and fixed summary format `deps:<n> depBy:<n> impact:<n>`.
10. `panel.summary` is informational only; availability truth comes from `graph.graphAvailable` + `graph.reason`.

### 7.2 TUI Contract
1. Compact summary is shown in detail mode.
2. `i` toggles details panel only in detail mode.
3. Outside detail mode, `i` is a deterministic no-op.
4. Expanded panel ordering is deterministic per section: file asc, start-line asc.
5. Each section is capped to panel display cap with `+N more` indicator.
6. Existing keys and navigation semantics are preserved.

### 7.3 Availability Contract
1. Graph/impact are available only for `repository` source mode.
2. For unavailable graph states, `graph.graphAvailable=false` and `graph.reason` is non-null.
3. For available graph states, `graph.graphAvailable=true` and `graph.reason=null`.
4. Unavailable states are non-fatal and must not abort render loop.

## 8. Service / Module Design
1. `commands/diff.rs`
   - add `--tui-http` and `--tui-http-port` plumbing.
2. `tui/mod.rs`
   - startup/shutdown wiring for graph snapshot service and HTTP server.
3. `tui/app.rs`
   - expose selection snapshot, panel expansion state, and `i` toggle behavior.
4. `tui/render.rs`
   - compact summary rendering and expandable detail panel rendering.
5. `tui/http_state.rs` (new)
   - local listener, route dispatch, JSON serialization, deterministic error payloads.
6. tests
   - CLI parsing tests, mapping tests, endpoint tests, render/app toggle tests.

## 9. Error Semantics
1. HTTP bind failure -> non-fatal, feature unavailable, TUI continues.
2. Graph build failure -> non-fatal, unavailable token in payload.
3. Selection not resolvable -> non-fatal, reason token set and empty lists.
4. Panel toggle when no data/outside detail -> deterministic no-op.

## 10. Migration Strategy
1. Additive and opt-in; default behavior remains unchanged.
2. Existing CLI/JSON contracts stay backward compatible.
3. New controls are additive only (`i` in detail mode).

## 11. Test Strategy
1. CLI parse tests for `--tui-http` and `--tui-http-port`.
2. Endpoint tests for `200 /state`, `404 unknown`, `405 non-GET /state`.
3. Snapshot tests for available/unavailable graph states.
4. Mapping tests for direct-id match, fallback overlap, and tie-break behavior.
5. Tests for `selectionNotResolvable` with valid graph.
6. Bind-failure test validating non-fatal continuation.
7. App tests for `i` toggle lifecycle and reset on detail exit.
8. Payload tests for panel state transitions across list/detail mode changes.
9. Render tests for summary token format and bounded expanded lists.
10. Regression tests for existing key behavior.

## 12. Acceptance Criteria
1. With `--tui-http`, `GET /state` returns full snapshot with graph/impact/panel sections.
2. Compact summary appears in detail mode and matches locked format.
3. `i` expands/collapses details panel without breaking navigation.
4. Unavailable graph states are explicit and non-fatal.
5. Disabled or bind-failed HTTP does not break TUI baseline behavior.

## 13. Constraints and Explicit User Preferences
1. HTTP response must include graph and impact data tied to active context.
2. Compact summary must remain concise while supporting expansion for details.
3. Preserve existing operator workflow; all changes are additive.
4. Keep implementation bounded to local, read-only runtime semantics.
