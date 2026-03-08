# Schema Proposal: Diff TUI HTTP State + Impact Panel

## Status
Locked

## 1. Goal
Lock a deterministic local HTTP contract for exposing active Diff TUI state, selected-entity graph metadata, and impact metadata.

## 2. Example Payloads

### 2.1 GET /state Success (200)
```json
{
  "session": {
    "http": {
      "enabled": true,
      "bound": true,
      "host": "127.0.0.1",
      "port": 7778
    },
    "sourceMode": "repository",
    "startedAt": "2026-03-08T21:00:00Z"
  },
  "selection": {
    "selected": true,
    "file": "src/auth.ts",
    "entityType": "function",
    "entityName": "validateToken",
    "lineRange": [42, 88],
    "ui": {
      "mode": "detail",
      "view": "unified",
      "contextMode": "entity",
      "hunkIndex": 1,
      "scroll": 12,
      "anchors": [2, 12]
    }
  },
  "graph": {
    "graphAvailable": true,
    "reason": null,
    "dependencies": [
      {"id": "g:98f5", "name": "parseJwt", "file": "src/auth.ts", "lines": [5, 30]}
    ],
    "dependents": [
      {"id": "g:44b1", "name": "authorize", "file": "src/api.ts", "lines": [101, 150]}
    ]
  },
  "impact": {
    "total": 5,
    "cap": 10000,
    "truncated": false,
    "entities": [
      {"id": "g:44b1", "name": "authorize", "file": "src/api.ts", "lines": [101, 150]}
    ]
  },
  "panel": {
    "expanded": true,
    "summary": "deps:1 depBy:1 impact:5"
  }
}
```

### 2.2 Graph Unavailable / Selection Not Resolvable (200)
```json
{
  "session": {
    "http": {
      "enabled": true,
      "bound": true,
      "host": "127.0.0.1",
      "port": 7778
    },
    "sourceMode": "repository",
    "startedAt": "2026-03-08T21:00:00Z"
  },
  "selection": {
    "selected": false,
    "file": null,
    "entityType": null,
    "entityName": null,
    "lineRange": null,
    "ui": {
      "mode": "list",
      "view": "unified",
      "contextMode": "hunk",
      "hunkIndex": 0,
      "scroll": 0,
      "anchors": [0, 0]
    }
  },
  "graph": {
    "graphAvailable": false,
    "reason": "selectionNotResolvable",
    "dependencies": [],
    "dependents": []
  },
  "impact": {
    "total": 0,
    "cap": 10000,
    "truncated": false,
    "entities": []
  },
  "panel": {
    "expanded": false,
    "summary": "deps:0 depBy:0 impact:0"
  }
}
```

### 2.3 Unknown Route (404)
```json
{
  "error": "notFound",
  "path": "/unknown"
}
```

### 2.4 Method Not Allowed (405)
```json
{
  "error": "methodNotAllowed",
  "path": "/state",
  "method": "POST"
}
```

## 3. JSON Schema Skeleton

### 3.1 Reason Token
```json
{
  "$id": "sem.tui.http.graph-availability-reason.v1",
  "type": ["string", "null"],
  "enum": ["unsupportedSourceMode", "graphBuildFailed", "selectionNotResolvable", null]
}
```

### 3.2 Entity Ref
```json
{
  "$id": "sem.tui.http.entity-ref.v1",
  "type": "object",
  "required": ["id", "name", "file", "lines"],
  "properties": {
    "id": {"type": "string"},
    "name": {"type": "string"},
    "file": {"type": "string"},
    "lines": {
      "type": "array",
      "minItems": 2,
      "maxItems": 2,
      "items": {"type": "integer", "minimum": 1}
    }
  },
  "additionalProperties": false
}
```

### 3.3 State Snapshot
```json
{
  "$id": "sem.tui.http.state-snapshot.v1",
  "type": "object",
  "required": ["session", "selection", "graph", "impact", "panel"],
  "properties": {
    "session": {
      "type": "object",
      "required": ["http", "sourceMode", "startedAt"],
      "properties": {
        "http": {
          "type": "object",
          "required": ["enabled", "bound", "host", "port"],
          "properties": {
            "enabled": {"type": "boolean"},
            "bound": {"type": "boolean"},
            "host": {"type": "string", "const": "127.0.0.1"},
            "port": {"type": "integer", "minimum": 1, "maximum": 65535}
          },
          "additionalProperties": false
        },
        "sourceMode": {"type": "string", "enum": ["repository", "stdin", "twoFile"]},
        "startedAt": {"type": "string", "format": "date-time"}
      },
      "additionalProperties": false
    },
    "selection": {
      "type": "object",
      "required": ["selected", "file", "entityType", "entityName", "lineRange", "ui"],
      "properties": {
        "selected": {"type": "boolean"},
        "file": {"type": ["string", "null"]},
        "entityType": {"type": ["string", "null"]},
        "entityName": {"type": ["string", "null"]},
        "lineRange": {
          "type": ["array", "null"],
          "minItems": 2,
          "maxItems": 2,
          "items": {"type": "integer", "minimum": 1}
        },
        "ui": {
          "type": "object",
          "required": ["mode", "view", "contextMode", "hunkIndex", "scroll", "anchors"],
          "properties": {
            "mode": {"type": "string", "enum": ["list", "detail"]},
            "view": {"type": "string", "enum": ["unified", "sideBySide"]},
            "contextMode": {"type": "string", "enum": ["hunk", "entity"]},
            "hunkIndex": {"type": "integer", "minimum": 0},
            "scroll": {"type": "integer", "minimum": 0},
            "anchors": {
              "type": "array",
              "minItems": 2,
              "maxItems": 2,
              "items": {"type": "integer", "minimum": 0}
            }
          },
          "additionalProperties": false
        }
      },
      "additionalProperties": false
    },
    "graph": {
      "type": "object",
      "required": ["graphAvailable", "reason", "dependencies", "dependents"],
      "properties": {
        "graphAvailable": {"type": "boolean"},
        "reason": {"type": ["string", "null"]},
        "dependencies": {"type": "array", "items": {"$ref": "sem.tui.http.entity-ref.v1"}},
        "dependents": {"type": "array", "items": {"$ref": "sem.tui.http.entity-ref.v1"}}
      },
      "additionalProperties": false
    },
    "impact": {
      "type": "object",
      "required": ["total", "cap", "truncated", "entities"],
      "properties": {
        "total": {"type": "integer", "minimum": 0},
        "cap": {"type": "integer", "minimum": 1},
        "truncated": {"type": "boolean"},
        "entities": {"type": "array", "items": {"$ref": "sem.tui.http.entity-ref.v1"}}
      },
      "additionalProperties": false
    },
    "panel": {
      "type": "object",
      "required": ["expanded", "summary"],
      "properties": {
        "expanded": {"type": "boolean"},
        "summary": {"type": "string", "pattern": "^deps:[0-9]+ depBy:[0-9]+ impact:[0-9]+$"}
      },
      "additionalProperties": false
    }
  },
  "additionalProperties": false
}
```

### 3.4 Not Found Error
```json
{
  "$id": "sem.tui.http.not-found.v1",
  "type": "object",
  "required": ["error", "path"],
  "properties": {
    "error": {"type": "string", "const": "notFound"},
    "path": {"type": "string"}
  },
  "additionalProperties": false
}
```

### 3.5 Method Not Allowed Error
```json
{
  "$id": "sem.tui.http.method-not-allowed.v1",
  "type": "object",
  "required": ["error", "path", "method"],
  "properties": {
    "error": {"type": "string", "const": "methodNotAllowed"},
    "path": {"type": "string", "const": "/state"},
    "method": {"type": "string"}
  },
  "additionalProperties": false
}
```

## 4. Endpoint / Contract Lock
1. `/state` supports `GET` only and returns full snapshot shape.
2. Success response status is `200`; unknown route is `404`; method mismatch on `/state` is `405`.
3. `impact.total` is `min(fullTransitiveDependentCount, impact.cap)`.
4. `impact.truncated=true` iff `fullTransitiveDependentCount > impact.cap`.
5. `panel.summary` format is exactly `deps:<n> depBy:<n> impact:<n>`.
6. Graph entity `id` is opaque and stable per snapshot; consumers must not parse format.
7. Listener bind is localhost-only (`127.0.0.1`).
8. No CORS headers are defined in this topic.
9. `graph.reason` is `null` iff `graph.graphAvailable=true`.
10. `panel.summary` is a compact count display only; graph availability truth is authoritative in `graph.graphAvailable` and `graph.reason`.

## 5. Deterministic Reject / Status Lock
1. If HTTP is disabled, no listener is started and baseline TUI behavior is unchanged.
2. Graph unavailable states must set `graph.graphAvailable=false` with non-null reason token.
3. `selection` and `panel` sections are always present in `200` responses.
4. Empty dependency/dependent/impact arrays are valid and non-error.
5. Bind failure is non-fatal to TUI; endpoint simply remains unavailable.

## 6. Notes
1. This contract is local runtime state, not a remote authenticated API.
2. Existing CLI `--format json` contracts are unchanged.
3. Response impact cap and panel display cap are intentionally separate controls.
4. `session.sourceMode` is input-origin oriented: repository-backed TUI modes map to `repository`; stdin maps to `stdin`; two-file input maps to `twoFile`.
5. Future enhancements deferred: health endpoint, streaming/pagination, remote bind/auth.
