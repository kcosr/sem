use sem_core::parser::differ::DiffResult;
use serde_json::json;

pub fn format_json(result: &DiffResult) -> String {
    let changes: Vec<serde_json::Value> = result
        .changes
        .iter()
        .map(|change| {
            serde_json::to_value(change)
                .expect("SemanticChange serialization should always succeed")
        })
        .collect();

    let output = json!({
        "summary": {
            "fileCount": result.file_count,
            "added": result.added_count,
            "modified": result.modified_count,
            "deleted": result.deleted_count,
            "moved": result.moved_count,
            "renamed": result.renamed_count,
            "total": result.changes.len(),
        },
        "changes": changes,
    });

    serde_json::to_string_pretty(&output).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sem_core::model::change::{ChangeType, SemanticChange};

    #[test]
    fn format_json_includes_optional_line_and_existing_optional_fields() {
        let result = DiffResult {
            changes: vec![SemanticChange {
                id: "change::x".to_string(),
                entity_id: "src/lib.rs::function::run".to_string(),
                change_type: ChangeType::Modified,
                entity_type: "function".to_string(),
                entity_name: "run".to_string(),
                file_path: "src/lib.rs".to_string(),
                old_file_path: None,
                before_content: Some("fn run() {}".to_string()),
                after_content: Some("fn run(v: i32) {}".to_string()),
                commit_sha: Some("abc123".to_string()),
                author: Some("kevin".to_string()),
                timestamp: Some("2026-03-08T00:00:00Z".to_string()),
                structural_change: Some(true),
                before_start_line: Some(10),
                before_end_line: Some(12),
                after_start_line: Some(10),
                after_end_line: Some(14),
            }],
            file_count: 1,
            added_count: 0,
            modified_count: 1,
            deleted_count: 0,
            moved_count: 0,
            renamed_count: 0,
        };

        let parsed: serde_json::Value =
            serde_json::from_str(&format_json(&result)).expect("json should parse");
        let change = &parsed["changes"][0];
        assert_eq!(change["id"], "change::x");
        assert_eq!(change["timestamp"], "2026-03-08T00:00:00Z");
        assert_eq!(change["structuralChange"], true);
        assert_eq!(change["beforeStartLine"], 10);
        assert_eq!(change["afterEndLine"], 14);
    }
}
