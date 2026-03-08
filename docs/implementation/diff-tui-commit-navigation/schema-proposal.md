# Schema Proposal: Diff TUI Commit Navigation

## Status
Locked

## 1. Goal
Lock the internal CLI/TUI contract for commit-stepping reloads so keyboard navigation, commit metadata, status/error handling, and stale-result behavior remain deterministic.

## 2. Example Request/Response Payloads

### 2.1 Logical Reload Request (internal)
```json
{
  "requestId": 42,
  "action": "stepOlder",
  "current": {
    "sha": "f1a2b3c4d5...",
    "revLabel": "HEAD~2"
  },
  "sourceMode": "commit"
}
```

### 2.2 Logical Reload Success Response (internal)
```json
{
  "ok": true,
  "status": "loaded",
  "appliedRequestId": 42,
  "snapshot": {
    "cursor": {
      "revLabel": "HEAD~3",
      "sha": "0ab12cd",
      "subject": "refactor: split diff loader",
      "hasOlder": true,
      "hasNewer": true
    },
    "summary": {
      "fileCount": 4,
      "added": 2,
      "modified": 8,
      "deleted": 1,
      "moved": 0,
      "renamed": 0,
      "total": 11
    },
    "changes": []
  }
}
```

### 2.3 Logical Reload Failure Response (internal)
```json
{
  "ok": false,
  "status": "loadFailed",
  "appliedRequestId": 42,
  "error": "unable to resolve commit HEAD~999",
  "retainPreviousSnapshot": true
}
```

## 3. JSON Schema Skeleton

```json
{
  "$id": "sem.tui.commit-navigation.v1",
  "type": "object",
  "required": ["ok", "status", "appliedRequestId"],
  "properties": {
    "ok": { "type": "boolean" },
    "status": {
      "type": "string",
      "enum": ["loading", "loaded", "loadFailed", "unsupportedMode", "boundaryNoop", "ignoredStaleResult"]
    },
    "appliedRequestId": { "type": "integer", "minimum": 0 },
    "snapshot": {
      "type": ["object", "null"],
      "required": ["cursor", "summary", "changes"],
      "properties": {
        "cursor": {
          "type": "object",
          "required": ["sha", "subject", "hasOlder", "hasNewer"],
          "properties": {
            "revLabel": { "type": ["string", "null"] },
            "sha": { "type": "string", "minLength": 7 },
            "subject": { "type": "string" },
            "hasOlder": { "type": "boolean" },
            "hasNewer": { "type": "boolean" }
          },
          "additionalProperties": false
        },
        "summary": {
          "type": "object",
          "required": ["fileCount", "added", "modified", "deleted", "moved", "renamed", "total"],
          "properties": {
            "fileCount": { "type": "integer", "minimum": 0 },
            "added": { "type": "integer", "minimum": 0 },
            "modified": { "type": "integer", "minimum": 0 },
            "deleted": { "type": "integer", "minimum": 0 },
            "moved": { "type": "integer", "minimum": 0 },
            "renamed": { "type": "integer", "minimum": 0 },
            "total": { "type": "integer", "minimum": 0 }
          },
          "additionalProperties": false
        },
        "changes": {
          "type": "array",
          "description": "SemanticChange[] payload equivalent to in-memory DiffResult.changes",
          "items": { "type": "object" }
        }
      },
      "additionalProperties": false
    },
    "error": { "type": ["string", "null"] },
    "retainPreviousSnapshot": { "type": ["boolean", "null"] }
  },
  "additionalProperties": false
}
```

## 4. Endpoint / Contract Lock
This is a local CLI/TUI internal contract (not HTTP).

Locked points:
1. Commit stepping actions are `stepOlder` and `stepNewer`.
2. Supported stepping source mode in v1 is commit-backed TUI sessions.
3. On reload success, cursor metadata (`sha`, `subject`, boundary booleans) is mandatory.
4. On reload failure, previous snapshot is retained.
5. Unsupported modes and boundary no-op are non-fatal statuses.
6. `requestId`/`appliedRequestId` determine stale-result rejection deterministically.

## 5. Deterministic Reject / Status Lock
1. Unsupported source mode returns `unsupportedMode` status (no crash).
2. Boundary step with no older/newer target returns `boundaryNoop` status.
3. Commit resolution/load errors return `loadFailed` with message and `retainPreviousSnapshot=true`.
4. In-progress reload sets `loading` status.
5. Completion for stale request sets `ignoredStaleResult` and does not mutate visible snapshot.

## 6. Notes
1. This schema is documentary for design/test determinism; it is not a new public JSON output API.
2. Existing `sem diff --format json` contract remains unchanged.
3. `revLabel` uses frozen session-head first-parent lineage: `HEAD~N` when derivable; else null.
4. `summary.fileCount` means files touched; `summary.total` means total semantic change records.
5. Merge commits use first-parent traversal for older/newer determinism.
