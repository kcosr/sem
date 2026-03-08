use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Moved,
    Renamed,
}

impl std::fmt::Display for ChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeType::Added => write!(f, "added"),
            ChangeType::Modified => write!(f, "modified"),
            ChangeType::Deleted => write!(f, "deleted"),
            ChangeType::Moved => write!(f, "moved"),
            ChangeType::Renamed => write!(f, "renamed"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticChange {
    pub id: String,
    pub entity_id: String,
    pub change_type: ChangeType,
    pub entity_type: String,
    pub entity_name: String,
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Whether the AST structure changed (true) or only formatting/comments (false).
    /// None when structural hash is unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structural_change: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_start_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_end_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_start_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_end_line: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_change_serializes_range_fields_as_camel_case() {
        let change = SemanticChange {
            id: "change::a".to_string(),
            entity_id: "a::function::foo".to_string(),
            change_type: ChangeType::Modified,
            entity_type: "function".to_string(),
            entity_name: "foo".to_string(),
            file_path: "src/a.ts".to_string(),
            old_file_path: None,
            before_content: Some("before".to_string()),
            after_content: Some("after".to_string()),
            commit_sha: None,
            author: None,
            timestamp: Some("2026-03-08T00:00:00Z".to_string()),
            structural_change: Some(true),
            before_start_line: Some(10),
            before_end_line: Some(15),
            after_start_line: Some(11),
            after_end_line: Some(17),
        };

        let value = serde_json::to_value(change).expect("change should serialize");
        assert_eq!(
            value.get("beforeStartLine").and_then(|v| v.as_u64()),
            Some(10)
        );
        assert_eq!(
            value.get("beforeEndLine").and_then(|v| v.as_u64()),
            Some(15)
        );
        assert_eq!(
            value.get("afterStartLine").and_then(|v| v.as_u64()),
            Some(11)
        );
        assert_eq!(value.get("afterEndLine").and_then(|v| v.as_u64()), Some(17));
        assert_eq!(
            value.get("timestamp").and_then(|v| v.as_str()),
            Some("2026-03-08T00:00:00Z")
        );
        assert_eq!(
            value.get("structuralChange").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert!(value.get("before_start_line").is_none());
    }

    #[test]
    fn test_semantic_change_omits_none_optional_fields() {
        let change = SemanticChange {
            id: "change::b".to_string(),
            entity_id: "b::function::bar".to_string(),
            change_type: ChangeType::Added,
            entity_type: "function".to_string(),
            entity_name: "bar".to_string(),
            file_path: "src/b.ts".to_string(),
            old_file_path: None,
            before_content: None,
            after_content: Some("after".to_string()),
            commit_sha: None,
            author: None,
            timestamp: None,
            structural_change: None,
            before_start_line: None,
            before_end_line: None,
            after_start_line: Some(1),
            after_end_line: Some(1),
        };

        let value = serde_json::to_value(change).expect("change should serialize");
        assert!(value.get("beforeStartLine").is_none());
        assert!(value.get("beforeEndLine").is_none());
        assert!(value.get("timestamp").is_none());
        assert!(value.get("structuralChange").is_none());
        assert_eq!(
            value.get("afterStartLine").and_then(|v| v.as_u64()),
            Some(1)
        );
        assert_eq!(value.get("afterEndLine").and_then(|v| v.as_u64()), Some(1));
    }
}
