# Schema Proposal: Diff TUI Entity Annotations

## Status
Draft

## 1. Goal
Define runtime and persistence contracts for per-entity text annotations in the TUI, extending the existing review state persistence file.

## 2. Example Payloads

### 2.1 Runtime Add/Replace Annotation Action (internal)
```json
{
  "action": "addAnnotation",
  "entity": {
    "logicalEntityKey": "entityId::src/auth.ts::function::validateToken"
  },
  "annotation": {
    "text": "needs error handling for expired tokens",
    "contentHashAtCreation": "sha256:37f8..."
  },
  "source": {
    "mode": "cumulative",
    "fromEndpointId": "commit:1111111...",
    "toEndpointId": "working"
  }
}
```

### 2.2 Runtime Delete Annotation Action (internal, keybinding: D)
```json
{
  "action": "confirmDeleteAnnotation",
  "entity": {
    "logicalEntityKey": "entityId::src/auth.ts::function::validateToken"
  },
  "confirmation": {
    "armed": true
  }
}
```

### 2.3 Footer Cells (internal)
Add one footer cell for the annotation filter state: `A: all|annotated|unannotated`. Annotation presence is still indicated per-entity via `[*]` / `[~]` badges.

### 2.4 Persistence File Example (extended)
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
    },
    {
      "logicalEntityKey": "entityId::src/db.rs::function::connect",
      "text": "revisit after connection pool PR lands",
      "contentHashAtCreation": "sha256:a1b2...",
      "createdAt": "2026-03-08T19:05:00Z",
      "updatedAt": "2026-03-08T19:05:00Z"
    }
  ]
}
```

## 3. JSON Schema Definitions

### 3.1 Annotation Record (persistence)
```json
{
  "$id": "sem.tui.annotation-record.v1",
  "type": "object",
  "required": ["logicalEntityKey", "text", "createdAt", "updatedAt"],
  "properties": {
    "logicalEntityKey": {
      "type": "string",
      "minLength": 1,
      "description": "Stable entity identity key using entityId:: or fallback:: grammar from review state"
    },
    "text": {
      "type": "string",
      "minLength": 1,
      "maxLength": 256,
      "description": "User-authored annotation text, single line"
    },
    "contentHashAtCreation": {
      "type": "string",
      "pattern": "^sha256:[0-9a-f]{64}$",
      "description": "SHA256 hash of normalized entity content when annotation was created; provenance metadata, not used for matching. Optional — absent when hash material was unavailable."
    },
    "createdAt": {
      "type": "string",
      "pattern": "^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}Z$",
      "description": "ISO 8601 UTC timestamp of initial creation"
    },
    "updatedAt": {
      "type": "string",
      "pattern": "^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}Z$",
      "description": "ISO 8601 UTC timestamp of last update (replace)"
    }
  },
  "additionalProperties": false
}
```

### 3.2 Extended Persistence File
```json
{
  "$id": "sem.tui.review-state.file.v1.extended",
  "type": "object",
  "required": ["version", "repoId", "reviewRecords"],
  "properties": {
    "version": { "type": "integer", "const": 1 },
    "repoId": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" },
    "uiPrefs": {
      "type": "object",
      "properties": {
        "reviewFilter": { "type": "string", "enum": ["all", "unreviewed", "reviewed"] },
        "viewMode": { "type": "string", "enum": ["list", "split", "detail"] },
        "diffView": { "type": "string", "enum": ["unified", "side-by-side"] },
        "entityContextMode": { "type": "string", "enum": ["hunk", "entity"] },
        "annotationFilter": { "type": "string", "enum": ["all", "annotated", "unannotated"] }
      }
    },
    "reviewRecords": {
      "type": "array",
      "items": { "$ref": "sem.tui.review-identity.v1" }
    },
    "annotations": {
      "type": "array",
      "items": { "$ref": "sem.tui.annotation-record.v1" },
      "default": [],
      "description": "Optional; missing or empty array treated as no annotations"
    }
  },
  "additionalProperties": false
}
```

## 4. Runtime Structs (Rust)

### 4.0 AnnotationFilter
```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnnotationFilter {
    #[default]
    All,
    Annotated,
    Unannotated,
}

impl AnnotationFilter {
    pub fn cycle(self) -> Self {
        match self {
            Self::All => Self::Annotated,
            Self::Annotated => Self::Unannotated,
            Self::Unannotated => Self::All,
        }
    }

    pub fn as_token(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Annotated => "annotated",
            Self::Unannotated => "unannotated",
        }
    }
}
```

### 4.1 Annotation
```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Annotation {
    pub text: String,
    pub content_hash_at_creation: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
```

### 4.2 PersistedAnnotationRecord (serde)
```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedAnnotationRecord {
    logical_entity_key: String,
    text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content_hash_at_creation: Option<String>,
    created_at: String,
    updated_at: String,
}
```

### 4.3 AnnotationInputState
```rust
#[derive(Clone, Debug)]
struct AnnotationInputState {
    text: String,
    cursor_position: usize,
    target_logical_entity_key: String,
    target_content_hash: Option<String>,
}
```

### 4.5 Extended ReviewStateData
```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReviewStateData {
    pub filter: ReviewFilter,
    pub annotation_filter: AnnotationFilter,
    pub ui_prefs: ReviewStateUiPrefs,
    pub records: HashMap<ReviewIdentity, String>,
    pub annotations: HashMap<String, Annotation>,  // keyed by logicalEntityKey
}
```

## 5. Compatibility Notes

### 5.1 Forward Compatibility
- The `annotations` field uses `#[serde(default)]` so existing files without annotations load cleanly.
- No schema version bump required — the field is additive and optional.

### 5.2 Backward Compatibility
- Older sem versions that do not recognize `annotations` will ignore the field (standard JSON behavior with `additionalProperties` or lenient deserialization).
- Review records remain unchanged.

### 5.3 Multi-Instance
- Same last-writer-wins semantics as review records.
- Annotation text from concurrent sessions may overwrite each other. Accepted limitation in v1.

## 6. Key Differences from Review Records

| Aspect | Review Records | Annotations |
|--------|---------------|-------------|
| Key | `logicalEntityKey` + `targetContentHash` (composite) | `logicalEntityKey` only |
| Value | `updatedAt` timestamp (boolean "reviewed" signal) | Structured: text, timestamps, provenance hash |
| Matching | Exact match on both key components | Entity key match only |
| Content sensitivity | Invalidated when content changes | Carried forward regardless of content changes |
| Endpoint gating | Gated by `endpoint_supports_review_hash()` | Not gated — works for all source modes |
| Compaction | Yes, 20,000 cap with oldest eviction | No automatic compaction |
| Cardinality | Many records per entity (one per content version) | One annotation per entity |
