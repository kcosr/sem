# Schema Proposal: Diff TUI File-Scope Navigation

## Status
Locked

## 1. Goal
Lock runtime contracts for selectable file rows and scope-aware diff rendering while preserving existing entity behavior and CLI surface.

## 2. Contract Surface

### 2.1 CLI Inputs
No new flags.

Primary command remains:
- `sem diff --tui`

### 2.2 Runtime Inputs
- `j/k` / arrows: move across scope rows allowed by the active navigation mode
- `Enter`: open detail for selected row scope
- `e`: toggle hunk/full for selected scope
- `f`: cycle navigation mode (`mixed`, `entity`, `file`)
- `n/p`: navigate hunks for selected scope when hunk anchors exist
- `a` / `D`: annotation actions for entity rows only; no-op on file rows
- `A`: annotation filter remains entity-scoped and affects file-row visibility indirectly through child rows
- `Left/Right` in detail: move to previous/next scope row allowed by active navigation mode

## 3. Runtime Scope Model (Conceptual)

### 3.1 File Scope Selection
```json
{
  "selectedRow": {
    "scopeKind": "file",
    "filePath": "src/auth.ts",
    "entityId": null
  },
  "scopeRenderMode": "hunk",
  "renderedDetail": {
    "scope": "file",
    "mode": "hunk",
    "source": "fileDiff"
  }
}
```

### 3.2 Entity Scope Selection
```json
{
  "selectedRow": {
    "scopeKind": "entity",
    "filePath": "src/auth.ts",
    "entityId": "src/auth.ts::function::validateToken"
  },
  "scopeRenderMode": "entity",
  "renderedDetail": {
    "scope": "entity",
    "mode": "entity",
    "source": "entityDiff"
  }
}
```

Notes:
1. This is a documentation-only runtime reference model, not an external endpoint payload.
2. Non-TUI JSON output remains unchanged.

## 4. JSON Schema Skeleton (Runtime Contract Model)
```json
{
  "$id": "sem.tui.file-scope.contract.v1",
  "type": "object",
  "required": [
    "rowScope",
    "scopeToggle",
    "navigationMode",
    "entityScopeBehavior",
    "fileScopeBehavior",
    "dataInputs",
    "filterBehavior",
    "detailNavigation"
  ],
  "properties": {
    "rowScope": {
      "type": "string",
      "enum": ["file", "entity"]
    },
    "scopeToggle": {
      "type": "object",
      "required": ["key", "modes"],
      "properties": {
        "key": {"type": "string", "const": "e"},
        "modes": {"type": "array", "const": ["hunk", "entity"]}
      }
    },
    "navigationMode": {
      "type": "object",
      "required": ["key", "modes", "persisted"],
      "properties": {
        "key": {"type": "string", "const": "f"},
        "modes": {"type": "array", "const": ["mixed", "entity", "file"]},
        "persisted": {"type": "boolean", "const": true}
      }
    },
    "entityScopeBehavior": {
      "type": "object",
      "required": ["hunkMode", "entityMode"],
      "properties": {
        "hunkMode": {"type": "string", "const": "groupedEntityHunks"},
        "entityMode": {"type": "string", "const": "fullEntityDiff"}
      }
    },
    "fileScopeBehavior": {
      "type": "object",
      "required": [
        "hunkMode",
        "entityMode",
        "hunkOrdering",
        "reviewable",
        "reviewAggregation",
        "annotatable"
      ],
      "properties": {
        "hunkMode": {"type": "string", "const": "groupedFileHunks"},
        "entityMode": {"type": "string", "const": "fullFileDiff"},
        "hunkOrdering": {"type": "string", "const": "fileLineOrderAscending"},
        "reviewable": {"type": "boolean", "const": true},
        "reviewAggregation": {
          "type": "string",
          "const": "derivedFromChildEntityReviewRecords"
        },
        "annotatable": {"type": "boolean", "const": false}
      }
    },
    "dataInputs": {
      "type": "object",
      "required": ["fileSnapshotsFromFileChange", "snapshotMapUsedByTui", "stepSnapshotsCarryFileSnapshots"],
      "properties": {
        "fileSnapshotsFromFileChange": {"type": "boolean", "const": true},
        "snapshotMapUsedByTui": {"type": "boolean", "const": true},
        "stepSnapshotsCarryFileSnapshots": {"type": "boolean", "const": true}
      }
    },
    "filterBehavior": {
      "type": "object",
      "required": ["annotationScope", "fileVisibilityRule"],
      "properties": {
        "annotationScope": {"type": "string", "const": "entityOnly"},
        "fileVisibilityRule": {
          "type": "string",
          "const": "showFileRowWhenAtLeastOneChildEntityIsVisible"
        }
      }
    },
    "detailNavigation": {
      "type": "object",
      "required": ["leftRightBehavior", "upDownBehavior"],
      "properties": {
        "leftRightBehavior": {
          "type": "string",
          "const": "moveAcrossScopeRowsAllowedByNavigationMode"
        },
        "upDownBehavior": {
          "type": "string",
          "const": "moveAcrossScopeRowsAllowedByNavigationMode"
        }
      }
    }
  }
}
```

## 5. Endpoint / Contract Lock
1. No new CLI endpoint or flag is added.
2. TUI row model includes selectable file rows.
3. File detail rendering semantics are locked by scope mode.
4. File review state is aggregate-only; file rows do not create persisted file review records.
5. Annotation behavior remains entity-scoped; file rows do not create annotation records.

## 6. Deterministic Reject / Status Lock
1. Selected file row without snapshot data must not panic; it renders placeholder.
2. File row rendered with entity-scope semantics is contract violation.
3. Entity row behavior regression under `e` toggle is contract violation.
4. File hunk ordering that is not line-ascending is contract violation.

## 7. Notes
1. Parent/class-level scope rows are out of scope.
2. This topic intentionally keeps hierarchy depth to file + entity.
3. Lazy-loading or caching optimizations are deferred unless baseline evidence fails thresholds.
