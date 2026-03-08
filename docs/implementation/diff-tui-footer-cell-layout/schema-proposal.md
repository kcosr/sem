# Schema Proposal: Diff TUI Footer Cell Layout

## Status
Locked

## 1. Goal
Define an internal footer-cell rendering contract that standardizes mode/filter/context indicators and status messaging.

## 2. Example Payloads

### 2.1 Footer Snapshot (baseline)
```json
{
  "cells": [
    { "key": "m", "value": "pairwise", "rendered": "m: pairwise" }
  ],
  "status": "Loading..."
}
```

### 2.2 Footer Snapshot (future-extended)
```json
{
  "cells": [
    { "key": "m", "value": "cumulative", "rendered": "m: cumulative" },
    { "key": "r", "value": "unreviewed", "rendered": "r: unreviewed" },
    { "key": "e", "value": "entity", "rendered": "e: entity" }
  ],
  "status": null
}
```

## 3. JSON Schema Skeleton

### 3.1 Footer Cell
```json
{
  "$id": "sem.tui.footer.cell.v1",
  "type": "object",
  "required": ["key", "value", "rendered"],
  "properties": {
    "key": { "type": "string", "enum": ["m", "r", "e"] },
    "value": { "type": "string", "minLength": 1 },
    "rendered": { "type": "string", "pattern": "^[mre]: .+$" }
  },
  "additionalProperties": false
}
```

### 3.2 Footer Snapshot
```json
{
  "$id": "sem.tui.footer.snapshot.v1",
  "type": "object",
  "required": ["cells", "status"],
  "properties": {
    "cells": {
      "type": "array",
      "items": { "$ref": "sem.tui.footer.cell.v1" }
    },
    "status": { "type": ["string", "null"] }
  },
  "additionalProperties": false
}
```

## 4. Endpoint / Contract Lock
1. Cell render format is exactly `<key>: <value>` in lowercase key form.
2. Cell order is canonical and stable: `m`, then `r`, then `e`.
3. Baseline implementation may render only `m` while preserving ordering contract.
4. `m` value domain is `pairwise|cumulative`.
5. Status text is independent from cells and is rendered in dedicated status slot.

## 5. Deterministic Reject / Status Lock
1. Unknown cell key is ignored in rendering (non-fatal) and must not reorder known keys.
2. Empty status is represented by `null` and results in no status slot text.
3. Missing `m` cell in baseline is treated as UI fallback `m: pairwise`.

## 6. Notes
1. This contract is internal to Rust TUI runtime.
2. `rendered` is documentation convenience and may be derived from `key` + `value`.
3. This addendum implements `m`; `r` and `e` are reserved for downstream topics.
