# Phase Task Plan: Diff TUI Full-Entity Toggle

## Status
Locked

## 1. Scope
Deliver a TUI entity-context toggle (`hunk` vs `entity`) with keyboard control (`e`), deterministic hunk navigation in both modes, and shared-footer-cell UX where `e: <mode>` coexists with `m` and optional `r` cells.

## 2. Global Rules
1. Execute phases in strict order: `H0 -> H1 -> H2 -> H3`.
2. Preserve existing default behavior by starting in `hunk` mode.
3. Keep existing keybindings intact except additive `e` toggle.
4. Keep missing-content behavior non-fatal.
5. Run 2 independent external reviews in authoring stage (Gemini + PI).
6. Triage every finding as `accept`, `defer`, or `reject`.
7. Section 9 evidence updates are mandatory.

## 3. Phases, Deliverables, Acceptance

### H0: Contract Lock
Deliverables:
1. Lock mode model (`hunk`, `entity`) and startup default.
2. Lock keyboard semantics for `e` in list/detail.
3. Lock footer cell contract (`e: <mode>`) with shared ordering/coexistence (`m`, `r`, `e`).
4. Lock changed-region definition for entity anchors.
5. Lock toggle-reset semantics in detail (`detail_hunk_index=0`, `detail_scroll=0`).
6. Lock 2x2 anchor behavior matrix: `(hunk|entity) x (unified|side-by-side)`.
7. Lock anchor coordinate space as 0-based rendered row indices in the active view output (`unified_lines` / shared `side_by_side_lines` row stream).

Acceptance:
1. No ambiguity on how `n/p` behaves per mode/view combination.
2. No ambiguity on footer requirements in both list/detail.
3. Design/schema/plan are internally consistent.

Gate:
- GO only when authoring-stage reviews are complete and triaged.

### H1: App State + Toggle Wiring
Deliverables:
1. Add `EntityContextMode` to app state.
2. Wire `e` key handling in list and detail modes.
3. Ensure detail refresh path uses current context mode.
4. Apply toggle-reset semantics in detail mode.
5. Add app-state tests for startup and toggle behavior.

Acceptance:
1. Startup mode is `hunk`.
2. Toggle works from both list and detail.
3. Existing navigation and quit/help behavior are unchanged.

Gate:
- GO only if `cargo test -p sem-cli` passes and toggle tests cover both UI modes.

### H2: Detail Rendering + Anchor Semantics
Deliverables:
1. Implement full-entity render path.
2. Keep grouped-hunk path unchanged for `hunk` mode.
3. Define changed-region anchors as contiguous non-equal diff-op runs.
4. Implement mode- and view-specific anchor generation with dedupe and deterministic ordering.
5. Ensure `n/p` handling uses active mode+view anchor set.

Acceptance:
1. Entity mode displays full entity context.
2. `n/p` jumps deterministically in both modes and both views.
3. No panic on empty or unavailable content.

Gate:
- GO only with passing renderer/navigation tests and manual verification.

### H3: Footer UX + Docs + Hardening
Deliverables:
1. Rework footer to include dedicated `e: <mode>` cell in list/detail within shared footer rail.
2. Update help overlay text with exact line: `e toggle hunk/entity context`.
3. Add/refresh docs and changelog for new toggle.
4. Add hardening tests for identical-content entity mode and non-zero index toggle behavior.
5. Complete Section 9 phase evidence entries.

Acceptance:
1. Footer mode cell is visible and stable in list/detail.
2. Docs match runtime behavior.
3. Evidence complete and triaged findings resolved/deferred/rejected.

Gate:
- GO only with tests, docs, and Section 9 complete.

## 4. Verification Matrix
| Area | Verification | Command / Evidence |
|---|---|---|
| contract consistency | doc review | design/schema/plan cross-check |
| startup mode | app test | `cargo test -p sem-cli` startup-state assertions |
| key handling | app test | list/detail `e` toggle behavior assertions |
| detail toggle reset | app test | non-zero index/scroll resets to `0` on `e` |
| hunk-mode parity | renderer test | grouped-hunk snapshot parity checks |
| entity-mode completeness | renderer test | full-context lines present beyond grouped hunks |
| changed-region semantics | renderer test | contiguous non-equal runs produce anchor heads |
| anchor dedupe/order | renderer test | deterministic anchor vector assertions |
| mode/view matrix | app/detail tests | `(hunk|entity) x (unified|side-by-side)` traversal |
| identical-content entity mode | renderer/app test | unchanged lines + empty anchors + `n/p` boundary no-op |
| placeholder toggle semantics | app/detail test | `e` toggles mode token while placeholder behavior remains non-fatal |
| round-trip toggle behavior | app/detail test | `hunk -> entity -> hunk` preserves deterministic reset/navigation behavior |
| footer cell | render test | `e: hunk` / `e: entity` appears list+detail and coexists with `m` + optional `r` |
| help text | render/help test | `e toggle hunk/entity context` visible |
| regression safety | full tests | `cargo test -p sem-cli && cargo test -p sem-core` |

## 5. Milestone Commit Gate
1. One milestone commit per phase.
2. Commit template:
   - `feat(sem): H0 full-entity toggle contract lock`
   - `feat(sem): H1 entity-context mode state + key wiring`
   - `feat(sem): H2 full-entity render + anchor semantics`
   - `feat(sem): H3 footer mode cell + docs hardening`

## 6. Risks and Mitigations
1. Risk: full-entity rendering reduces scan speed.
- Mitigation: keep default `hunk` mode and fast toggle.
2. Risk: anchor mismatches between modes/views cause confusing jumps.
- Mitigation: explicit mode+view anchor contract and matrix tests.
3. Risk: footer overcrowding degrades readability.
- Mitigation: fixed concise mode cell format and shared cell ordering from footer-cell addendum.
4. Risk: render-path divergence creates regressions.
- Mitigation: parity tests for hunk mode and full regression suite.
5. Risk: very large entity payloads can increase render latency.
- Mitigation: defer line-cap/lazy rendering policy to hardening follow-up after baseline instrumentation.

## 7. Deferred Items
1. File-aggregate detail mode.
2. Persisting entity-context mode preference.
3. Hybrid raw-line fallback mode for parser-sparse entities.
4. Additional keybinding customization.
5. Hard line-cap or lazy-render strategy for very large entities.

## 8. Review Findings Triage Ledger (Authoring Stage)

### Review Run A (generic-gemini)
- Run ID: `r_20260308190302808_7fccc955`
- Findings summary:
  - missing detail-toggle scroll/index behavior
  - changed-region definition ambiguity
  - large-entity performance risk
  - test gaps for in-detail toggle and stress scenarios
- Triage:
  - `accept`:
    - locked detail toggle reset semantics
    - locked changed-region definition for anchor generation
    - expanded verification matrix for in-detail toggle behavior and identical-content anchor-empty path
  - `defer`:
    - explicit large-entity line-cap/lazy-render policy to future hardening follow-up
  - `reject`:
    - none

### Review Run B (generic-pi)
- Run ID: `r_20260308190337804_ac695371`
- Findings summary:
  - "entity" term ambiguity
  - full-entity render path and anchor semantics underdefined
  - side-by-side anchor matrix and toggle-reset behavior underspecified
  - schema definition gaps around `changedRegions`
  - test gaps for matrix behavior and non-zero-index toggle
- Triage:
  - `accept`:
    - added explicit entity definition in design
    - locked changed-region semantics and anchor example
    - locked 2x2 mode/view anchor behavior matrix
    - locked detail toggle reset semantics
    - locked help overlay text and expanded test matrix coverage
  - `defer`:
    - deeper perf strategy (line cap/lazy rendering) to future hardening follow-up
  - `reject`:
    - remove derived `rendered` footer schema field (kept as documentation convenience)

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
  - drafted required artifacts (`design.md`, `phase-task-plan.md`, `schema-proposal.md`)
  - executed 2 independent review runs with stream-confirmed terminal events
  - triaged all findings and applied accepted edits
- Review run IDs + triage outcomes:
  - `r_20260308190302808_7fccc955`: accept + defer, no rejects
  - `r_20260308190337804_ac695371`: accept + defer + reject, reject limited to schema-field removal suggestion
- Go/No-Go: GO
- Notes:
  - review completion confirmed from live session stream terminal events (`result.completed`).

### 9.4 H0 Evidence
- Completion date: 2026-03-08
- Commit hash(es): `af0cda88e7f9d88af794f0ef01574cd0f9a0fafc`
- Acceptance evidence:
  - manual: cross-checked `schema-proposal.md`, `design.md`, and `phase-task-plan.md` for H0 contract consistency and removed side-by-side anchor ambiguity.
  - docs update: locked anchor coordinate space as 0-based rendered row indices in both unified and side-by-side render streams.
  - docs update: locked `e`/`Tab` orthogonality and placeholder/loading toggle semantics.
  - docs update: expanded verification matrix to include placeholder toggle and round-trip mode-toggle coverage.
- Review run IDs + triage outcomes:
  - `r_20260308210406018_40ead61d`:
    - `accept`: no additional edits required beyond explicit lock clarifications already present in this phase.
    - `defer`: large-entity performance policy and extreme-width footer contention strategy remain deferred per Section 7.
    - `reject`: whitespace-specific changed-region interpretation change (contract remains diff-op based and renderer-defined).
  - `r_20260308210439614_c0f1f699`:
    - `accept`: added side-by-side anchor coordinate/scroll semantics, loading/placeholder toggle semantics, and clarified illustrative footer payload context.
    - `defer`: maximum anchor/line-count guard policy remains deferred per Section 7.
    - `reject`: additional prerequisite proof artifact (user handoff already states prerequisite landed and this plan keeps it as explicit handoff requirement).
- Go/No-Go: GO
- Notes:
  - both reviews completed with stream terminal events (`result.completed`).

### 9.5 H1 Evidence
- Completion date: 2026-03-08
- Commit hash(es): `32b26cd2504900cbbee7bf514b05c7d5a0e6eb39`
- Acceptance evidence:
  - `cargo test -p sem-cli` (run from `crates/`) => pass (`110 passed`, `0 failed`).
  - manual: verified startup defaults to `hunk`, `e` toggles in list/detail, and detail-mode toggles reset `detail_hunk_index` + `detail_scroll` to `0`.
  - tests added in `tui/app.rs` for list/detail toggle behavior, detail round-trip reset, non-first-entity stability, and unavailable-content toggle safety.
- Review run IDs + triage outcomes:
  - `r_20260308210942672_b3c50e2d`:
    - `accept`: none in-scope beyond H1 wiring/tests already delivered.
    - `defer`: full-entity rendering path and changed-region anchors (H2 scope).
    - `reject`: none.
  - `r_20260308211110786_7c529e84`:
    - `accept`: added detail-mode round-trip reset, non-first-entity stability, and unavailable-content toggle tests.
    - `defer`: dead-code warning on `EntityContextMode::as_token` until footer integration in H3.
    - `reject`: none.
- Go/No-Go: GO
- Notes:
  - both reviews completed with stream terminal events (`result.completed`).

### 9.6 H2 Evidence
- Completion date: 2026-03-08
- Commit hash(es): `4b2797585990504cc7329bb612c756d5d44026c2`
- Acceptance evidence:
  - `cargo test -p sem-cli` (run from `crates/`) => pass (`116 passed`, `0 failed`).
  - renderer: `EntityContextMode::Entity` now uses full-entity line stream with changed-region anchor generation for unified and side-by-side outputs.
  - navigation: app tests validate active-anchor traversal for `(entity, unified)` and `(entity, side-by-side)` plus deterministic boundary no-op behavior.
  - hardening: added contiguous multi-line dedupe test and added-content anchor test for entity mode.
- Review run IDs + triage outcomes:
  - `r_20260308211620614_9881f860`:
    - `accept`: no additional changes required.
    - `defer`: none.
    - `reject`: none.
  - `r_20260308211716115_546d82de`:
    - `accept`: added RenderedDiff anchor-field clarity comments, contiguous-region dedupe coverage, entity-mode last-anchor boundary assertions, and added-content anchor coverage.
    - `defer`: footer `e` cell/help text integration and render-frame/footer hardening coverage to H3 scope.
    - `reject`: change toggle-reset semantics to snap to first anchor after `e` (kept reset contract `detail_scroll=0`), and treat identical-content hunk mode as full-entity rendering (hunk-mode parity remains locked to grouped behavior).
- Go/No-Go: GO
- Notes:
  - both reviews completed with stream terminal events (`result.completed`).

### 9.7 H3 Evidence
- Completion date: 2026-03-08
- Commit hash(es): `1b755987727d90de86dd9ac0556dd24d256b4ebd`
- Acceptance evidence:
  - `cargo test -p sem-cli && cargo test -p sem-core` (run from `crates/`) => pass (`121` sem-cli tests, `41` sem-core tests, `0` failures).
  - footer UX: `render.rs` now emits `e: <hunk|entity>` in list/detail footer cells with shared ordering `m | r | e`.
  - help text: overlay includes exact line `e toggle hunk/entity context` and list/detail overlay tests assert visibility.
  - docs/changelog: updated `crates/README.md` keybindings and `CHANGELOG.md` feature/testing notes for entity-context toggle and hardening coverage.
  - hardening tests: added draw-level footer visibility assertion for `e: entity` and realistic-width footer contention guard.
- Review run IDs + triage outcomes:
  - `r_20260308212204531_fc9c7aad`:
    - `accept`: added explicit H3 Section 9 evidence entry and final triage logging.
    - `defer`: none.
    - `reject`: none.
  - `r_20260308212248713_cf45ac6d`:
    - `accept`: added detail-mode help-overlay assertion, draw-level `e: entity` footer assertion, realistic-width footer layout test, and changelog testing specificity update.
    - `defer`: wording cleanup for historical \"optional `r`\" phrase in verification matrix.
    - `reject`: force controls/help wording to match exactly (`e context` in footer controls remains intentional for compactness).
- Go/No-Go: GO
- Notes:
  - both reviews completed with stream terminal events (`result.completed`).
  - `docs/reference/architecture.md` and `docs/implementation/implementation-plan.md` do not exist in this repository; finalization docs were applied to available canonical docs (`crates/README.md`, `CHANGELOG.md`, and this phase ledger).

## 10. Execution Handoff Contract
0. Prerequisite:
   - `diff-tui-footer-cell-layout` implementation is landed (shared footer cell baseline and ordering contract).
1. Required read order:
   1) `docs/implementation/diff-tui-full-entity-toggle/schema-proposal.md`
   2) `docs/implementation/diff-tui-full-entity-toggle/design.md`
   3) `docs/implementation/diff-tui-full-entity-toggle/phase-task-plan.md`
2. Start at `H0`.
3. Execute phases in strict order using this plan as source of truth for scope, gates, review/triage policy, and Section 9 evidence updates.
4. Completion requirements:
   - update docs/reference if behavior stabilizes
   - add changelog milestone entry
   - complete Section 9 evidence for all phases
