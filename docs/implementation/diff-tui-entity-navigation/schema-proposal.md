# Schema Proposal: Diff TUI Entity Navigation

## Status
Locked

## 1. Goal
Lock CLI and JSON contracts required for TUI diff navigation and entity range labeling while preserving backward compatibility.

## 2. Example CLI Request/Response Semantics

### 2.1 CLI Invocation Examples
1. `sem diff --tui`
2. `sem diff --tui --diff-view unified`
3. `sem diff --tui --diff-view side-by-side`
4. `sem diff --tui --stdin`
5. `sem diff fileA.ts fileB.ts --tui`

### 2.2 Deterministic Invalid Examples
1. `sem diff --tui --format json`
2. `sem diff --tui --format terminal`
3. `sem diff --tui --diff-view diagonal`

Expected behavior for each invalid example:
- exit non-zero
- deterministic argument error message

### 2.3 Default Lock
When `--tui` is set and `--diff-view` is omitted, default is `unified`.

## 3. JSON Schema Skeleton (Diff Output Extension)

```json
{
  "$id": "sem.diff.result.vNext",
  "type": "object",
  "required": ["summary", "changes"],
  "properties": {
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
      "items": {
        "type": "object",
        "required": ["entityId", "changeType", "entityType", "entityName", "filePath"],
        "properties": {
          "id": { "type": ["string", "null"] },
          "entityId": { "type": "string" },
          "changeType": { "type": "string", "enum": ["added", "modified", "deleted", "moved", "renamed"] },
          "entityType": { "type": "string" },
          "entityName": { "type": "string" },
          "filePath": { "type": "string" },
          "oldFilePath": { "type": ["string", "null"] },
          "beforeContent": { "type": ["string", "null"] },
          "afterContent": { "type": ["string", "null"] },
          "commitSha": { "type": ["string", "null"] },
          "author": { "type": ["string", "null"] },
          "timestamp": { "type": ["string", "null"] },
          "structuralChange": { "type": ["boolean", "null"] },
          "beforeStartLine": { "type": ["integer", "null"], "minimum": 1 },
          "beforeEndLine": { "type": ["integer", "null"], "minimum": 1 },
          "afterStartLine": { "type": ["integer", "null"], "minimum": 1 },
          "afterEndLine": { "type": ["integer", "null"], "minimum": 1 }
        },
        "additionalProperties": true
      }
    }
  },
  "additionalProperties": false
}
```

## 4. Endpoint / Contract Lock
This is a local CLI contract (not HTTP endpoint).

Locked contract points:
1. `--tui` launches interactive mode.
2. `--diff-view` accepted values: `unified`, `side-by-side`.
3. `--tui` is incompatible with `--format` (any value).
4. `--tui` works with git mode, `--stdin`, and two-file compare mode if semantic result generation succeeds.
5. JSON output may include optional range fields per change.
6. Existing fields and summary semantics remain unchanged.

## 5. Deterministic Reject / Status Lock
1. Reject invalid `--diff-view` values with non-zero exit.
2. Reject any `--tui` + `--format <...>` combination with non-zero exit.
3. If `--tui` requested but no changes found, print existing no-change message and exit zero (no TUI session).
4. Side-by-side on narrow terminal does not reject; fallback to unified and show status hint.
5. If binary/non-UTF8 diff content cannot be rendered, show non-fatal placeholder in detail pane.

## 6. Notes
1. Line fields are optional to preserve backward compatibility with old producers/consumers.
2. Added/deleted changes legitimately have one-sided line metadata only.
3. Absolute hunk headers in TUI derive from base line fields plus entity-local hunk offsets.
4. Naming convention:
   - Rust fields: snake_case.
   - JSON output: camelCase.
5. `changes.items.additionalProperties = true` remains intentionally permissive for forward-compatible per-change evolution.
