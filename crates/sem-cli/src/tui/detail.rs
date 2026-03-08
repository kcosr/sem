use sem_core::model::change::SemanticChange;
use similar::{ChangeTag, TextDiff};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Header,
    Added,
    Removed,
    Unchanged,
    Modified,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SideBySideLine {
    pub left_number: Option<usize>,
    pub left_text: String,
    pub right_number: Option<usize>,
    pub right_text: String,
    pub kind: LineKind,
}

#[derive(Clone, Debug)]
pub struct RenderedDiff {
    pub unified_lines: Vec<(LineKind, String)>,
    pub side_by_side_lines: Vec<SideBySideLine>,
    pub unified_hunks: Vec<usize>,
    pub side_by_side_hunks: Vec<usize>,
}

impl RenderedDiff {
    pub fn unavailable() -> Self {
        Self {
            unified_lines: vec![(LineKind::Unchanged, "content unavailable".to_string())],
            side_by_side_lines: vec![SideBySideLine {
                left_number: None,
                left_text: "content unavailable".to_string(),
                right_number: None,
                right_text: String::new(),
                kind: LineKind::Unchanged,
            }],
            unified_hunks: vec![0],
            side_by_side_hunks: vec![0],
        }
    }
}

pub fn render_change(change: &SemanticChange) -> RenderedDiff {
    let before = change.before_content.as_deref().unwrap_or("");
    let after = change.after_content.as_deref().unwrap_or("");

    if before.is_empty() && after.is_empty() {
        return RenderedDiff::unavailable();
    }

    let diff = TextDiff::from_lines(before, after);
    let groups = diff.grouped_ops(3);

    if groups.is_empty() {
        return RenderedDiff::unavailable();
    }

    let mut unified_lines: Vec<(LineKind, String)> = Vec::new();
    let mut unified_hunks: Vec<usize> = Vec::new();
    let mut side_by_side_lines: Vec<SideBySideLine> = Vec::new();
    let mut side_by_side_hunks: Vec<usize> = Vec::new();

    let base_old = change.before_start_line.unwrap_or(1);
    let base_new = change.after_start_line.unwrap_or(1);

    for group in groups {
        let old_start = base_old.saturating_add(group[0].old_range().start);
        let new_start = base_new.saturating_add(group[0].new_range().start);
        let old_count: usize = group.iter().map(|op| op.old_range().len()).sum();
        let new_count: usize = group.iter().map(|op| op.new_range().len()).sum();
        let mut old_line = old_start;
        let mut new_line = new_start;

        let header = format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@");
        unified_hunks.push(unified_lines.len());
        unified_lines.push((LineKind::Header, header.clone()));

        side_by_side_hunks.push(side_by_side_lines.len());
        side_by_side_lines.push(SideBySideLine {
            left_number: None,
            left_text: header.clone(),
            right_number: None,
            right_text: String::new(),
            kind: LineKind::Header,
        });

        let mut pending_removed: Vec<(usize, String)> = Vec::new();
        let mut pending_added: Vec<(usize, String)> = Vec::new();

        for op in group {
            for diff_change in diff.iter_changes(&op) {
                let text = diff_change.value().trim_end_matches('\n').to_string();
                match diff_change.tag() {
                    ChangeTag::Delete => {
                        unified_lines.push((LineKind::Removed, format!("- {text}")));
                        pending_removed.push((old_line, text));
                        old_line = old_line.saturating_add(line_count(diff_change.value()));
                    }
                    ChangeTag::Insert => {
                        unified_lines.push((LineKind::Added, format!("+ {text}")));
                        pending_added.push((new_line, text));
                        new_line = new_line.saturating_add(line_count(diff_change.value()));
                    }
                    ChangeTag::Equal => {
                        flush_pending(
                            &mut side_by_side_lines,
                            &mut pending_removed,
                            &mut pending_added,
                        );
                        unified_lines.push((LineKind::Unchanged, format!("  {text}")));
                        side_by_side_lines.push(SideBySideLine {
                            left_number: Some(old_line),
                            left_text: text.clone(),
                            right_number: Some(new_line),
                            right_text: text,
                            kind: LineKind::Unchanged,
                        });
                        old_line = old_line.saturating_add(line_count(diff_change.value()));
                        new_line = new_line.saturating_add(line_count(diff_change.value()));
                    }
                }
            }
        }

        flush_pending(
            &mut side_by_side_lines,
            &mut pending_removed,
            &mut pending_added,
        );
    }

    RenderedDiff {
        unified_lines,
        side_by_side_lines,
        unified_hunks,
        side_by_side_hunks,
    }
}

fn flush_pending(
    rows: &mut Vec<SideBySideLine>,
    removed: &mut Vec<(usize, String)>,
    added: &mut Vec<(usize, String)>,
) {
    let pairs = removed.len().max(added.len());
    for index in 0..pairs {
        let left = removed.get(index);
        let right = added.get(index);
        let kind = match (left, right) {
            (Some(_), Some(_)) => LineKind::Modified,
            (Some(_), None) => LineKind::Removed,
            (None, Some(_)) => LineKind::Added,
            (None, None) => LineKind::Unchanged,
        };

        rows.push(SideBySideLine {
            left_number: left.map(|(number, _)| *number),
            left_text: left.map_or_else(String::new, |(_, text)| text.clone()),
            right_number: right.map(|(number, _)| *number),
            right_text: right.map_or_else(String::new, |(_, text)| text.clone()),
            kind,
        });
    }

    removed.clear();
    added.clear();
}

fn line_count(text: &str) -> usize {
    let newlines = text.chars().filter(|character| *character == '\n').count();
    if newlines == 0 {
        1
    } else {
        newlines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sem_core::model::change::{ChangeType, SemanticChange};

    fn change(before: Option<&str>, after: Option<&str>) -> SemanticChange {
        SemanticChange {
            id: "change::x".to_string(),
            entity_id: "x::entity".to_string(),
            change_type: ChangeType::Modified,
            entity_type: "function".to_string(),
            entity_name: "demo".to_string(),
            file_path: "src/demo.rs".to_string(),
            old_file_path: None,
            before_content: before.map(str::to_string),
            after_content: after.map(str::to_string),
            commit_sha: None,
            author: None,
            timestamp: None,
            structural_change: Some(true),
            before_start_line: Some(1),
            before_end_line: Some(20),
            after_start_line: Some(1),
            after_end_line: Some(20),
        }
    }

    #[test]
    fn render_change_detects_multiple_hunks() {
        let before = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\n";
        let after = "line1\nline2 changed\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11 changed\nline12\n";
        let rendered = render_change(&change(Some(before), Some(after)));
        assert!(rendered.unified_hunks.len() >= 2);
        assert!(rendered.side_by_side_hunks.len() >= 2);
        let modified_lines: Vec<usize> = rendered
            .side_by_side_lines
            .iter()
            .filter(|line| line.kind == LineKind::Modified)
            .filter_map(|line| line.left_number)
            .collect();
        assert!(modified_lines.contains(&2));
        assert!(modified_lines.contains(&11));
    }

    #[test]
    fn render_change_handles_added_content() {
        let rendered = render_change(&change(None, Some("new line\n")));
        assert!(rendered
            .unified_lines
            .iter()
            .any(|(kind, _)| *kind == LineKind::Added));
        assert!(rendered
            .side_by_side_lines
            .iter()
            .any(|line| line.kind == LineKind::Added));
    }

    #[test]
    fn render_change_handles_deleted_content() {
        let rendered = render_change(&change(Some("old line\n"), None));
        assert!(rendered
            .unified_lines
            .iter()
            .any(|(kind, _)| *kind == LineKind::Removed));
        assert!(rendered
            .side_by_side_lines
            .iter()
            .any(|line| line.kind == LineKind::Removed));
    }
}
