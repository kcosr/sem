# Schema Proposal: Diff TUI Full-Entity Toggle

## Status
Locked

## 1. Goal
Lock an internal runtime contract for entity-context mode toggling and mode-specific render snapshot semantics in Diff TUI.

## 2. Example Payloads

### 2.1 Runtime Toggle Action (internal)
```json
{
  "action": "toggleEntityContext",
  "currentMode": "hunk",
  "nextMode": "entity",
  "reset": {
    "detailHunkIndex": 0,
    "detailScroll": 0
  }
}
```

### 2.2 Render Snapshot (internal)
```json
{
  "entityContextMode": "entity",
  "view": "unified",
  "anchors": [12, 30],
  "anchorSource": "changedRegions",
  "lineCount": 96,
  "placeholder": false
}
```

### 2.3 Footer Cell Model (internal)
```json
{
  "precedingCells": ["m: cumulative", "r: unreviewed"],
  "key": "e",
  "value": "entity",
  "rendered": "e: entity"
}
```
`precedingCells` is illustrative context only (it is not part of the locked footer cell schema for this topic).

## 3. JSON Schema Skeleton

### 3.1 Context Mode Token
```json
{
  "$id": "sem.tui.entity-context-mode.v1",
  "type": "string",
  "enum": ["hunk", "entity"]
}
```

### 3.2 Render Snapshot Shape
```json
{
  "$id": "sem.tui.entity-context-render-snapshot.v1",
  "type": "object",
  "required": ["entityContextMode", "view", "anchors", "anchorSource", "lineCount", "placeholder"],
  "properties": {
    "entityContextMode": { "type": "string", "enum": ["hunk", "entity"] },
    "view": { "type": "string", "enum": ["unified", "sideBySide"] },
    "anchors": {
      "type": "array",
      "items": { "type": "integer", "minimum": 0 }
    },
    "anchorSource": { "type": "string", "enum": ["groupedHunks", "changedRegions"] },
    "lineCount": { "type": "integer", "minimum": 0 },
    "placeholder": { "type": "boolean" }
  },
  "additionalProperties": false
}
```

### 3.3 Footer Cell Shape
```json
{
  "$id": "sem.tui.footer.entity-context-cell.v1",
  "type": "object",
  "required": ["key", "value", "rendered"],
  "properties": {
    "key": { "type": "string", "const": "e" },
    "value": { "type": "string", "enum": ["hunk", "entity"] },
    "rendered": { "type": "string", "pattern": "^e: (hunk|entity)$" }
  },
  "additionalProperties": false
}
```

## 4. Endpoint / Contract Lock
1. Valid runtime entity context tokens are exactly `hunk` and `entity`.
2. Toggle always flips token value and never yields a third state.
3. Toggle in detail mode always resets:
   - `detailHunkIndex` to `0`
   - `detailScroll` to `0`
4. `hunk` mode anchor source is `groupedHunks`.
5. `entity` mode anchor source is `changedRegions` from full entity render stream.
6. `changedRegions` means contiguous non-equal diff-op runs; each anchor is the first rendered row index of that run in the active view output vector.
7. Anchor coordinate space is always 0-based rendered row index:
   - `unified`: index into `unified_lines`
   - `sideBySide`: index into `side_by_side_lines` shared row stream
8. Footer cell render format is exactly lowercase `e: <token>`.
9. Startup default is `hunk`.
10. Footer cell ordering must follow shared contract `m`, `r`, `e`; this topic owns only `e`.

## 5. Deterministic Reject / Status Lock
1. Unknown mode token must be rejected in internal constructors/tests; runtime falls back to `hunk` only via guarded initialization, not silent parse from user input.
2. Empty anchor list is valid and treated as boundary no-op for `n/p`.
3. Missing content yields placeholder snapshot with `placeholder=true` and preserves mode token.
4. View/mode anchor selection must be deterministic for all combinations of:
   - `hunk` or `entity`
   - `unified` or `sideBySide`
5. `e` toggle is valid during placeholder/loading states; mode flip remains deterministic and status-slot loading semantics remain unchanged.

## 6. Notes
1. This schema is internal to Rust TUI runtime and not an external CLI JSON output contract.
2. Existing `--format json` output remains unchanged.
3. `rendered` in footer cell is a documentation convenience and may be derived from `key` + `value`.
4. File-aggregate and persisted mode preference remain deferred.
5. Footer layout semantics align with `docs/implementation/diff-tui-footer-cell-layout/`.
