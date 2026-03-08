use crossterm::event::{KeyCode, KeyEvent};
use sem_core::model::change::SemanticChange;
use sem_core::parser::differ::DiffResult;

use crate::commands::diff::DiffView;

#[derive(Clone, Debug)]
pub struct EntityRow {
    pub file_path: String,
    pub entity_type: String,
    pub entity_name: String,
    pub change_type: String,
    pub range_label: Option<String>,
}

#[derive(Debug)]
pub struct AppState {
    rows: Vec<EntityRow>,
    selected: usize,
    should_quit: bool,
    initial_view: DiffView,
}

impl AppState {
    pub fn from_diff_result(result: &DiffResult, initial_view: DiffView) -> Self {
        let mut changes = result.changes.clone();
        // Stable sort groups by file while preserving semantic order within each file.
        changes.sort_by(|a, b| a.file_path.cmp(&b.file_path));

        let rows = changes
            .iter()
            .map(|change| EntityRow {
                file_path: change.file_path.clone(),
                entity_type: change.entity_type.clone(),
                entity_name: change.entity_name.clone(),
                change_type: change.change_type.to_string(),
                range_label: range_label(change),
            })
            .collect();

        Self {
            rows,
            selected: 0,
            should_quit: false,
            initial_view,
        }
    }

    pub fn rows(&self) -> &[EntityRow] {
        &self.rows
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn initial_view(&self) -> DiffView {
        self.initial_view
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Up | KeyCode::Char('k') => self.move_up(),
            KeyCode::Down | KeyCode::Char('j') => self.move_down(),
            _ => {}
        }
    }

    fn move_up(&mut self) {
        if self.rows.is_empty() {
            return;
        }

        if self.selected == 0 {
            self.selected = self.rows.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    fn move_down(&mut self) {
        if self.rows.is_empty() {
            return;
        }

        self.selected = (self.selected + 1) % self.rows.len();
    }
}

fn range_label(change: &SemanticChange) -> Option<String> {
    match (
        change.before_start_line,
        change.before_end_line,
        change.after_start_line,
        change.after_end_line,
    ) {
        (Some(before_start), Some(before_end), Some(after_start), Some(after_end)) => {
            Some(format!("[L{before_start}-L{before_end} -> L{after_start}-L{after_end}]") )
        }
        (Some(before_start), Some(before_end), None, None) => {
            Some(format!("[L{before_start}-L{before_end}]") )
        }
        (None, None, Some(after_start), Some(after_end)) => {
            Some(format!("[L{after_start}-L{after_end}]") )
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use sem_core::model::change::{ChangeType, SemanticChange};
    use sem_core::parser::differ::DiffResult;

    fn change(file: &str, name: &str) -> SemanticChange {
        SemanticChange {
            id: format!("change::{name}"),
            entity_id: format!("{file}::{name}"),
            change_type: ChangeType::Modified,
            entity_type: "function".to_string(),
            entity_name: name.to_string(),
            file_path: file.to_string(),
            old_file_path: None,
            before_content: Some("before".to_string()),
            after_content: Some("after".to_string()),
            commit_sha: None,
            author: None,
            timestamp: None,
            structural_change: Some(true),
            before_start_line: Some(1),
            before_end_line: Some(2),
            after_start_line: Some(1),
            after_end_line: Some(3),
        }
    }

    fn result() -> DiffResult {
        DiffResult {
            changes: vec![change("b.ts", "beta"), change("a.ts", "alpha")],
            file_count: 2,
            added_count: 0,
            modified_count: 2,
            deleted_count: 0,
            moved_count: 0,
            renamed_count: 0,
        }
    }

    #[test]
    fn app_state_sorts_rows_by_file_path() {
        let app = AppState::from_diff_result(&result(), DiffView::Unified);
        assert_eq!(app.rows()[0].file_path, "a.ts");
        assert_eq!(app.rows()[1].file_path, "b.ts");
    }

    #[test]
    fn app_state_moves_selection_with_j_k() {
        let mut app = AppState::from_diff_result(&result(), DiffView::Unified);

        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 1);

        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 0);

        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 1);
    }

    #[test]
    fn app_state_quits_with_q() {
        let mut app = AppState::from_diff_result(&result(), DiffView::Unified);
        assert!(!app.should_quit());
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.should_quit());
    }
}
