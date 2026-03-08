use colored::Colorize;
use sem_core::model::change::ChangeType;
use sem_core::parser::differ::DiffResult;
use std::collections::BTreeMap;

fn range_label(change: &sem_core::model::change::SemanticChange) -> Option<String> {
    match (
        change.before_start_line,
        change.before_end_line,
        change.after_start_line,
        change.after_end_line,
    ) {
        (Some(before_start), Some(before_end), Some(after_start), Some(after_end)) => {
            Some(format!("[L{before_start}-L{before_end} -> L{after_start}-L{after_end}]"))
        }
        (Some(before_start), Some(before_end), None, None) => {
            Some(format!("[L{before_start}-L{before_end}]"))
        }
        (None, None, Some(after_start), Some(after_end)) => {
            Some(format!("[L{after_start}-L{after_end}]"))
        }
        _ => None,
    }
}

pub fn format_terminal(result: &DiffResult) -> String {
    if result.changes.is_empty() {
        return "No semantic changes detected.".dimmed().to_string();
    }

    let mut lines: Vec<String> = Vec::new();

    // Group changes by file (BTreeMap for sorted output)
    let mut by_file: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, change) in result.changes.iter().enumerate() {
        by_file.entry(&change.file_path).or_default().push(i);
    }

    for (file_path, indices) in &by_file {
        let header = format!("─ {file_path} ");
        let pad_len = 55usize.saturating_sub(header.len());
        lines.push(format!("┌{header}{}", "─".repeat(pad_len)).dimmed().to_string());
        lines.push("│".dimmed().to_string());

        for &idx in indices {
            let change = &result.changes[idx];
            let (symbol, tag) = match change.change_type {
                ChangeType::Added => (
                    "⊕".green().to_string(),
                    "[added]".green().to_string(),
                ),
                ChangeType::Modified => {
                    let is_cosmetic = change.structural_change == Some(false);
                    if is_cosmetic {
                        (
                            "~".dimmed().to_string(),
                            "[cosmetic]".dimmed().to_string(),
                        )
                    } else {
                        (
                            "∆".yellow().to_string(),
                            "[modified]".yellow().to_string(),
                        )
                    }
                }
                ChangeType::Deleted => (
                    "⊖".red().to_string(),
                    "[deleted]".red().to_string(),
                ),
                ChangeType::Moved => (
                    "→".blue().to_string(),
                    "[moved]".blue().to_string(),
                ),
                ChangeType::Renamed => (
                    "↻".cyan().to_string(),
                    "[renamed]".cyan().to_string(),
                ),
            };

            let type_label = format!("{:<10}", change.entity_type);
            let name_label = format!("{:<25}", change.entity_name);
            let range = range_label(change)
                .map(|label| format!(" {label}").dimmed().to_string())
                .unwrap_or_default();

            lines.push(format!(
                "{}  {} {} {} {}{}",
                "│".dimmed(),
                symbol,
                type_label.dimmed(),
                name_label.bold(),
                tag,
                range,
            ));

            // Show content diff for modified properties
            if change.change_type == ChangeType::Modified {
                if let (Some(before), Some(after)) =
                    (&change.before_content, &change.after_content)
                {
                    let before_lines: Vec<&str> = before.lines().collect();
                    let after_lines: Vec<&str> = after.lines().collect();

                    if before_lines.len() <= 3 && after_lines.len() <= 3 {
                        for line in &before_lines {
                            lines.push(format!(
                                "{}    {}",
                                "│".dimmed(),
                                format!("- {}", line.trim()).red(),
                            ));
                        }
                        for line in &after_lines {
                            lines.push(format!(
                                "{}    {}",
                                "│".dimmed(),
                                format!("+ {}", line.trim()).green(),
                            ));
                        }
                    }
                }
            }

            // Show rename/move details
            if matches!(
                change.change_type,
                ChangeType::Renamed | ChangeType::Moved
            ) {
                if let Some(ref old_path) = change.old_file_path {
                    lines.push(format!(
                        "{}    {}",
                        "│".dimmed(),
                        format!("from {old_path}").dimmed(),
                    ));
                }
            }
        }

        lines.push("│".dimmed().to_string());
        lines.push(format!("└{}", "─".repeat(55)).dimmed().to_string());
        lines.push(String::new());
    }

    // Summary
    let mut parts: Vec<String> = Vec::new();
    if result.added_count > 0 {
        parts.push(format!("{} added", result.added_count).green().to_string());
    }
    if result.modified_count > 0 {
        parts.push(
            format!("{} modified", result.modified_count)
                .yellow()
                .to_string(),
        );
    }
    if result.deleted_count > 0 {
        parts.push(format!("{} deleted", result.deleted_count).red().to_string());
    }
    if result.moved_count > 0 {
        parts.push(format!("{} moved", result.moved_count).blue().to_string());
    }
    if result.renamed_count > 0 {
        parts.push(
            format!("{} renamed", result.renamed_count)
                .cyan()
                .to_string(),
        );
    }

    let files_label = if result.file_count == 1 {
        "file"
    } else {
        "files"
    };

    lines.push(format!(
        "Summary: {} across {} {files_label}",
        parts.join(", "),
        result.file_count,
    ));

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sem_core::model::change::{ChangeType, SemanticChange};

    #[test]
    fn format_terminal_shows_range_labels_when_present() {
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
                commit_sha: None,
                author: None,
                timestamp: None,
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

        let output = format_terminal(&result);
        assert!(output.contains("[L10-L12 -> L10-L14]"));
    }
}
