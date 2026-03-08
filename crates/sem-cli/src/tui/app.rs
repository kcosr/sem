use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sem_core::model::change::SemanticChange;
use sem_core::parser::differ::DiffResult;
use similar::{ChangeTag, TextDiff};

use crate::commands::diff::{
    CommitCursor, CommitLoadStatus, CommitSnapshot, CommitStepAction, CommitStepResponse, DiffView,
    TuiSourceMode,
};

use super::detail::{render_change, LineKind, RenderedDiff, SideBySideLine};

const MIN_SIDE_BY_SIDE_WIDTH: u16 = 120;

#[derive(Clone, Debug)]
pub struct EntityRow {
    pub file_path: String,
    pub entity_type: String,
    pub entity_name: String,
    pub added_lines: usize,
    pub removed_lines: usize,
    pub range_label: Option<String>,
    pub change: SemanticChange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    List,
    Detail,
}

#[derive(Debug)]
pub struct AppState {
    rows: Vec<EntityRow>,
    list_header_command: String,
    selected: usize,
    mode: Mode,
    requested_view: DiffView,
    detail_scroll: usize,
    detail_hunk_index: usize,
    detail: Option<RenderedDiff>,
    show_help: bool,
    should_quit: bool,
    viewport_width: u16,
    viewport_height: u16,
    commit_source_mode: TuiSourceMode,
    commit_cursor: Option<CommitCursor>,
    commit_loading: bool,
    commit_status_message: Option<String>,
    pending_commit_action: Option<CommitStepAction>,
}

impl AppState {
    pub fn from_diff_result(result: &DiffResult, initial_view: DiffView) -> Self {
        let rows = Self::rows_from_diff_result(result);

        Self {
            rows,
            list_header_command: "sem diff --tui".to_string(),
            selected: 0,
            mode: Mode::List,
            requested_view: initial_view,
            detail_scroll: 0,
            detail_hunk_index: 0,
            detail: None,
            show_help: false,
            should_quit: false,
            viewport_width: 120,
            viewport_height: 40,
            commit_source_mode: TuiSourceMode::Unsupported,
            commit_cursor: None,
            commit_loading: false,
            commit_status_message: None,
            pending_commit_action: None,
        }
    }

    fn rows_from_diff_result(result: &DiffResult) -> Vec<EntityRow> {
        let mut changes = result.changes.clone();
        // Stable sort groups by file while preserving semantic order within each file.
        changes.sort_by(|a, b| a.file_path.cmp(&b.file_path));

        changes
            .into_iter()
            .map(|change| {
                let (added_lines, removed_lines) = change_line_counts(&change);
                EntityRow {
                    file_path: change.file_path.clone(),
                    entity_type: change.entity_type.clone(),
                    entity_name: change.entity_name.clone(),
                    added_lines,
                    removed_lines,
                    range_label: range_label(&change),
                    change,
                }
            })
            .collect()
    }

    pub fn set_viewport(&mut self, width: u16, height: u16) {
        self.viewport_width = width;
        self.viewport_height = height;
    }

    pub fn rows(&self) -> &[EntityRow] {
        &self.rows
    }

    pub fn set_list_header_command(&mut self, command: String) {
        self.list_header_command = command;
    }

    pub fn list_header_command(&self) -> &str {
        &self.list_header_command
    }

    pub fn configure_commit_navigation(
        &mut self,
        source_mode: TuiSourceMode,
        cursor: Option<CommitCursor>,
    ) {
        self.commit_source_mode = source_mode;
        self.commit_cursor = cursor;
    }

    pub fn commit_source_mode(&self) -> TuiSourceMode {
        self.commit_source_mode
    }

    pub fn commit_navigation_enabled(&self) -> bool {
        self.commit_source_mode == TuiSourceMode::Commit
    }

    pub fn commit_cursor(&self) -> Option<&CommitCursor> {
        self.commit_cursor.as_ref()
    }

    pub fn commit_loading(&self) -> bool {
        self.commit_loading
    }

    pub fn commit_status_message(&self) -> Option<&str> {
        self.commit_status_message.as_deref()
    }

    pub fn set_commit_loading(&mut self, loading: bool) {
        self.commit_loading = loading;
    }

    pub fn commit_context_line(&self) -> String {
        if !self.commit_navigation_enabled() {
            return "Commit navigation unavailable for current input mode".to_string();
        }

        let Some(cursor) = self.commit_cursor() else {
            return "Commit metadata unavailable".to_string();
        };

        let short_sha: String = cursor.sha.chars().take(7).collect();
        match &cursor.rev_label {
            Some(rev_label) => format!("{rev_label}  {short_sha}  {}", cursor.subject),
            None => format!("{short_sha}  {}", cursor.subject),
        }
    }

    pub fn queue_commit_action(&mut self, action: CommitStepAction) {
        self.pending_commit_action = Some(action);
        self.commit_status_message = None;
    }

    pub fn take_pending_commit_action(&mut self) -> Option<CommitStepAction> {
        self.pending_commit_action.take()
    }

    pub fn apply_commit_step_response(&mut self, response: CommitStepResponse) {
        match response.status {
            CommitLoadStatus::Loaded => {
                if let Some(snapshot) = response.snapshot {
                    self.apply_commit_snapshot(snapshot);
                    self.commit_status_message = Some("Commit snapshot loaded".to_string());
                }
                self.commit_loading = false;
            }
            CommitLoadStatus::LoadFailed => {
                let mut message = response
                    .error
                    .unwrap_or_else(|| "commit reload failed".to_string());
                if response.retain_previous_snapshot {
                    message.push_str(" (previous snapshot retained)");
                }
                self.commit_status_message = Some(message);
                self.commit_loading = false;
            }
            CommitLoadStatus::UnsupportedMode => {
                self.commit_status_message =
                    Some("Commit navigation unavailable for current input mode".to_string());
                self.commit_loading = false;
            }
            CommitLoadStatus::BoundaryNoop => {
                self.commit_status_message = Some("Commit boundary reached".to_string());
                self.commit_loading = false;
            }
            CommitLoadStatus::IgnoredStaleResult => {
                self.commit_status_message = Some("Ignored stale commit reload result".to_string());
            }
        }
    }

    fn apply_commit_snapshot(&mut self, snapshot: CommitSnapshot) {
        self.commit_cursor = Some(snapshot.cursor);
        self.rows = Self::rows_from_diff_result(&snapshot.result);
        self.selected = 0;
        self.detail_scroll = 0;
        self.detail_hunk_index = 0;
        if self.mode == Mode::Detail {
            self.refresh_detail();
        } else {
            self.detail = None;
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn effective_view(&self) -> DiffView {
        if self.requested_view == DiffView::SideBySide
            && self.viewport_width < MIN_SIDE_BY_SIDE_WIDTH
        {
            DiffView::Unified
        } else {
            self.requested_view
        }
    }

    pub fn fallback_active(&self) -> bool {
        self.requested_view == DiffView::SideBySide && self.effective_view() == DiffView::Unified
    }

    pub fn show_help(&self) -> bool {
        self.show_help
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn detail_scroll(&self) -> usize {
        self.detail_scroll
    }

    pub fn detail_title(&self) -> String {
        let Some(row) = self.rows.get(self.selected) else {
            return "Detail".to_string();
        };

        match &row.range_label {
            Some(range) => format!("{} {} {}", row.file_path, row.entity_name, range),
            None => format!("{} {}", row.file_path, row.entity_name),
        }
    }

    pub fn unified_lines(&self) -> &[(LineKind, String)] {
        if let Some(detail) = &self.detail {
            &detail.unified_lines
        } else {
            &[]
        }
    }

    pub fn side_by_side_lines(&self) -> &[SideBySideLine] {
        if let Some(detail) = &self.detail {
            &detail.side_by_side_lines
        } else {
            &[]
        }
    }

    #[cfg(test)]
    pub fn detail_hunk_index(&self) -> usize {
        self.detail_hunk_index
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }

        if self.show_help {
            match key.code {
                KeyCode::Char('?') | KeyCode::Esc => self.show_help = false,
                KeyCode::Char('q') => self.should_quit = true,
                _ => {}
            }
            return;
        }

        match self.mode {
            Mode::List => self.handle_list_key(key),
            Mode::Detail => self.handle_detail_key(key),
        }
    }

    fn handle_list_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('[') => self.queue_commit_action(CommitStepAction::Older),
            KeyCode::Char(']') => self.queue_commit_action(CommitStepAction::Newer),
            KeyCode::Up | KeyCode::Char('k') => self.move_up(),
            KeyCode::Down | KeyCode::Char('j') => self.move_down(),
            KeyCode::Char('g') => self.selected = 0,
            KeyCode::Char('G') => {
                if !self.rows.is_empty() {
                    self.selected = self.rows.len() - 1;
                }
            }
            KeyCode::Enter => self.open_detail(),
            _ => {}
        }
    }

    fn handle_detail_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('[') => self.queue_commit_action(CommitStepAction::Older),
            KeyCode::Char(']') => self.queue_commit_action(CommitStepAction::Newer),
            KeyCode::Esc => self.close_detail(),
            KeyCode::Left => self.previous_entity(),
            KeyCode::Right => self.next_entity(),
            KeyCode::Tab => self.toggle_view(),
            KeyCode::Char('n') => self.next_hunk(),
            KeyCode::Char('p') => self.previous_hunk(),
            KeyCode::PageDown => self.scroll_page_down(),
            KeyCode::PageUp => self.scroll_page_up(),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_line_down(),
            KeyCode::Up | KeyCode::Char('k') => self.scroll_line_up(),
            KeyCode::Char('g') => self.detail_scroll = 0,
            KeyCode::Char('G') => {
                self.detail_scroll = self.max_scroll();
            }
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

    fn open_detail(&mut self) {
        self.mode = Mode::Detail;
        self.refresh_detail();
    }

    fn next_entity(&mut self) {
        if self.rows.is_empty() {
            return;
        }

        self.selected = (self.selected + 1) % self.rows.len();
        self.refresh_detail();
    }

    fn previous_entity(&mut self) {
        if self.rows.is_empty() {
            return;
        }

        if self.selected == 0 {
            self.selected = self.rows.len() - 1;
        } else {
            self.selected -= 1;
        }
        self.refresh_detail();
    }

    fn refresh_detail(&mut self) {
        if let Some(row) = self.rows.get(self.selected) {
            self.detail = Some(render_change(&row.change));
        } else {
            self.detail = None;
        }

        self.detail_scroll = 0;
        self.detail_hunk_index = 0;
        self.jump_to_hunk();
    }

    fn close_detail(&mut self) {
        self.mode = Mode::List;
        self.detail_scroll = 0;
        self.detail_hunk_index = 0;
        self.detail = None;
    }

    fn toggle_view(&mut self) {
        self.requested_view = match self.requested_view {
            DiffView::Unified => DiffView::SideBySide,
            DiffView::SideBySide => DiffView::Unified,
        };

        self.detail_hunk_index = 0;
        self.jump_to_hunk();
    }

    fn next_hunk(&mut self) {
        let hunk_count = self.hunk_positions().len();
        if hunk_count == 0 {
            return;
        }

        if self.detail_hunk_index + 1 < hunk_count {
            self.detail_hunk_index += 1;
            self.jump_to_hunk();
        }
    }

    fn previous_hunk(&mut self) {
        if self.hunk_positions().is_empty() {
            return;
        }

        if self.detail_hunk_index > 0 {
            self.detail_hunk_index -= 1;
            self.jump_to_hunk();
        }
    }

    fn jump_to_hunk(&mut self) {
        let Some(line) = self.hunk_positions().get(self.detail_hunk_index).copied() else {
            self.detail_scroll = 0;
            return;
        };

        self.detail_scroll = line;
    }

    fn scroll_page_down(&mut self) {
        let page_size = self.page_size();
        self.detail_scroll = (self.detail_scroll + page_size).min(self.max_scroll());
    }

    fn scroll_page_up(&mut self) {
        let page_size = self.page_size();
        self.detail_scroll = self.detail_scroll.saturating_sub(page_size);
    }

    fn scroll_line_down(&mut self) {
        self.detail_scroll = (self.detail_scroll + 1).min(self.max_scroll());
    }

    fn scroll_line_up(&mut self) {
        self.detail_scroll = self.detail_scroll.saturating_sub(1);
    }

    fn page_size(&self) -> usize {
        self.viewport_height.saturating_sub(8) as usize
    }

    fn max_scroll(&self) -> usize {
        self.detail_line_count().saturating_sub(1)
    }

    fn detail_line_count(&self) -> usize {
        match self.effective_view() {
            DiffView::Unified => self.unified_lines().len(),
            DiffView::SideBySide => self.side_by_side_lines().len(),
        }
    }

    fn hunk_positions(&self) -> &[usize] {
        let Some(detail) = &self.detail else {
            return &[];
        };

        match self.effective_view() {
            DiffView::Unified => &detail.unified_hunks,
            DiffView::SideBySide => &detail.side_by_side_hunks,
        }
    }
}

fn range_label(change: &SemanticChange) -> Option<String> {
    match (
        change.before_start_line,
        change.before_end_line,
        change.after_start_line,
        change.after_end_line,
    ) {
        (Some(before_start), Some(before_end), Some(after_start), Some(after_end)) => Some(
            format!("[L{before_start}-L{before_end} -> L{after_start}-L{after_end}]"),
        ),
        (Some(before_start), Some(before_end), None, None) => {
            Some(format!("[L{before_start}-L{before_end}]"))
        }
        (None, None, Some(after_start), Some(after_end)) => {
            Some(format!("[L{after_start}-L{after_end}]"))
        }
        _ => None,
    }
}

fn change_line_counts(change: &SemanticChange) -> (usize, usize) {
    let before = change.before_content.as_deref().unwrap_or("");
    let after = change.after_content.as_deref().unwrap_or("");
    if before.is_empty() && after.is_empty() {
        return (0, 0);
    }

    let diff = TextDiff::from_lines(before, after);
    let mut added: usize = 0;
    let mut removed: usize = 0;

    for op in diff.ops() {
        for diff_change in diff.iter_changes(op) {
            match diff_change.tag() {
                ChangeTag::Insert => {
                    added = added.saturating_add(changed_line_count(diff_change.value()));
                }
                ChangeTag::Delete => {
                    removed = removed.saturating_add(changed_line_count(diff_change.value()));
                }
                ChangeTag::Equal => {}
            }
        }
    }

    (added, removed)
}

fn changed_line_count(text: &str) -> usize {
    let newline_count = text.chars().filter(|character| *character == '\n').count();
    newline_count.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use sem_core::model::change::{ChangeType, SemanticChange};

    fn change(file: &str, name: &str, before: &str, after: &str) -> SemanticChange {
        SemanticChange {
            id: format!("change::{name}"),
            entity_id: format!("{file}::{name}"),
            change_type: ChangeType::Modified,
            entity_type: "function".to_string(),
            entity_name: name.to_string(),
            file_path: file.to_string(),
            old_file_path: None,
            before_content: Some(before.to_string()),
            after_content: Some(after.to_string()),
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

    fn app() -> AppState {
        let before = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\n";
        let after = "line1\nline2 changed\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11 changed\nline12\n";
        let result = DiffResult {
            changes: vec![
                change("b.ts", "beta", before, after),
                change("a.ts", "alpha", before, after),
            ],
            file_count: 2,
            added_count: 0,
            modified_count: 2,
            deleted_count: 0,
            moved_count: 0,
            renamed_count: 0,
        };

        AppState::from_diff_result(&result, DiffView::Unified)
    }

    #[test]
    fn app_state_sorts_rows_by_file_path() {
        let app = app();
        assert_eq!(app.rows()[0].file_path, "a.ts");
        assert_eq!(app.rows()[1].file_path, "b.ts");
    }

    #[test]
    fn app_state_moves_selection_with_j_k() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 1);
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 0);
        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 1);
    }

    #[test]
    fn rows_include_added_and_removed_counts() {
        let app = app();
        assert_eq!(app.rows()[0].added_lines, 2);
        assert_eq!(app.rows()[0].removed_lines, 2);
    }

    #[test]
    fn app_state_quits_with_q() {
        let mut app = app();
        assert!(!app.should_quit());
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.should_quit());
    }

    #[test]
    fn app_state_quits_with_ctrl_c() {
        let mut app = app();
        assert!(!app.should_quit());
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit());
    }

    #[test]
    fn enter_opens_detail_and_escape_closes_it() {
        let mut app = app();
        assert_eq!(app.mode(), Mode::List);
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Detail);
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::List);
    }

    #[test]
    fn side_by_side_falls_back_on_narrow_width() {
        let mut app = app();
        app.requested_view = DiffView::SideBySide;
        app.set_viewport(90, 30);
        assert_eq!(app.effective_view(), DiffView::Unified);
        assert!(app.fallback_active());

        app.set_viewport(160, 30);
        assert_eq!(app.effective_view(), DiffView::SideBySide);
        assert!(!app.fallback_active());
    }

    #[test]
    fn hunk_navigation_stays_within_bounds() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
        assert_eq!(app.detail_hunk_index(), 0);

        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        let after_next = app.detail_hunk_index();
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(app.detail_hunk_index() >= after_next);
    }

    #[test]
    fn help_overlay_toggles_with_question_mark_and_escape() {
        let mut app = app();
        assert!(!app.show_help());
        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        assert!(app.show_help());
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.show_help());
    }

    #[test]
    fn tab_toggles_requested_view_in_detail_mode() {
        let mut app = app();
        app.set_viewport(200, 40);
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.effective_view(), DiffView::Unified);
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.effective_view(), DiffView::SideBySide);
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.effective_view(), DiffView::Unified);
    }

    #[test]
    fn list_mode_g_and_g_keys_jump_to_bounds() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
        assert_eq!(app.selected(), app.rows().len() - 1);
        app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 0);
    }

    #[test]
    fn detail_mode_page_scroll_stays_in_bounds() {
        let mut app = app();
        app.set_viewport(120, 12);
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let start = app.detail_scroll();
        app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        assert!(app.detail_scroll() >= start);
        app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
        assert_eq!(app.detail_scroll(), 0);
    }

    #[test]
    fn detail_mode_left_and_right_cycle_entities() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.detail_title().contains("alpha"));

        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(app.detail_title().contains("beta"));

        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(app.detail_title().contains("alpha"));

        app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert!(app.detail_title().contains("beta"));
    }

    #[test]
    fn bracket_keys_queue_commit_actions_in_list_and_detail_modes() {
        let mut app = app();

        app.handle_key(KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE));
        assert_eq!(
            app.take_pending_commit_action(),
            Some(CommitStepAction::Older)
        );

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE));
        assert_eq!(
            app.take_pending_commit_action(),
            Some(CommitStepAction::Newer)
        );
    }

    #[test]
    fn apply_loaded_commit_snapshot_resets_selection_and_cursor_state() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 1);

        let snapshot = CommitSnapshot {
            cursor: CommitCursor {
                rev_label: Some("HEAD~2".to_string()),
                sha: "abc1234".to_string(),
                subject: "test subject".to_string(),
                has_older: true,
                has_newer: true,
            },
            result: DiffResult {
                changes: vec![change("c.ts", "gamma", "x\n", "y\n")],
                file_count: 1,
                added_count: 0,
                modified_count: 1,
                deleted_count: 0,
                moved_count: 0,
                renamed_count: 0,
            },
        };

        app.apply_commit_step_response(CommitStepResponse {
            applied_request_id: 1,
            status: CommitLoadStatus::Loaded,
            snapshot: Some(snapshot),
            error: None,
            retain_previous_snapshot: false,
        });

        assert_eq!(app.selected(), 0);
        assert_eq!(app.rows().len(), 1);
        assert_eq!(
            app.commit_cursor().map(|cursor| cursor.sha.as_str()),
            Some("abc1234")
        );
        assert_eq!(app.commit_status_message(), Some("Commit snapshot loaded"));
    }

    #[test]
    fn commit_context_line_formats_for_supported_and_unsupported_modes() {
        let mut app = app();
        app.configure_commit_navigation(TuiSourceMode::Unsupported, None);
        assert_eq!(
            app.commit_context_line(),
            "Commit navigation unavailable for current input mode"
        );

        app.configure_commit_navigation(
            TuiSourceMode::Commit,
            Some(CommitCursor {
                rev_label: Some("HEAD~3".to_string()),
                sha: "0123456789abcdef".to_string(),
                subject: "feat: add stepping".to_string(),
                has_older: true,
                has_newer: true,
            }),
        );
        assert_eq!(
            app.commit_context_line(),
            "HEAD~3  0123456  feat: add stepping"
        );

        app.configure_commit_navigation(
            TuiSourceMode::Commit,
            Some(CommitCursor {
                rev_label: None,
                sha: "abcdef0123456789".to_string(),
                subject: "chore: cleanup".to_string(),
                has_older: true,
                has_newer: false,
            }),
        );
        assert_eq!(app.commit_context_line(), "abcdef0  chore: cleanup");
    }

    #[test]
    fn apply_empty_snapshot_in_detail_mode_keeps_ui_stable() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Detail);

        app.apply_commit_step_response(CommitStepResponse {
            applied_request_id: 9,
            status: CommitLoadStatus::Loaded,
            snapshot: Some(CommitSnapshot {
                cursor: CommitCursor {
                    rev_label: Some("HEAD~0".to_string()),
                    sha: "abc1234".to_string(),
                    subject: "empty semantic diff".to_string(),
                    has_older: true,
                    has_newer: false,
                },
                result: DiffResult {
                    changes: vec![],
                    file_count: 0,
                    added_count: 0,
                    modified_count: 0,
                    deleted_count: 0,
                    moved_count: 0,
                    renamed_count: 0,
                },
            }),
            error: None,
            retain_previous_snapshot: false,
        });

        assert_eq!(app.mode(), Mode::Detail);
        assert_eq!(app.rows().len(), 0);
        assert_eq!(app.selected(), 0);
        assert_eq!(app.detail_scroll(), 0);
    }

    #[test]
    fn unsupported_mode_response_sets_status_hint() {
        let mut app = app();
        app.configure_commit_navigation(TuiSourceMode::Unsupported, None);
        app.set_commit_loading(true);
        assert!(app.commit_loading());
        app.queue_commit_action(CommitStepAction::Older);
        assert_eq!(
            app.take_pending_commit_action(),
            Some(CommitStepAction::Older)
        );

        app.apply_commit_step_response(CommitStepResponse {
            applied_request_id: 2,
            status: CommitLoadStatus::UnsupportedMode,
            snapshot: None,
            error: None,
            retain_previous_snapshot: true,
        });
        assert_eq!(
            app.commit_status_message(),
            Some("Commit navigation unavailable for current input mode")
        );
        assert!(!app.commit_loading());
    }

    #[test]
    fn app_quits_immediately_even_while_commit_reload_is_marked_loading() {
        let mut app = app();
        app.set_commit_loading(true);
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.should_quit());
    }

    #[test]
    fn load_failed_response_keeps_existing_rows_and_reports_retained_snapshot() {
        let mut app = app();
        let baseline_rows = app.rows().len();

        app.apply_commit_step_response(CommitStepResponse {
            applied_request_id: 11,
            status: CommitLoadStatus::LoadFailed,
            snapshot: None,
            error: Some("unable to resolve commit".to_string()),
            retain_previous_snapshot: true,
        });

        assert_eq!(app.rows().len(), baseline_rows);
        assert_eq!(
            app.commit_status_message(),
            Some("unable to resolve commit (previous snapshot retained)")
        );
        assert!(!app.commit_loading());
    }
}
