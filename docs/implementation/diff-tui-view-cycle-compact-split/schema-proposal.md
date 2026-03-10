# Schema Proposal: Diff TUI View Cycle + Compact Split

## Status
Locked

## 1. Goal
Lock the TUI runtime contract for three-view navigation (`list`, `split`, `detail`) and compact split rendering semantics without changing CLI surface area.

## 2. Contract Surface

### 2.1 CLI Inputs
No new flags are introduced.

Primary entry remains:
- `sem diff --tui`
- optional existing flags remain valid (`--diff-view`, `--step-mode`, git/range inputs).

### 2.2 Runtime Inputs (Keyboard)
- `v`: cycle view mode in fixed order: `list -> split -> detail -> list`
- `Enter`: from `list` or `split`, open `detail`
- `Esc`: from `detail`, return to previous non-detail mode
- `Tab` in `detail` and `split`: toggle unified/side-by-side diff view
  - in `split`, this toggles preview renderer only and does not mutate detail hunk/scroll cursor state
- `n/p`, `PageUp/PageDown`, and line-scroll keys are detail-only; in `split` they are no-op

### 2.3 Runtime Output Shape (Conceptual)
```json
{
  "viewMode": "split",
  "selection": {
    "filePath": "src/auth.ts",
    "entityName": "validateToken"
  },
  "split": {
    "leftPane": {
      "layout": "compact",
      "grouping": "file",
      "rowFields": ["marker", "changeIcon", "entityIcon", "entityName", "deltaInline"],
      "truncation": "singleLineEllipsis"
    },
    "rightPane": {
      "source": "activeEntityDiff",
      "focusMode": "passivePreview",
      "diffView": "unified"
    }
  },
  "footerCells": ["m: pairwise", "r: all", "e: hunk", "v: split"]
}
```

Notes:
1. This is a documentation-only runtime reference model.
2. It does not require introducing a serialized endpoint payload or a new exported state struct.
3. Existing non-TUI JSON output is unchanged.
4. Split preview content is derived from active sidebar selection; split does not introduce an independent diff-scroll cursor.

## 3. Example Interaction Flows

### 3.1 List to Split to Detail (cycle)
```json
[
  {"before": "list", "key": "v", "after": "split"},
  {"before": "split", "key": "v", "after": "detail"}
]
```

### 3.2 Split to Detail and Back
```json
[
  {"before": "split", "key": "Enter", "after": "detail"},
  {"before": "detail", "key": "Esc", "after": "split"}
]
```

### 3.3 Detail Cycle Wrap (inclusive-of-wrap representation)
```json
[
  {"before": "detail", "key": "v", "after": "list"}
]
```

## 4. JSON Schema Skeleton (Runtime Contract Model)
```json
{
  "$id": "sem.tui.view-mode.contract.v1",
  "type": "object",
  "required": ["viewMode", "splitCompact", "transitions", "footer", "layout"],
  "properties": {
    "viewMode": {
      "type": "string",
      "enum": ["list", "split", "detail"]
    },
    "splitCompact": {
      "type": "object",
      "required": ["groupBy", "rowFields", "omitFields", "focusMode"],
      "properties": {
        "groupBy": {"type": "string", "enum": ["file"]},
        "rowFields": {
          "type": "array",
          "const": ["marker", "changeIcon", "entityIcon", "entityName", "deltaInline"]
        },
        "omitFields": {
          "type": "array",
          "const": ["entityTypeText", "changeTagText"]
        },
        "focusMode": {"type": "string", "const": "leftPaneActive"}
      }
    },
    "transitions": {
      "type": "object",
      "required": ["cycleKey", "cycleOrderInclusiveWrap", "enterFrom", "escFromDetailReturnsToPreviousNonDetail"],
      "properties": {
        "cycleKey": {"type": "string", "const": "v"},
        "cycleOrderInclusiveWrap": {
          "type": "array",
          "const": ["list", "split", "detail", "list"]
        },
        "enterFrom": {
          "type": "array",
          "const": ["list", "split"]
        },
        "escFromDetailReturnsToPreviousNonDetail": {"type": "boolean", "const": true}
      }
    },
    "footer": {
      "type": "object",
      "required": ["cellKey", "valueDomain", "cellOrder"],
      "properties": {
        "cellKey": {"type": "string", "const": "v"},
        "valueDomain": {
          "type": "array",
          "const": ["list", "split", "detail"]
        },
        "cellOrder": {
          "type": "array",
          "const": ["m", "r", "e", "v"]
        }
      }
    },
    "layout": {
      "type": "object",
      "required": ["targetRatio", "minLeftCols", "minRightCols", "narrowFallback"],
      "properties": {
        "targetRatio": {"type": "string", "const": "30/70"},
        "minLeftCols": {"type": "integer", "const": 28},
        "minRightCols": {"type": "integer", "const": 52},
        "narrowFallback": {"type": "string", "const": "splitCompactListOnlyWithNotice"}
      }
    }
  }
}
```

## 5. Endpoint / Contract Lock
1. No new CLI flag or endpoint is added.
2. Existing `sem diff --tui` invocation remains canonical.
3. `v` is reserved for view-cycle semantics in this topic.
4. Split layout compact row composition is locked to avoid column drift.

## 6. Deterministic Reject / Status Lock
1. Unknown view token is invalid contract state.
2. Missing `v` footer cell in runtime footer rail is contract violation.
3. Split mode rendering that includes verbose text columns (`entityTypeText`, `changeTagText`) is contract violation.
4. `Esc` from detail returning to a mode other than previous non-detail is contract violation.
5. Split mode with dual-focus keyboard model in v1 is contract violation.
6. Split mode where `n/p` or paging keys mutate preview cursor state is contract violation.

## 7. Notes
1. This proposal intentionally keeps non-TUI JSON output unchanged.
2. Class-hierarchy nesting remains out of scope for this topic.
3. Split ratio tuning beyond locked defaults is deferred.
