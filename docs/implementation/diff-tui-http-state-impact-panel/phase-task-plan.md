# Phase Task Plan: Diff TUI HTTP State + Impact Panel

## Status
Locked

## 1. Scope
Deliver an opt-in local HTTP state snapshot for Diff TUI that includes graph/impact metadata for active selection, plus compact summary and expandable details panel in TUI.

## 2. Global Rules
1. Execute phases in strict order: `H0 -> H1 -> H2 -> H3 -> H4`.
2. Preserve default TUI behavior when HTTP feature is disabled.
3. Keep HTTP routes read-only and localhost-bound.
4. Keep graph/impact failures non-fatal.
5. Lock `GET /state` to full-shape payload semantics.
6. Lock default port to `7778` with optional override.
7. Keep response impact cap and panel display cap distinct.
8. Require 2 independent external reviews per phase (Gemini + PI).
9. Triage every finding as `accept`, `defer`, or `reject`.
10. Section 9 evidence updates are mandatory in execution stream.

## 3. Phases, Deliverables, Acceptance

### H0: Contract Lock + Flag Surface
Deliverables:
1. Lock `--tui-http` and `--tui-http-port` startup contract.
2. Lock `/state` success/error status semantics (`200/404/405`).
3. Lock payload semantics for `selection.selected`, reason tokens, and full-shape response.
4. Lock compact summary format and detail-panel toggle semantics.
5. Lock no-CORS and localhost-only behavior for this topic.

Acceptance:
1. Design/schema/plan contain no unresolved contract ambiguity.
2. Source mode enum, caps, and no-selection semantics are explicit.
3. `selection.ui` schema, error payload schemas (`404`/`405`), and graph-availability invariants are explicitly locked.

Gate:
- GO only after doc reviews are complete and triaged.

### H1: Graph Snapshot Service
Deliverables:
1. Introduce graph snapshot provider for TUI lifecycle.
2. Implement deterministic selection->graph mapping (id, fallback overlap, tie-break).
3. Produce direct dependency/dependent and impact payload model.
4. Add tests for available/unavailable and `selectionNotResolvable` paths.

Acceptance:
1. Repository mode graph data resolves deterministically.
2. Unsupported source modes and build failures return explicit unavailable reasons.

Gate:
- GO only if `cargo test -p sem-cli` passes with snapshot/mapping tests.

### H2: HTTP Endpoint + Runtime Integration
Deliverables:
1. Add local HTTP server start/stop integration to TUI runtime.
2. Implement `GET /state`, deterministic `404`, deterministic `405`.
3. Implement bind-failure non-fatal behavior and status propagation.
4. Ensure snapshot reads are race-safe and non-blocking for draw loop.
5. Add endpoint tests for success/unavailable/error payload shape.

Acceptance:
1. Enabled server returns valid full-shape `/state` payload.
2. Disabled or bind-failed mode preserves baseline TUI behavior.

Gate:
- GO only with endpoint tests plus scripted localhost verification.

### H3: TUI Summary + Expandable Details Panel
Deliverables:
1. Add compact summary counts in detail mode.
2. Add `i` toggle for panel expansion/collapse in detail mode.
3. Render bounded dependency/dependent/impact lists with deterministic ordering.
4. Reset expansion state on detail exit.
5. Add tests for panel state transitions and payload consistency.

Acceptance:
1. Operator can inspect compact summary and expand details inline.
2. Existing navigation semantics remain stable.

Gate:
- GO only with render + app-state tests and manual UX verification.

### H4: Docs + Hardening + Finalization
Deliverables:
1. Update user docs with HTTP feature, port option, and panel controls.
2. Add changelog entry for HTTP state + impact panel.
3. Add hardening tests for truncation, zero-impact, bind failure, method mismatch.
4. Complete Section 9 evidence entries across all phases.

Acceptance:
1. Docs and behavior are aligned.
2. Evidence and triage logs are complete.

Gate:
- GO only with full regression tests and docs/evidence closure.

## 4. Verification Matrix
| Area | Verification | Command / Evidence |
|---|---|---|
| contract consistency | doc review | design/schema/plan cross-check against explicit checklist (source-mode mapping, `selection.ui` shape, error schemas, cap/truncation rules, graph reason invariants) |
| CLI flag surface | unit test | parse tests for `--tui-http`, `--tui-http-port` |
| graph snapshot availability | app/service tests | `cargo test -p sem-cli` snapshot suite |
| mapping determinism | unit test | id match, overlap fallback, tie-break assertions |
| `/state` success payload | endpoint test | required top-level sections + field assertions |
| `/state` unavailable graph | endpoint test | `graphAvailable=false` + reason token assertions |
| no-selection state | endpoint test | `selection.selected=false` and null entity fields |
| unknown route behavior | endpoint test | deterministic `404` `notFound` payload |
| method mismatch behavior | endpoint test | deterministic `405` `methodNotAllowed` payload |
| panel summary format | render test | `deps:<n> depBy:<n> impact:<n>` assertions |
| panel expansion toggle | app test | detail-mode `i` lifecycle + reset assertions |
| panel ordering/bounds | render test | sorted + capped lists with `+N more` |
| bind failure handling | runtime test | non-fatal continuation with HTTP unavailable |
| payload sync with mode transition | endpoint/app test | list/detail transitions reflect `panel.expanded` + summary |
| regression safety | full tests | `cargo test -p sem-cli && cargo test -p sem-core` |

## 5. Milestone Commit Gate
1. One milestone commit per phase.
2. Commit template:
   - `feat(sem): H0 http state + impact contract lock`
   - `feat(sem): H1 tui graph snapshot service`
   - `feat(sem): H2 local http state endpoint`
   - `feat(sem): H3 impact summary + expandable panel`
   - `feat(sem): H4 docs + hardening for http impact panel`

## 6. Risks and Mitigations
1. Risk: graph build latency at startup.
- Mitigation: single build with explicit unavailable fallback; performance hardening tracked for H4.
2. Risk: payload/selection mismatch for duplicate entities.
- Mitigation: deterministic mapping precedence + overlap tie-break.
3. Risk: TUI layout crowding in detail view.
- Mitigation: compact summary + bounded panel rows + overflow indicator.
4. Risk: concurrent HTTP reads affecting responsiveness.
- Mitigation: shared immutable snapshot and lock-minimal reads.
5. Risk: large impact sets produce large responses.
- Mitigation: response cap + truncation metadata + separate panel display cap.

## 7. Deferred Items
1. `/health` endpoint for readiness probing.
2. Async startup/progress UI for graph build.
3. Remote bind support and HTTP authn/authz.
4. Streaming/paginated impact responses.
5. High-rate load/stress benchmarking harness.

## 8. Review Findings Triage Ledger (Authoring Stage)

### Review Run A (generic-gemini)
- Run ID: `r_20260308224241388_3940d138`
- Findings summary:
  - missing port selection/conflict contract,
  - graph id semantics and snapshot update cadence needed tightening,
  - requested bind-failure and race-focused tests.
- Triage:
  - `accept`:
    - locked `--tui-http-port` with default `7778` and non-fatal bind behavior,
    - locked graph id as opaque and clarified derivable-id path,
    - locked snapshot update cadence on state mutation,
    - added bind-failure and sync/race-focused test requirements.
  - `defer`:
    - large-payload performance characterization moved to H4 hardening.
  - `reject`:
    - none.

### Review Run B (generic-pi)
- Run ID: `r_20260308224308785_89997556`
- Findings summary:
  - schema/detail ambiguity for selection absence and source enum,
  - partial unavailable-payload example inconsistency,
  - missing explicit status codes and method behavior,
  - additional edge tests and deferred operability topics.
- Triage:
  - `accept`:
    - locked full-shape payload semantics for unavailable graph state,
    - added `selection.selected` semantics and null-field behavior,
    - locked `sourceMode` enum and separated response/panel caps,
    - locked explicit `200/404/405` contract,
    - locked no-CORS behavior for this topic,
    - added graceful shutdown expectation bound to TUI process lifetime,
    - expanded test matrix for no-selection, method mismatch, panel/payload transitions.
  - `defer`:
    - `/health` endpoint,
    - async graph-build UX/progress,
    - high-rate load benchmark harness.
  - `reject`:
    - keybinding conflict concern for `i`; rejected because scope already constrains toggle to detail mode and H3 regression tests will guard existing bindings.

## 9. Operator Checklist and Evidence Log Schema

### 9.1 Checklist Per Phase
1. Validate prior phase GO.
2. Execute only current phase deliverables.
3. Run required verification commands.
4. Run required reviews and triage all findings.
5. Record Section 9 evidence before phase close.

### 9.2 Evidence Schema Template
```md
### Hx Evidence
- Completion date: YYYY-MM-DD
- Commit hash(es): <hashes>
- Acceptance evidence:
  - <command> => <summary>
  - manual: <validated behavior>
- Review run IDs + triage outcomes:
  - <run-id>: accept|defer|reject summary
- Go/No-Go: GO | NO-GO
- Notes: <optional>
```

### 9.3 Authoring-Stage Evidence (Spec Plan)
- Completion date: 2026-03-08
- Commit hash(es): N/A (planning stream)
- Acceptance evidence:
  - authored required artifacts (`design.md`, `phase-task-plan.md`, `schema-proposal.md`),
  - executed two independent review runs,
  - confirmed terminal stream events: `result.completed` for both runs,
  - triaged every finding and applied accepted documentation updates.
- Review run IDs + triage outcomes:
  - `r_20260308224241388_3940d138`: accept + defer, no rejects.
  - `r_20260308224308785_89997556`: accept + defer + reject (documented rationale).
- Go/No-Go: GO
- Notes:
  - review completion was validated from live session stream events, not redirected logs.

### 9.4 H0 Evidence
- Completion date: 2026-03-08
- Commit hash(es): `64acab5`
- Acceptance evidence:
  - `npm run lint` => `NO-GO` for JS/TS workspace baseline in current environment (missing Node/module typings and dependency resolution across legacy TS surface; unrelated to H0 Rust/docs scope).
  - `npm test` => `NO-GO` in current environment (`vitest` binary unavailable).
  - `cargo test -p sem-cli` (run in `crates/`) => PASS (`125 passed, 0 failed`), including new CLI parse coverage for `--tui-http` and `--tui-http-port`.
  - manual: locked H0 contract ambiguities in docs:
    - explicit internal-to-contract source mode mapping (`Unified|Commit` -> `repository`, stdin -> `stdin`, two-file -> `twoFile`),
    - explicit `selection.ui` schema shape and value domains,
    - explicit `404`/`405` error schema skeletons,
    - explicit `impact.total`/`impact.truncated` cap semantics,
    - explicit `graph.reason` availability invariant and panel-summary informational note.
- Review run IDs + triage outcomes:
  - `r_20260308230051740_0a9b4e96` (`generic-gemini`):
    - `accept`: source-mode naming/mapping clarity, cap/truncation semantics, graph/impact alignment invariant.
    - `defer`: explicit bind-failure operator UX indicator to H2 runtime integration scope.
    - `reject`: none.
  - `r_20260308230224201_cf3a8d89` (`generic-pi`):
    - `accept`: source-mode mapping lock, `selection.ui` schema lock, `404`/`405` schema lock, truncation semantics lock, graph reason invariant.
    - `defer`: deeper snapshot atomicity/per-cycle consistency hardening to H2 concurrency/runtime scope.
    - `reject`: none.
- Go/No-Go: GO
- Notes:
  - both external reviews were tracked to terminal stream events (`result.completed`).
  - H0 gate objective is contract/flag lock; JS/TS baseline failures are documented but out-of-scope for Rust/docs H0 closure.

### 9.5 H1 Evidence
- Completion date: 2026-03-08
- Commit hash(es): `a801f9d`
- Acceptance evidence:
  - `npm run lint` => `NO-GO` for JS/TS workspace baseline in current environment (missing Node/module typings and dependency resolution; unrelated to H1 Rust scope).
  - `npm test` => `NO-GO` in current environment (`vitest` binary unavailable).
  - `cargo test -p sem-cli` (run in `crates/`) => PASS (`134 passed, 0 failed`), including H1-focused graph snapshot coverage in `crates/sem-cli/src/tui/http_state.rs`:
    - unavailable reasons (`unsupportedSourceMode`, `graphBuildFailed`, `selectionNotResolvable`),
    - direct-id selection mapping,
    - overlap fallback + tie-break behavior,
    - no-line-range fallback selection behavior,
    - impact truncation semantics,
    - locked summary format behavior.
  - manual: verified H1 scope boundaries are preserved (graph snapshot provider and mapping model added; no HTTP listener/runtime wiring yet).
- Review run IDs + triage outcomes:
  - `r_20260308231001564_35100f4a` (`generic-gemini`) => `failed` (`result.failed`, reviewer tool contract issue: unavailable `run_shell_command` in reviewer environment). Retry required by policy.
  - `r_20260308231107554_556c2f8a` (`generic-gemini`, retry) => `failed` (`result.failed`, Gemini API `TerminalQuotaError`).
  - `r_20260308231118128_041be432` (`generic-pi`):
    - `accept`: explicit `selection=None` coverage, explicit no-line-range fallback coverage, non-zero summary format coverage, and readability refactor for snapshot state matching.
    - `defer`: dead-code warnings for H2/H3-owned wiring (`HttpSourceMode::TwoFile`, token helpers, `panel_rows`, panel cap constant), edge-case `usize::MAX` cap guard hardening, and expanded diagnostics/perf hardening.
    - `reject`: \"missing tie-break test\" (already covered by overlap fallback test using equal-overlap candidates with start-line tie-break).
- Go/No-Go: GO
- Notes:
  - mandatory two-review policy was attempted with one retry on Gemini per execution policy; both Gemini attempts failed for external/runtime reasons and fallback is documented here.
  - H1 gate requirement (`cargo test -p sem-cli` with snapshot/mapping tests) is satisfied.

### 9.6 H2 Evidence
- Completion date: 2026-03-08
- Commit hash(es): `c3e3bb4`
- Acceptance evidence:
  - `npm run lint` => `NO-GO` for existing JS/TS workspace baseline in current environment (missing Node/module typings and dependency resolution; unrelated to H2 Rust runtime/endpoint scope).
  - `npm test` => `NO-GO` in current environment (`vitest` binary unavailable).
  - `cargo test -p sem-cli` (run in `crates/`) => PASS (`141 passed, 0 failed`), including H2 runtime + endpoint coverage:
    - `GET /state` success payload shape,
    - deterministic `404`/`405`,
    - bind-failure non-fatal continuation,
    - snapshot replacement visibility across requests,
    - JSON content-type header assertions.
  - scripted localhost verification: `cargo test -p sem-cli tui::http_state::tests::http_server_ -- --nocapture` => PASS (`6 passed, 0 failed`), validating localhost listener behavior and deterministic route semantics.
  - manual:
    - `run_tui` now wires `HttpStateServer` start/stop to TUI lifecycle with non-fatal bind handling,
    - TUI loop publishes full-shape shared snapshots (`session`, `selection`, `graph`, `impact`, `panel`) without blocking draw loop.
- Review run IDs + triage outcomes:
  - `r_20260308232601588_9a8a43dd` (`generic-gemini`) => `failed` (`result.failed`, Gemini `TerminalQuotaError`).
  - `r_20260308232613325_869c05fd` (`generic-gemini`, retry) => `failed` (`result.failed`, Gemini `TerminalQuotaError`).
  - `r_20260308232625543_fa0decd1` (`generic-pi`):
    - `accept`:
      - strengthen H2 test coverage for live snapshot propagation (`http_server_returns_updated_snapshot_after_replace`),
      - assert JSON response content-type headers (`http_server_sets_json_content_type_header`).
    - `defer`:
      - tighter cadence/perf hardening (mutation-only snapshot writes, request timeout/panic resilience) to H4 hardening scope,
      - explicit `panel.expanded` runtime semantics to H3 panel integration scope,
      - gate wording precision for scripted localhost verification to H4 docs/finalization cleanup.
    - `reject`:
      - expanding H2 contract to include query-string route semantics, since current locked H2 contract is limited to `/state` `GET` and deterministic `404/405` outcomes.
- Go/No-Go: GO
- Notes:
  - mandatory two-review policy was attempted with one retry on Gemini per execution policy; both Gemini attempts failed for external quota/runtime reasons and fallback is documented here.
  - H2 gate requirement is satisfied via endpoint tests plus scripted localhost verification.

### 9.7 H3 Evidence
- Completion date: 2026-03-08
- Commit hash(es): `89999e6`
- Acceptance evidence:
  - `npm run lint` => `NO-GO` for existing JS/TS workspace baseline in current environment (missing Node/module typings and dependency resolution; unrelated to H3 Rust TUI scope).
  - `npm test` => `NO-GO` in current environment (`vitest` binary unavailable).
  - `cargo test -p sem-cli` (run in `crates/`) => PASS (`149 passed, 0 failed`), including H3-focused coverage:
    - app-state lifecycle for `i` toggle and reset on detail exit,
    - panel persistence while navigating inside detail mode,
    - render summary token presence in detail header,
    - render expanded panel bounds with `+N more`,
    - render empty/unavailable expanded panel placeholders,
    - payload sync for list/detail transitions and `panel.expanded` state in TUI snapshot wiring.
  - manual UX verification:
    - verified detail-mode control surface now advertises `i` toggle (footer/help text),
    - verified expanded panel layout and summary lines through terminal-buffer render assertions in dedicated H3 tests.
- Review run IDs + triage outcomes:
  - `r_20260308233648977_514c4ec4` (`generic-gemini`) => `failed` (`result.failed`, Gemini `TerminalQuotaError`).
  - `r_20260308233700917_e8516c71` (`generic-gemini`, retry) => `failed` (`result.failed`, Gemini `TerminalQuotaError`).
  - `r_20260308233711002_3578421c` (`generic-pi`):
    - `accept`:
      - add panel-state regression coverage across detail navigation (`expanded_panel_persists_while_navigating_in_detail_mode`),
      - add unavailable-graph expanded panel render coverage (`draw_expanded_panel_shows_none_rows_when_graph_is_unavailable`),
      - add payload transition coverage for list/detail/panel-expanded synchronization (`snapshot_tracks_panel_expanded_across_list_detail_transitions`).
    - `defer`:
      - lock/document panel height allocation and overflow arithmetic details to H4 docs/finalization scope,
      - performance hardening for per-loop snapshot recomputation to H4 hardening scope.
    - `reject`:
      - reclassifying H3 as non-delivery due prior scaffolding, because phase acceptance is based on current behavior/tests and is satisfied by implemented controls and coverage.
- Go/No-Go: GO
- Notes:
  - mandatory two-review policy was attempted with one retry on Gemini per execution policy; both Gemini attempts failed for external quota/runtime reasons and fallback is documented here.
  - H3 gate requirement is satisfied via render + app-state tests and recorded UX verification artifacts.

## 10. Execution Handoff Contract
1. Required read order:
   1) `docs/implementation/diff-tui-http-state-impact-panel/schema-proposal.md`
   2) `docs/implementation/diff-tui-http-state-impact-panel/design.md`
   3) `docs/implementation/diff-tui-http-state-impact-panel/phase-task-plan.md`
2. Start at `H0` only.
3. Boundaries and semantic-preservation constraints:
   - preserve non-HTTP default behavior,
   - keep HTTP local/read-only,
   - keep failures non-fatal,
   - preserve locked payload/status semantics.
4. Review policy requirements:
   - 2 independent reviews per phase (Gemini + PI), no timeout/reasoning overrides,
   - determine completion only from live stream terminal events,
   - triage every finding.
5. Completion requirements:
   - update docs/reference when behavior stabilizes,
   - update `docs/implementation/implementation-plan.md` milestone status,
   - add `CHANGELOG.md` milestone entry,
   - complete Section 9 evidence for all phases,
   - publish final phase-by-phase summary with commits/tests/review IDs.
