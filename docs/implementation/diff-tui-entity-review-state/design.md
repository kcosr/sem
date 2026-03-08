# Design: Diff TUI Entity Review State

## Status
Locked

## 1. Purpose
Add entity-level reviewed state in TUI with keyboard toggling, filtering, and persistent metadata that carries across ranges/modes when entity output content is unchanged.

## 2. Problem Statement
The TUI currently has no way to mark progress. Operators reviewing large ranges must manually track what is already reviewed. This causes repeated scanning and context loss across sessions and across stepping/range changes.

## 3. Goals
1. Support entity-level `reviewed` toggle via `Space` in both list and detail modes.
2. Provide 3-state visibility filter: `all`, `unreviewed`, `reviewed`.
3. Persist reviewed state to local repo metadata.
4. Carry reviewed state across ranges/modes when entity identity and target content hash match.
5. Keep navigation deterministic when filters hide rows.

## 4. Non-Goals
1. No hunk-level reviewed state in v1.
2. No annotation authoring/rendering in v1.
3. No range/cursor/session resume restoration in v1.
4. No cross-repo or cloud sync in v1.

## 5. Current Baseline
1. TUI supports entity navigation, detail mode, and commit/range stepping.
2. Unified stepping model provides `pairwise`/`cumulative` modes with comparator endpoint semantics and mode indicator/footer token.
3. Unified endpoint paths can include commits plus synthetic `INDEX` and `WORKING` endpoints.
4. No reviewed metadata exists in app state or on disk.
5. List rendering groups entities by file and supports selection-based navigation.

## 6. Key Decisions
1. Reviewed state is entity-level only in v1.
2. Toggle key is `Space` in list and detail modes.
3. Filter key cycles `all -> unreviewed -> reviewed -> all` (binding finalized in H0; proposed `r`).
4. Filter semantics:
   - rows not matching filter are hidden
   - file header is hidden if file has zero visible entities
   - if zero visible entities globally, show deterministic empty-state row
5. Navigation semantics under filter:
   - list up/down and detail left/right skip hidden rows
   - if no visible rows, movement and enter are no-op
6. Review identity key for persistence is composite:
   - `logicalEntityKey`: stable entity identity key string
   - `targetContentHash`: normalized hash of comparator target-side entity content
   - reviewed carryover requires exact match on both values
7. `logicalEntityKey` v1 grammar is locked:
   - preferred: `entityId::<entity_id>` when parser provides stable `entity_id`
   - fallback: `fallback::<canonicalPath>::<entityType>::<entityName>::<occurrenceOrdinal>`
   - `occurrenceOrdinal` is deterministic index within same `(canonicalPath, entityType, entityName)` group using semantic emission order for current snapshot
8. `targetContentHash` normalization is locked:
   - input is comparator target-side entity text (for delete: pre-state entity text)
   - normalize line endings to `\n`
   - preserve internal whitespace and indentation
   - trim trailing `\n` run to a single terminal `\n`
   - hash algorithm: `sha256` of normalized UTF-8 bytes, encoded as `sha256:<hex>`
9. Deleted entities use deleted-target hash material from pre-state content and are still stored in `targetContentHash` using same normalization/hash format.
10. Persistence file path is local repo metadata:
   - `.sem/tui-review-state.json`
11. Persisted data v1:
   - reviewed records (required)
   - UI review filter preference (`reviewFilter`) allowed
   - no persisted cursor/range resume
12. Persistence semantics:
   - unreview removes record (no explicit `reviewed=false` record retained)
   - writes are atomic (temp + rename)
   - writes are debounced (`<=500ms`) and flushed on exit
   - startup compaction deduplicates by key and enforces max-record cap (`20,000`, drop oldest by `updatedAt`)
13. Multi-instance behavior in v1 is accepted limitation:
   - last-writer-wins for concurrent sessions
14. Repo binding:
   - `repoId` is hash of canonical repo root path (`sha256:...`)
   - mismatch on load is non-fatal and file contents are ignored for session
15. Cross-range/mode behavior:
   - reviewed status is shown whenever identity key matches
   - no automatic relevance suppression in v1
16. Review-state behavior is orthogonal to stepping:
   - `Space`/filter actions do not modify step cursor, step mode, or comparator endpoint selection
   - `[`/`]` and `m` behavior stays unchanged
17. Hash source lock under unified stepping:
   - `targetContentHash` uses active comparator target endpoint content (`comparison.toEndpointId`)
   - valid target endpoint kinds include commit endpoint IDs, `index`, and `working`
18. Footer composition lock:
   - preserve `m: <pairwise|cumulative>` cell
   - add review filter cell as `r: <all|unreviewed|reviewed>`
   - delimit cells using shared ` | ` separator contract
   - keep status text in dedicated status slot (separate from cell values)
   - under constrained width, status truncates/omits before cell eviction
   - do not remove or reorder reserved `e` position

## 7. Contract / Interface Semantics
This is a CLI/TUI contract (not HTTP).

### 7.1 Keyboard Contract
1. `Space`: toggle reviewed on focused entity.
2. `r` (proposed): cycle filter state.
3. Existing navigation keys remain unchanged.
4. In detail mode, `Space` applies to the currently opened entity.
5. Stepping/mode keys (`[`/`]`, `m`) remain unchanged and independent of review actions.

### 7.2 Filter Contract
1. `all`: show every entity.
2. `unreviewed`: show only entities without matching reviewed record.
3. `reviewed`: show only entities with matching reviewed record.
4. Empty filtered result produces explicit no-match state; does not auto-reset filter.
5. Footer filter state is rendered as `r: <state>`.
6. Footer cell rendering inherits shared separator and ordering behavior from footer-cell contract.

### 7.3 Persistence Contract
1. File path: `.sem/tui-review-state.json`.
2. Writes are atomic (temp + rename) to avoid corruption.
3. Missing/corrupt/version-mismatch/repo-mismatch file is non-fatal; app continues with empty in-memory state and status hint.
4. H3 docs include guidance that `.sem/tui-review-state.json` is local state and should not be committed.

## 8. Service / Module Design
1. `tui/app.rs`
   - add reviewed map and filter state
   - add toggle/filter actions and visible-row projection
2. `tui/render.rs`
   - add reviewed marker in rows
   - add review filter footer cell `r: <state>` while preserving `m` cell
   - render filtered-empty state
3. `commands/diff.rs` / TUI controller
   - compute target content hash for focused row from current comparator target endpoint content
   - support comparator target endpoint IDs across commit/index/working
4. persistence module (new)
   - load/save `.sem/tui-review-state.json`
   - schema version checks + migration hook entry point

## 9. Error Semantics
1. Persistence read error: non-fatal, start with empty state, show warning hint.
2. Persistence write error: non-fatal, keep in-memory state for session, show warning hint.
3. Hash material unavailable: non-fatal; entity considered unreviewed with status hint.

## 10. Migration Strategy
1. New feature is additive and TUI-only.
2. No changes to non-TUI output contracts.
3. Persistence file creation is lazy on first state mutation.
4. Future schema versions:
   - unknown version => ignore file in v1
   - v2+ must provide explicit one-way migration or reset policy in its own design lock.
5. Footer behavior aligns with `docs/implementation/diff-tui-footer-cell-layout/` addendum.

## 11. Test Strategy
1. App-state tests:
   - space toggle in list/detail
   - filter cycling and projection
   - hidden-row navigation skip behavior
   - no-op behavior when no visible rows
2. Identity/hash tests:
   - carryover across range/mode when identity+hash match
   - no carryover when hash differs
   - fallback `logicalEntityKey` construction and collision grouping behavior
   - deleted-entity hash path behavior
3. Persistence tests:
   - load missing/corrupt/version-mismatch/repo-mismatch file behavior
   - atomic write behavior
   - compaction/dedupe/max-record cap
   - multi-writer last-write-wins expectation documented by test note
4. Render tests:
   - reviewed markers
   - file header suppression when no visible entities
   - global filtered-empty state
5. Scale smoke:
   - load/save responsiveness with `>=10,000` records.
6. Unified stepping compatibility tests:
   - reviewed carryover behavior in both `pairwise` and `cumulative` modes
   - reviewed carryover behavior when comparator target endpoint is commit/index/working

## 12. Acceptance Criteria
1. Operator can toggle reviewed state with `Space` from list and detail.
2. Filter states (`all/unreviewed/reviewed`) work with deterministic row visibility.
3. Reviewed records persist across sessions.
4. Reviewed carryover works only when entity identity and target content hash match.
5. Empty filtered views are stable and non-crashing.
6. Footer shows `r: all|unreviewed|reviewed` alongside existing mode cell contract.

## 13. Constraints and Explicit User Preferences
1. Entity-level only in v1; hunk-level deferred.
2. No automatic hiding of potentially stale context by relevance assumptions.
3. Filter/no-match behavior must be explicit and no-op, not auto-reset.
4. This topic is intentionally separate from annotations design.
