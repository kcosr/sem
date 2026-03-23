# Design: Diff TUI Entity Annotations

## Status
Draft

## 1. Purpose
Add per-entity annotation support to the TUI, allowing users to attach free-text notes to semantic entities during review. Annotations persist across sessions, carry across commit stepping, and bind to entity identity rather than content snapshots.

## 2. Problem Statement
The TUI supports marking entities as reviewed, but there is no way to attach qualitative notes during review. Reviewers need to capture observations like "needs error handling", "ask Alice about this", or "revisit after auth migration" directly alongside the entity they're examining. Without annotations, this context lives outside the tool (scratch files, chat messages, memory) and is disconnected from the entity it describes.

## 3. Goals
1. Support per-entity text annotations via keyboard-driven input in list, split, and detail modes.
2. Persist annotations to local repo metadata alongside existing review state.
3. Carry annotations across commit stepping by matching on `logicalEntityKey` only (not content hash).
4. Display annotation presence in list/split views and full annotation text in detail view.
5. Support annotation deletion.

## 4. Non-Goals
1. No hunk-level or line-level annotations in v1.
2. No annotation categories/tags (note, todo, question) in v1 — free text only.
3. No multi-line annotation editing in v1 — single-line input only.
4. No cross-repo or cloud sync.
5. No automatic compaction or eviction — annotations are user-authored content, not derived state.
6. No rich text, markdown rendering, or syntax highlighting in annotation text.

## 5. Current Baseline
1. TUI supports entity navigation across list, split, and detail modes.
2. Unified stepping model provides `pairwise`/`cumulative` modes with comparator endpoint semantics.
3. Review state persists to `.sem/tui-review-state.json` with debounced atomic writes.
4. `ReviewIdentity` uses composite key: `logicalEntityKey` + `targetContentHash`.
5. `logicalEntityKey` provides stable entity identity via `entityId::<entity_id>` (preferred) or `fallback::<path>::<type>::<name>::<ordinal>` grammar.
6. `build_logical_entity_key()` and `build_target_content_hash()` in `review_state.rs` are the shared identity-building functions.
7. Per-row identity is recomputed on each snapshot application via `recompute_review_identities()`.
8. `ReviewStateUiPrefs` now persists view mode, diff view, and entity context mode preferences.

## 6. Key Decisions

### 6.1 Annotation Identity
1. Annotations bind to `logicalEntityKey` only. The content hash is **not** part of the annotation lookup key.
2. The `logicalEntityKey` grammar is the same as review state (§5.5). This provides stable identity across commit stepping as long as the entity's ID or name+path+type remain consistent.
3. When the user annotates an entity, the current `targetContentHash` is stored as provenance metadata (`contentHashAtCreation`) but is not used for matching.
4. An entity can have at most one annotation in v1. Adding an annotation to an already-annotated entity replaces the existing annotation. This avoids the need for annotation list management UI.

### 6.2 Matching Semantics Across Stepping
1. When a commit step replaces `self.rows`, annotation matching is recomputed.
2. Matching is entity-key-only: if the entity appears in the new diff and its `logicalEntityKey` exists in the annotation store, the annotation is displayed.
3. If an annotated entity does not appear in the current diff (it wasn't changed in this commit pair), the annotation is retained in storage but not visible. It is not lost.
4. Annotations created in pairwise mode at step N are visible at step M if the same entity appears in the diff at step M.
5. Annotations created in cumulative mode behave identically — the key is the entity, not the stepping context.
6. **Fallback key stability caveat**: the fallback key grammar (`fallback::<path>::<type>::<name>::<ordinal>`) uses ordinals recomputed per snapshot from row emission order. If duplicate same-name entities reorder, appear, or disappear between steps, a fallback-keyed annotation can silently bind to the wrong entity. This is an accepted v1 limitation. Mitigation: annotations on fallback-keyed entities should be treated as best-effort. Parsers that provide stable `entity_id` values avoid this entirely. A future improvement could warn when a fallback-keyed annotation matches a row whose content hash diverges significantly from `contentHashAtCreation`.

### 6.3 Keybindings
1. `a` — add/replace annotation on selected entity. Enters inline text input mode.
2. `D` (shift+d) — delete annotation on selected entity. First press arms deletion for the selected entity; pressing `D` again confirms. `Esc` cancels a pending delete.
3. `A` (shift+a) — cycle annotation filter: `all → annotated → unannotated → all`.
4. These keybindings are available in list, split, and detail modes.

### 6.11 Annotation Filter
1. Three states: `All`, `Annotated`, `Unannotated`. Default is `All`.
2. `A` (shift+a) cycles through the states.
3. The annotation filter composes with the review filter (`r`). A row is visible only if it passes both filters. `visible_row_indices()` applies both predicates.
4. When the annotation filter hides the currently selected row, selection realigns to the next visible row (same behavior as review filter).
5. The annotation filter state is persisted in `uiPrefs` alongside `reviewFilter`.
6. Footer shows the current annotation filter state, e.g. `A: annotated`.

### 6.4 Text Input Mode
1. Pressing `a` transitions the TUI into an inline text input mode.
2. A single-line text input bar appears at the bottom of the screen (above the footer), pre-filled with any existing annotation text for edit.
3. `Enter` confirms the annotation. `Esc` cancels without saving.
4. Standard text editing: left/right cursor, backspace, delete, home/end.
5. The input bar shows a prompt like `annotation: ` followed by the editable text.
6. While in input mode, all other keybindings are suppressed.
7. Maximum annotation length: 256 characters (single line, terminal-width-practical).
8. Submitting empty or whitespace-only text is treated as a cancel (no annotation stored, no deletion of existing annotation).

### 6.5 Display
1. **List mode**: annotated entities show a `[*]` badge after the entity name column.
2. **Split mode sidebar**: same `[*]` badge as list mode.
3. **Split mode preview / Detail mode**: annotation text is rendered in a dedicated annotation panel above the diff content. The panel title is `Annotation` and the body contains the raw annotation text.
4. If the annotation's `contentHashAtCreation` is present and differs from the current entity content hash, a visual hint is shown to indicate the annotation was written against a different version of the code:
   - **List/Split sidebar**: badge changes from `[*]` to `[~]`
   - **Detail/Split preview**: annotation panel title changes to `Annotation (Different Version)` and uses dimmer styling
   - If `contentHashAtCreation` is absent (hash material was unavailable at creation) or the current content hash cannot be computed, no hint is shown — the annotation displays normally.
   - The annotation is always shown regardless of hash match. The hint is informational only.

### 6.6 Persistence
1. Annotations are stored in the same `.sem/tui-review-state.json` file, in a new `annotations` array alongside `reviewRecords`.
2. Schema version remains `1` — the `annotations` field is optional and defaults to empty on load. Existing files without `annotations` load normally.
3. Annotations use the same debounced atomic write path as review records.
4. Annotation dirty flag is unified with `review_state_dirty` — any mutation (add, replace, delete) marks the state dirty.
5. No compaction. Annotations are user-authored content and are never automatically evicted.

### 6.7 Persistence Record Format
Each annotation record contains:
- `logicalEntityKey`: string — the entity identity key
- `text`: string — the annotation content
- `contentHashAtCreation`: string (optional) — `sha256:<hex>` hash of the entity content when the annotation was created (provenance, not used for matching). May be absent if hash material was unavailable at creation time.
- `createdAt`: string — ISO 8601 UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`)
- `updatedAt`: string — ISO 8601 UTC timestamp, updated on replace

### 6.10 Annotatability
An entity is annotatable if its `logicalEntityKey` can be resolved (i.e., `row_annotation_keys[i]` is `Some`). The `contentHashAtCreation` field is best-effort: if `build_target_content_hash()` returns `None` for the entity (no hash material), the annotation is still created with `contentHashAtCreation` omitted. This ensures entities without content (e.g., pure-metadata changes) are still annotatable.

### 6.8 Runtime State in AppState
1. `annotations: HashMap<String, Annotation>` — keyed by `logicalEntityKey`, stores current annotation data.
2. `row_annotation_keys: Vec<Option<String>>` — per-row, the `logicalEntityKey` for annotation lookup. This is computed independently from `row_review_identities` — annotation key generation must NOT be gated by `endpoint_supports_review_hash()`, since annotations should work for all source modes including STDIN and unsupported endpoints.
3. `annotation_input: Option<AnnotationInputState>` — present when inline text input is active.
4. `annotation_filter: AnnotationFilter` — current filter state, persisted in `uiPrefs`.
5. `annotation_dirty: bool` — unified with or parallel to `review_state_dirty`.

### 6.9 AnnotationInputState
```
struct AnnotationInputState {
    text: String,
    cursor_position: usize,
    target_logical_entity_key: String,
    target_content_hash: Option<String>,
}
```
Captures the input buffer, cursor position, and the identity of the entity being annotated (resolved at the moment `a` is pressed, not at confirmation time).

## 7. Data Flow

### 7.1 Adding an Annotation
1. User presses `a` on a selected entity.
2. `AppState` resolves the selected row's `logicalEntityKey` via `row_annotation_keys[selected_row_index]`.
3. If the key is `None` (entity identity unavailable), show a status message and abort.
4. `AppState` enters input mode: `annotation_input = Some(AnnotationInputState { ... })`.
5. If an existing annotation exists for this key, pre-fill `text` with it.
6. Rendering shows the input bar. All keys route to input handling.
7. On `Enter`: store the annotation in `self.annotations`, mark dirty, exit input mode.
8. On `Esc`: discard, exit input mode.

### 7.2 Deleting an Annotation
1. User presses `D` (shift+d) on an annotated entity.
2. If no annotation exists for the selected entity, no-op.
3. On first press, the app arms delete confirmation for the selected entity and shows a transient status message: `Press D again to delete annotation; Esc cancels`.
4. On second `D` press for the same selected entity, remove the annotation from `self.annotations`, mark dirty, and show `annotation removed`.
5. If the user presses `Esc` before confirming, the pending delete is cancelled.

### 7.3 Stepping / Snapshot Application
1. `apply_commit_snapshot()` replaces `self.rows`.
2. Existing call to `recompute_review_identities()` is extended (or a parallel `recompute_annotation_keys()` is added).
3. For each new row, `logicalEntityKey` is computed and stored in `row_annotation_keys`.
4. `self.annotations` HashMap is unchanged — it persists across steps.
5. Rendering checks `row_annotation_keys[i]` against `self.annotations` to determine badge/display.

### 7.5 Stepping While Annotation Input Is Active
Step/refresh responses arrive asynchronously via `ReloadCoordinator` (mod.rs:219) and `apply_commit_snapshot()` resets rows and selection immediately. If annotation input is active when a snapshot arrives:
1. The in-progress annotation input is **cancelled automatically** — `annotation_input` is set to `None`.
2. A transient status message is shown: "annotation input cancelled: commit step applied".
3. The `target_logical_entity_key` captured at `a`-press time may no longer correspond to any row. Discarding is safer than committing to a potentially mismatched entity.
4. The user can re-press `a` after the step completes to start a new annotation on the now-current selection.

### 7.4 Persistence Round-Trip
1. On save: serialize `self.annotations` into the `annotations` array in the JSON file.
2. On load: deserialize the `annotations` array into `self.annotations` HashMap keyed by `logicalEntityKey`.
3. Duplicate keys on load: last-writer-wins (same as review records).

## 8. Resolved Questions

### 8.1 Delete Confirmation — RESOLVED
Delete uses `D` (shift+d) with explicit two-step confirmation. First press arms deletion and shows `Press D again to delete annotation; Esc cancels`; second press confirms deletion. This preserves keyboard-only flow without a modal dialog.

### 8.2 Keybinding Conflicts — RESOLVED
`a` and `D` have no conflicts with existing bindings (`v`, `s`, `n`, `p`, `e`, `r`, `m`, `[`, `]`, `g`, `G`, `j`, `k`, `q`, `?`, Space, Enter, Esc, Tab, arrows, PageUp/PageDown). Lowercase `d` remains available for future use.

### 8.3 Annotation Visibility Across Diff Sources — RESOLVED
Annotations work uniformly across all source modes. Annotation key generation is decoupled from `endpoint_supports_review_hash()` (unlike review identities), so annotations are available for STDIN, unsupported endpoints, and all stepping modes.

### 8.4 Input Mode and Mouse Events — RESOLVED
While in annotation input mode, mouse events are ignored (same pattern as the help overlay).

### 8.5 Blank/Whitespace Submit — RESOLVED
Submitting empty or whitespace-only text is treated as cancel. No annotation is stored or deleted.

### 8.6 Stepping During Input — RESOLVED
If a commit step response arrives while annotation input is active, the input is cancelled automatically with a status message. See §7.5.

## 9. Accepted Limitations

### 9.1 Fallback Key Instability
Annotations on fallback-keyed entities (those without stable `entity_id`) may silently bind to the wrong entity if duplicate same-name entities reorder across steps. See §6.2.6.

### 9.2 Co-located Persistence Write Risk
The entire persistence file is rewritten on any dirty flag change (review toggle, UI pref change, or annotation mutation). In concurrent sessions, a review-only toggle in one session can overwrite annotations saved by another. This is the same last-writer-wins behavior as review records and is accepted for v1.

## 9. Persistence Schema Extension

### 9.1 Extended File Example
```json
{
  "version": 1,
  "repoId": "sha256:8f7d...",
  "uiPrefs": {
    "reviewFilter": "all",
    "viewMode": "split",
    "diffView": "side-by-side",
    "entityContextMode": "hunk",
    "annotationFilter": "all"
  },
  "reviewRecords": [
    {
      "logicalEntityKey": "entityId::src/auth.ts::function::validateToken",
      "targetContentHash": "sha256:37f8...",
      "updatedAt": "2026-03-08T18:20:00Z"
    }
  ],
  "annotations": [
    {
      "logicalEntityKey": "entityId::src/auth.ts::function::validateToken",
      "text": "needs error handling for expired tokens",
      "contentHashAtCreation": "sha256:37f8...",
      "createdAt": "2026-03-08T18:21:00Z",
      "updatedAt": "2026-03-08T18:21:00Z"
    }
  ]
}
```

### 9.2 JSON Schema for Annotation Record
```json
{
  "$id": "sem.tui.annotation-record.v1",
  "type": "object",
  "required": ["logicalEntityKey", "text", "createdAt", "updatedAt"],
  "properties": {
    "logicalEntityKey": { "type": "string", "minLength": 1 },
    "text": { "type": "string", "minLength": 1, "maxLength": 256 },
    "contentHashAtCreation": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" },
    "createdAt": { "type": "string", "pattern": "^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}Z$" },
    "updatedAt": { "type": "string", "pattern": "^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}Z$" }
  },
  "additionalProperties": false
}
```

## 10. Implementation Outline

### Phase 1: Data Model and Persistence
- Add `Annotation` struct and `PersistedAnnotationRecord` to `review_state.rs`.
- Extend `ReviewStateData` with `annotations: HashMap<String, Annotation>`.
- Extend `PersistedReviewState` with optional `annotations` array.
- Update `load()` and `save()` to round-trip annotations.
- Add tests for annotation persistence round-trip, missing field defaults, and duplicate key handling.

### Phase 2: AppState Integration
- Add `annotations`, `row_annotation_keys`, `annotation_input`, and dirty tracking to `AppState`.
- Add `recompute_annotation_keys()` called alongside `recompute_review_identities()`.
- Wire `apply_review_state()` to load annotations into AppState.
- Wire `review_state_snapshot()` to include annotations in the dirty snapshot.
- Add `begin_annotation_input()`, `confirm_annotation()`, `cancel_annotation()`, `delete_annotation()` methods.
- Add `is_row_annotated()`, `row_annotation_text()`, and `row_annotation_hash_matches()` accessors. The hash comparison checks the row's current `targetContentHash` (from `build_target_content_hash`) against the annotation's `contentHashAtCreation`.

### Phase 3: Input Mode
- Add `Mode::AnnotationInput` or a separate `annotation_input: Option<AnnotationInputState>` flag.
- Route key events to input handler when input mode is active.
- Implement basic text editing (insert char, backspace, delete, left, right, home, end).
- Suppress all other keybindings and mouse events during input.

### Phase 4: Rendering
- Add `[*]` / `[~]` badge to list and split sidebar rendering for annotated entities (`[*]` when hash matches or is unavailable, `[~]` when hash differs).
- Add annotation text block in detail and split preview rendering, with "different version" suffix when provenance hash diverges.
- Add input bar rendering at the bottom of the screen during input mode.
- Style annotation display (color, prefix, dimmed variant for hash-divergent annotations).

### Phase 5: Testing
- Unit tests for annotation CRUD in AppState (add, replace, delete, blank-submit-as-cancel).
- Unit tests for annotation carry-across-step (simulate `apply_commit_snapshot` and verify annotations match new rows by `logicalEntityKey`).
- Unit tests for fallback-key drift: duplicate same-name entities reordering across snapshots, verifying annotation binding behavior.
- Unit tests for in-flight step response cancelling active annotation input (§7.5).
- Unit tests for annotation key generation without endpoint support (STDIN/unsupported sources — verify keys are still computed when `endpoint_supports_review_hash()` returns false).
- Unit tests for entities with no hash material (verify annotatability with `contentHashAtCreation` as `None`).
- Unit tests for persistence round-trip with both review records and annotations, including files with annotations but no review records and vice versa.
- Unit tests for input mode key handling (insert, backspace, delete, cursor movement, home/end, enter, esc).
- Unit tests for mixed persistence writes: verify annotation survival when only review state triggers a save.

## 11. Review Log

### 2026-03-23: Gemini + Codex Review (Draft → Revised Draft)
**Reviewers**: generic-gemini, generic-codex via agent-runner

**Incorporated findings:**
1. (Codex, High) Fallback key instability across stepping — added §6.2.6 documenting the risk and accepted limitation.
2. (Codex, Medium) In-flight step response during annotation input — added §7.5 with explicit cancel-on-step behavior.
3. (Gemini, Medium) Endpoint support decoupling — updated §6.8 to specify annotation key generation is independent of `endpoint_supports_review_hash()`.
4. (Codex, Medium) Hash material absence — added §6.10 defining annotatability and made `contentHashAtCreation` optional in schema.
5. (Gemini+Codex, Medium) Delete safety — changed from `d` to `D` (shift+d) in §6.3.
6. (Codex, Low) Co-located write risk — added §9.2 documenting accepted limitation.
7. (Codex) Blank/whitespace submit — added §6.4.8 and §8.5.
8. (Codex) Test coverage gaps — expanded Phase 5 with fallback drift, input cancellation, endpoint-ungated keys, no-hash-material, and mixed persistence tests.
