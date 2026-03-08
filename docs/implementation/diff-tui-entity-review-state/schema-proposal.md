# Schema Proposal: Diff TUI Entity Review State

## Status
Locked

## 1. Goal
Define an internal persistence and runtime contract for entity-level reviewed state and visibility filtering in TUI.

## 2. Example Payloads

### 2.1 Runtime Toggle Action (internal)
```json
{
  "action": "toggleReviewed",
  "entity": {
    "logicalEntityKey": "entityId::f::src/auth.ts::validateToken",
    "targetContentHash": "sha256:37f8..."
  },
  "source": {
    "mode": "cumulative",
    "fromEndpointId": "commit:1111111...",
    "toEndpointId": "working"
  }
}
```

### 2.2 Runtime Filter State (internal)
```json
{
  "filter": "unreviewed"
}
```

### 2.3 Footer Cells (internal)
```json
{
  "modeCell": "m: cumulative",
  "reviewCell": "r: unreviewed"
}
```

### 2.4 Persistence File Example
```json
{
  "version": 1,
  "repoId": "sha256:8f7d...",
  "uiPrefs": {
    "reviewFilter": "all"
  },
  "reviewRecords": [
    {
      "logicalEntityKey": "entityId::f::src/auth.ts::validateToken",
      "targetContentHash": "sha256:37f8...",
      "updatedAt": "2026-03-08T18:20:00Z"
    }
  ]
}
```

## 3. JSON Schema Skeleton

### 3.1 Runtime Review Identity
```json
{
  "$id": "sem.tui.review-identity.v1",
  "type": "object",
  "required": ["logicalEntityKey", "targetContentHash"],
  "properties": {
    "logicalEntityKey": { "type": "string", "minLength": 1 },
    "targetContentHash": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" }
  },
  "additionalProperties": false
}
```

### 3.2 Persistence File
```json
{
  "$id": "sem.tui.review-state.file.v1",
  "type": "object",
  "required": ["version", "repoId", "reviewRecords"],
  "properties": {
    "version": { "type": "integer", "enum": [1] },
    "repoId": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" },
    "uiPrefs": {
      "type": "object",
      "required": ["reviewFilter"],
      "properties": {
        "reviewFilter": { "type": "string", "enum": ["all", "unreviewed", "reviewed"] }
      },
      "additionalProperties": false
    },
    "reviewRecords": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["logicalEntityKey", "targetContentHash", "updatedAt"],
        "properties": {
          "logicalEntityKey": { "type": "string", "minLength": 1 },
          "targetContentHash": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" },
          "updatedAt": { "type": "string", "format": "date-time" }
        },
        "additionalProperties": false
      }
    }
  },
  "additionalProperties": false
}
```

## 4. Endpoint / Contract Lock
1. Reviewed carryover requires exact match on both `logicalEntityKey` and `targetContentHash`.
2. Filter states are exactly: `all`, `unreviewed`, `reviewed`.
3. Persistence path is `.sem/tui-review-state.json`.
4. Persistence scope is local repo only.
5. Record presence means reviewed; unreview removes record.
6. `targetContentHash` is derived from active comparator target endpoint content (`toEndpointId`) in current step snapshot.
7. Valid comparator endpoint ID kinds for review hashing are:
   - `commit:<sha>`
   - `index`
   - `working`
8. Review toggle/filter actions never mutate step cursor, step mode, or comparator endpoint IDs.
9. Review filter footer cell format is exactly `r: <all|unreviewed|reviewed>`.

## 5. Deterministic Reject / Status Lock
1. Corrupt persistence file => ignore file, keep session usable, show non-fatal status.
2. Unsupported schema version => ignore file with non-fatal status.
3. Repo ID mismatch => ignore file with non-fatal status.
4. Missing hash material => entity cannot be matched to stored reviewed state for that render cycle.
5. Unknown comparator endpoint kind for hash source => treat as missing hash material (non-fatal).

## 6. Notes
1. `logicalEntityKey` grammar (from design lock):
   - `entityId::<entity_id>` preferred
   - fallback `fallback::<canonicalPath>::<entityType>::<entityName>::<occurrenceOrdinal>`
2. `targetContentHash` is normalized target-side content hash; delete-path uses pre-state entity content as hash material.
3. `repoId` is hash of canonical repo root path.
4. This schema is internal; no external JSON API contract changes.
5. Hunk-level review state and annotations are out of scope for this topic.
6. Cursor/range resume restoration is deferred.
7. Footer rendering with review filter indicator must preserve `m: <mode>` step-mode cell from unified stepping.
8. Footer cell ordering follows `docs/implementation/diff-tui-footer-cell-layout/` (`m`, `r`, `e`).
