# Schema Proposal: Diff TUI Unified Stepping

## Status
Locked

## 1. Goal
Lock an internal request/response contract for unified endpoint stepping with mode semantics (`pairwise`, `cumulative`) across commits, `INDEX`, and `WORKING`.

## 2. Example Payloads

### 2.1 Step Request (internal)
```json
{
  "requestId": 108,
  "action": "stepOlder",
  "cursor": {
    "endpointId": "commit:89ab012",
    "index": 7,
    "displayRef": "HEAD~2"
  },
  "mode": "pairwise",
  "baseEndpointId": "commit:fedcba9"
}
```

### 2.2 Step Response (loaded)
```json
{
  "appliedRequestId": 108,
  "status": "loaded",
  "snapshot": {
    "cursor": {
      "endpointId": "commit:abc1234",
      "index": 6,
      "hasOlder": true,
      "hasNewer": true
    },
    "mode": "pairwise",
    "baseEndpointId": "commit:fedcba9",
    "comparison": {
      "fromEndpointId": "commit:fedcba9",
      "toEndpointId": "commit:abc1234"
    },
    "summary": {
      "fileCount": 3,
      "added": 1,
      "modified": 4,
      "deleted": 1,
      "moved": 0,
      "renamed": 0,
      "total": 6
    }
  },
  "retainPreviousSnapshot": false
}
```

### 2.3 Step Response (boundary)
```json
{
  "appliedRequestId": 109,
  "status": "boundaryNoop",
  "snapshot": null,
  "retainPreviousSnapshot": true
}
```

## 3. JSON Schema Skeleton

### 3.1 Endpoint ID
`endpointId` encoding:
1. `commit:<40-hex-sha>`
2. `index`
3. `working`

### 3.2 Request Skeleton
```json
{
  "$id": "sem.tui.unified-step.request.v1",
  "type": "object",
  "required": ["requestId", "action", "cursor", "mode"],
  "properties": {
    "requestId": { "type": "integer", "minimum": 0 },
    "action": { "type": "string", "enum": ["stepOlder", "stepNewer"] },
    "cursor": {
      "type": "object",
      "required": ["endpointId", "index"],
      "properties": {
        "endpointId": { "type": "string", "minLength": 1 },
        "index": { "type": "integer", "minimum": 0 },
        "displayRef": { "type": ["string", "null"] }
      },
      "additionalProperties": false
    },
    "mode": { "type": "string", "enum": ["pairwise", "cumulative"] },
    "baseEndpointId": { "type": ["string", "null"] }
  },
  "additionalProperties": false
}
```

### 3.3 Response Skeleton
```json
{
  "$id": "sem.tui.unified-step.response.v1",
  "type": "object",
  "required": ["appliedRequestId", "status", "retainPreviousSnapshot"],
  "properties": {
    "appliedRequestId": { "type": "integer", "minimum": 0 },
    "status": {
      "type": "string",
      "enum": ["loaded", "loadFailed", "boundaryNoop", "unsupportedMode", "ignoredStaleResult"]
    },
    "snapshot": {
      "oneOf": [
        { "type": "null" },
        {
          "type": "object",
          "required": ["cursor", "mode", "comparison", "summary"],
          "properties": {
            "cursor": {
              "type": "object",
              "required": ["endpointId", "index", "hasOlder", "hasNewer"],
              "properties": {
                "endpointId": { "type": "string" },
                "index": { "type": "integer", "minimum": 0 },
                "hasOlder": { "type": "boolean" },
                "hasNewer": { "type": "boolean" }
              },
              "additionalProperties": false
            },
            "mode": { "type": "string", "enum": ["pairwise", "cumulative"] },
            "baseEndpointId": { "type": ["string", "null"] },
            "comparison": {
              "type": "object",
              "required": ["fromEndpointId", "toEndpointId"],
              "properties": {
                "fromEndpointId": { "type": "string" },
                "toEndpointId": { "type": "string" }
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
            }
          },
          "additionalProperties": false
        }
      ]
    },
    "error": { "type": ["string", "null"] },
    "retainPreviousSnapshot": { "type": "boolean" }
  },
  "additionalProperties": false
}
```

## 4. Endpoint / Contract Lock
1. Unified stepping always operates over ordered endpoint paths.
2. Mode is explicit per snapshot (`pairwise` or `cumulative`).
3. Response always states effective comparator endpoints (`comparison.fromEndpointId`, `comparison.toEndpointId`).
4. Boundary and failure are non-fatal and preserve prior view.
5. `displayRef` is presentation-only; endpoint identity is `endpointId`.

## 5. Deterministic Reject / Status Lock
1. Invalid endpoint => `loadFailed` with `retainPreviousSnapshot=true`.
2. Out-of-range step => `boundaryNoop`.
3. Unsupported source mode => `unsupportedMode`.
4. Stale response => `ignoredStaleResult`.
5. `baseEndpointId = null` in cumulative mode means "re-anchor to current cursor on toggle-on" per design lock.

## 6. Notes
1. Symbolic refs are resolved to SHA endpoint IDs before cursor/runtime storage.
2. This contract is internal to CLI/TUI runtime architecture.
3. Existing external JSON formatter API remains unchanged.
4. Dynamic bound editing is deferred and intentionally absent from v1 schema.
