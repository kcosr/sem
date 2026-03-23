# Phase Task Plan: Diff TUI Entity Annotations

## Status
Draft

## 1. Scope
Deliver per-entity text annotations with keyboard-driven inline input, provenance-aware display with hash-divergence hints, 3-state annotation filter composing with the existing review filter, and persistent local annotation metadata integrated into the existing review-state file. Annotations match on `logicalEntityKey` only and carry across stepping modes.

## 2. Global Rules
1. Execute phases in strict order: `A0 -> A1 -> A2 -> A3 -> A4`.
2. Preserve existing review-state, stepping, and footer semantics.
3. Keep entity-level scope only (no hunk-level or line-level annotations).
4. Keep all persistence failures non-fatal.
5. Preserve unified stepping semantics (`[`/`]`, `m`, comparator endpoints, and step-mode indicator token).
6. Annotation key generation must not be gated by `endpoint_supports_review_hash()`.
7. No automatic compaction or eviction of annotations.
8. Section 8 evidence updates are mandatory.

## 3. Prerequisites
1. `diff-tui-entity-review-state` implementation is landed (review state persistence, identity helpers, filter infrastructure).
2. `ReviewStateUiPrefs` supports persisting view mode, diff view, and entity context mode preferences (landed in current baseline).

## 4. Phases, Deliverables, Acceptance

### A0: Contract Lock
Deliverables:
1. Lock annotation identity semantics: `logicalEntityKey` only, no content hash in lookup key.
2. Lock `contentHashAtCreation` as optional provenance metadata.
3. Lock keybindings: `a` (add/replace), `D` (delete), `A` (cycle annotation filter).
4. Lock annotation filter: `All -> Annotated -> Unannotated` cycle, composing with review filter.
5. Lock persistence shape: `annotations` array in existing `.sem/tui-review-state.json`, optional field, no version bump.
6. Lock display contract: `[*]` badge (hash match or unavailable), `[~]` badge (hash divergent), "different version" label in detail/preview.
7. Lock input mode contract: single-line, 256-char max, Enter confirms, Esc cancels, whitespace-only treated as cancel.
8. Lock stepping-during-input behavior: cancel input on snapshot arrival.
9. Lock annotatability rule: entity is annotatable if `logicalEntityKey` resolves, regardless of hash material availability.
10. Verify design/schema/plan cross-consistency.

Acceptance:
1. No ambiguity on annotation identity, matching, display, or input semantics.
2. Docs internally consistent (`design.md`, `schema-proposal.md`, `phase-task-plan.md`).

Gate:
- GO only when contract docs are consistent and all decisions are locked.

### A1: Persistence Foundation
Deliverables:
1. Add `Annotation` struct and `PersistedAnnotationRecord` to `review_state.rs`.
2. Add `AnnotationFilter` enum with `cycle()` and `as_token()` to `review_state.rs`.
3. Extend `ReviewStateData` with `annotation_filter: AnnotationFilter` and `annotations: HashMap<String, Annotation>`.
4. Extend `PersistedReviewState` with optional `annotations` array (`#[serde(default)]`).
5. Extend `PersistedUiPrefs` with optional `annotationFilter` field.
6. Update `ReviewStateStore::load()` to deserialize annotations into HashMap keyed by `logicalEntityKey`, with last-writer-wins for duplicate keys.
7. Update `ReviewStateStore::save()` to serialize annotations array sorted by `logicalEntityKey`.
8. Unit tests:
   - Annotation persistence round-trip (save then load).
   - Load file with annotations but no review records and vice versa.
   - Load file without `annotations` field (backward compat, defaults to empty).
   - Duplicate `logicalEntityKey` on load: last-writer-wins.
   - `AnnotationFilter` cycle covers all 3 states.
   - `contentHashAtCreation` absent (None) round-trips correctly.

Acceptance:
1. Existing review-state behavior unchanged — files without `annotations` load cleanly.
2. Persistence read/write failures remain non-fatal and visible.
3. `cargo test -p sem-cli` passes with new annotation persistence tests.

Gate:
- GO only if tests pass and existing review-state tests remain green.

### A2: AppState Integration
Deliverables:
1. Add `annotations: HashMap<String, Annotation>` to `AppState`.
2. Add `row_annotation_keys: Vec<Option<String>>` to `AppState`.
3. Add `annotation_filter: AnnotationFilter` to `AppState`.
4. Add `annotation_input: Option<AnnotationInputState>` to `AppState`.
5. Add `recompute_annotation_keys()` — computes `logicalEntityKey` for each row independently from `recompute_review_identities()`, not gated by `endpoint_supports_review_hash()`.
6. Call `recompute_annotation_keys()` from:
   - `configure_commit_navigation()` (initial load)
   - `apply_commit_snapshot()` (stepping)
   - `from_diff_result()` (construction)
7. Cancel active `annotation_input` in `apply_commit_snapshot()` with status message (§7.5).
8. Wire `apply_review_state()` to load annotations and annotation filter into AppState.
9. Wire `review_state_snapshot()` to include annotations and annotation filter in dirty snapshot.
10. Add `begin_annotation_input()`: resolve selected row's key, pre-fill existing text, enter input mode.
11. Add `confirm_annotation()`: validate non-whitespace, store annotation with timestamps and provenance hash, mark dirty, exit input mode. On replace: preserve `createdAt`, update `updatedAt` and `contentHashAtCreation`.
12. Add `cancel_annotation()`: discard input, exit input mode.
13. Add `delete_annotation()`: remove from HashMap if present, mark dirty, show status message.
14. Add `cycle_annotation_filter()`: cycle filter, mark dirty, realign selection.
15. Add accessors: `is_row_annotated()`, `row_annotation_text()`, `row_annotation_hash_matches()`.
16. Extend `visible_row_indices()` to apply annotation filter predicate alongside review filter.
17. Unit tests:
   - Add annotation via begin/confirm flow.
   - Replace existing annotation preserves `createdAt`.
   - Delete annotation removes from HashMap.
   - Blank/whitespace confirm treated as cancel.
   - `begin_annotation_input()` returns status message when key is None.
   - Annotation carry-across-step: annotations match new rows by `logicalEntityKey` after `apply_commit_snapshot()`.
   - Annotation key generation works when `endpoint_supports_review_hash()` returns false.
   - Entities with no hash material are annotatable (`contentHashAtCreation` is None).
   - `row_annotation_hash_matches()` returns true when hashes match, false when divergent, true when either hash is None.
   - Cancel-on-step: `apply_commit_snapshot()` clears active `annotation_input`.
   - Annotation filter composes with review filter in `visible_row_indices()`.
   - Filter cycle realigns selection when current row becomes hidden.
   - Fallback-key drift: duplicate same-name entities reordering across snapshots.

Acceptance:
1. Existing review-state and stepping behavior unchanged.
2. Annotation CRUD is deterministic in all modes.
3. `cargo test -p sem-cli` passes with all new AppState tests.

Gate:
- GO only if tests pass and no regressions.

### A3: Input Mode + Keybindings + Rendering
Deliverables:
1. Wire `a` keybinding in list, split, and detail mode key handlers to call `begin_annotation_input()`.
2. Wire `D` keybinding in list, split, and detail mode key handlers to call `delete_annotation()`.
3. Wire `A` keybinding in list, split, and detail mode key handlers to call `cycle_annotation_filter()`.
4. Add input mode key routing: when `annotation_input` is `Some`, route all key events to annotation input handler instead of mode-specific handlers.
5. Implement annotation input key handler:
   - Printable char: insert at cursor, enforce 256-char max.
   - Backspace: delete char before cursor.
   - Delete: delete char at cursor.
   - Left/Right: move cursor.
   - Home/End: cursor to start/end.
   - Enter: call `confirm_annotation()`.
   - Esc: call `cancel_annotation()`.
6. Suppress mouse events when `annotation_input` is active (same pattern as help overlay).
7. Render annotation input bar at the bottom of the screen (above footer) when `annotation_input` is active:
   - Prompt: `annotation: ` followed by editable text with visible cursor.
   - Distinct background color to indicate input mode.
8. Render `[*]` / `[~]` badge in list mode entity rows for annotated entities.
9. Render `[*]` / `[~]` badge in split mode sidebar for annotated entities.
10. Render annotation text block above diff content in detail mode and split preview:
    - Hash match or unavailable: `note: <text>` in accent color (yellow or cyan).
    - Hash divergent: `note (different version): <text>` in dimmer color.
11. Render annotation filter state in footer: `A: all|annotated|unannotated`.
12. Update help overlay text with `a`, `D`, `A` keybinding descriptions.
13. Unit/render tests:
    - Input mode key handling: insert, backspace, delete, cursor movement, home/end, enter, esc.
    - 256-char max enforcement.
    - Whitespace-only enter treated as cancel.
    - Mouse events suppressed during input mode.
    - Badge rendering: `[*]` for hash-match, `[~]` for hash-divergent, no badge for unannotated.
    - Annotation text block rendering in detail view.
    - Footer annotation filter cell renders correct token.

Acceptance:
1. Keybindings functional in all three modes (list, split, detail).
2. Input mode captures text and suppresses all other interactions.
3. Badges and annotation text display correctly with hash-divergence hints.
4. Annotation filter composes with review filter and footer renders both.
5. `cargo test -p sem-cli` passes with all new tests.

Gate:
- GO only with passing tests and manual verification of rendering.

### A4: Hardening + Docs
Deliverables:
1. Hardening tests:
   - Mixed persistence writes: annotation survival when only review state triggers a save.
   - Persistence round-trip with both review records, annotations, and all UI prefs.
   - Annotation filter preference restore on startup.
   - Cross-step annotation visibility: annotate at step N, step to M, verify annotation displays when entity is present and is absent when entity is not in diff.
   - Hash-divergence hint accuracy across steps: annotate at step N (hash X), step to M where entity has hash Y, verify `[~]` badge.
   - Input mode cancel-on-step with pending reload coordinator response.
2. README/docs updates:
   - Document `a`, `D`, `A` keybindings.
   - Document annotation persistence in `.sem/tui-review-state.json`.
   - Document annotation filter behavior.
   - Document provenance hash hint display.
3. Changelog milestone entry for annotation support.
4. Section 8 evidence closure for all phases.

Acceptance:
1. All hardening tests validate cross-step and cross-mode behavior.
2. User docs match runtime behavior.
3. No regressions in existing review-state or stepping tests.

Gate:
- GO only with complete evidence and all tests passing.

## 5. Verification Matrix
| Area | Verification | Command / Evidence |
|---|---|---|
| contract consistency | doc review | design/schema/plan cross-check |
| annotation persistence round-trip | unit tests | `cargo test -p sem-cli` annotation persistence tests |
| backward compat (no annotations field) | unit tests | load file without `annotations` key |
| duplicate key handling | unit tests | last-writer-wins on load |
| annotation CRUD | app tests | add/replace/delete/cancel state transitions |
| blank-submit-as-cancel | app tests | whitespace-only confirm discards |
| annotatability without hash material | app tests | entities with None content hash |
| annotation key endpoint independence | app tests | keys generated when `endpoint_supports_review_hash()` is false |
| annotation carry-across-step | app tests | `apply_commit_snapshot` preserves annotations |
| fallback-key drift | app tests | duplicate same-name entity reordering |
| cancel-on-step | app tests | `apply_commit_snapshot` clears active input |
| annotation filter cycle | app tests | 3-state cycle + dirty flag |
| filter composition | app tests | annotation filter + review filter compose in `visible_row_indices()` |
| filter-driven selection realignment | app tests | hidden row advances selection |
| input mode key handling | app/render tests | insert, backspace, delete, cursor, home/end, enter, esc |
| input mode 256-char max | app tests | char insertion rejected at limit |
| input mode mouse suppression | app tests | mouse events ignored during input |
| badge rendering (`[*]` / `[~]`) | render tests | hash match vs divergent assertions |
| annotation text block rendering | render tests | detail view annotation line |
| hash-divergence hint | render tests | "different version" suffix |
| footer annotation filter cell | render tests | `A: all\|annotated\|unannotated` |
| help overlay | render tests | `a`, `D`, `A` entries present |
| mixed persistence writes | unit tests | annotation survival across review-only saves |
| annotation filter preference restore | unit tests | `uiPrefs.annotationFilter` load/default |
| cross-step hash-divergence accuracy | integration tests | annotate step N, verify `[~]` at step M |
| stepping compatibility | integration tests | pairwise/cumulative annotation visibility |
| regression safety | full tests | `cargo test -p sem-cli && cargo test -p sem-core` |

## 6. Milestone Commit Gate
1. One milestone commit per phase.
2. Commit template:
   - `feat(sem): A0 annotation contracts lock`
   - `feat(sem): A1 annotation persistence foundation`
   - `feat(sem): A2 annotation AppState integration`
   - `feat(sem): A3 annotation input mode + rendering`
   - `feat(sem): A4 annotation hardening + docs`

## 7. Risks and Mitigations
1. Risk: fallback key instability causes annotation misbinding across steps.
   - Mitigation: accepted v1 limitation, documented in design §6.2.6 and §9.1. Parsers providing stable `entity_id` avoid this entirely.
2. Risk: annotation filter composes with review filter causing zero-visible-row confusion.
   - Mitigation: reuse existing no-match row + no-op controls from review filter implementation.
3. Risk: concurrent sessions overwrite annotations via last-writer-wins.
   - Mitigation: accepted v1 limitation, documented in design §9.2. Same behavior as review records.
4. Risk: input mode conflicts with async step responses.
   - Mitigation: explicit cancel-on-step behavior (design §7.5).
5. Risk: annotation input keybinding (`a`) collides with future feature bindings.
   - Mitigation: `a`/`D`/`A` verified free across all current mode handlers.
6. Risk: provenance hash hint shows false positives for formatting-only changes.
   - Mitigation: `contentHashAtCreation` uses content hash (not structural hash), so formatting changes are intentionally detected. This matches the review-state hash behavior. Structural hash comparison could be a future enhancement.
7. Risk: annotation display in detail/split preview consumes vertical space reducing diff visibility.
   - Mitigation: annotation is a single line; impact is minimal. No multi-line annotations in v1.

## 8. Operator Checklist and Evidence Log Schema

### 8.1 Checklist Per Phase
1. Validate prior phase GO.
2. Execute only current phase deliverables.
3. Run required verification commands.
4. Record Section 8 evidence before phase close.

### 8.2 Evidence Schema Template
```md
### Ax Evidence
- Completion date: YYYY-MM-DD
- Commit hash(es): <hashes>
- Acceptance evidence:
  - <command> => <summary>
  - manual: <validated behavior>
- Go/No-Go: GO | NO-GO
- Notes: <optional>
```

## 9. Deferred Items
1. Hunk-level or line-level annotations.
2. Annotation categories/tags (note, todo, question).
3. Multi-line annotation editing.
4. Cross-repo or cloud sync.
5. Automatic compaction or eviction.
6. Rich text or markdown rendering in annotation text.
7. Structural hash comparison for provenance hint (would suppress formatting-only divergence hints).
8. Robust multi-process merge conflict handling beyond last-writer-wins.

## 10. Execution Handoff Contract
0. Prerequisite:
   - `diff-tui-entity-review-state` implementation is landed.
   - `ReviewStateUiPrefs` supports view mode, diff view, and entity context mode persistence.
1. Required read order:
   1) `docs/implementation/diff-tui-entity-annotations/schema-proposal.md`
   2) `docs/implementation/diff-tui-entity-annotations/design.md`
   3) `docs/implementation/diff-tui-entity-annotations/phase-task-plan.md`
2. Start at `A0`.
3. Execute phases in strict order using this plan as source of truth for scope, gates, and Section 8 evidence updates.
