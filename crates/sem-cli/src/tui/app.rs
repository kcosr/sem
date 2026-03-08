use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sem_core::model::change::SemanticChange;
use sem_core::parser::differ::DiffResult;
use similar::{ChangeTag, TextDiff};
use std::collections::HashMap;

use super::review_state::{
    build_logical_entity_key, build_target_content_hash, current_updated_at,
    endpoint_supports_review_hash, ReviewFilter, ReviewIdentity, ReviewStateData,
};
use crate::commands::diff::{
    CommitCursor, CommitLoadStatus, CommitSnapshot, CommitStepAction, CommitStepResponse, DiffView,
    StepComparison, StepEndpoint, StepMode, TuiSourceMode,
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
    navigation_endpoints: Vec<StepEndpoint>,
    navigation_endpoint_index: HashMap<String, usize>,
    commit_cursor: Option<CommitCursor>,
    step_mode: StepMode,
    cumulative_base_endpoint_id: Option<String>,
    comparison: Option<StepComparison>,
    commit_loading: bool,
    commit_status_message: Option<String>,
    pending_navigation_request: Option<PendingNavigationRequest>,
    review_filter: ReviewFilter,
    reviewed_records: HashMap<ReviewIdentity, String>,
    row_review_identities: Vec<Option<ReviewIdentity>>,
    review_status_message: Option<String>,
    review_state_dirty: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingNavigationRequest {
    Step(CommitStepAction),
    Refresh,
}

impl AppState {
    pub fn from_diff_result(result: &DiffResult, initial_view: DiffView) -> Self {
        let rows = Self::rows_from_diff_result(result);
        let row_count = rows.len();

        Self {
            rows,
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
            navigation_endpoints: vec![],
            navigation_endpoint_index: HashMap::new(),
            commit_cursor: None,
            step_mode: StepMode::Pairwise,
            cumulative_base_endpoint_id: None,
            comparison: None,
            commit_loading: false,
            commit_status_message: None,
            pending_navigation_request: None,
            review_filter: ReviewFilter::All,
            reviewed_records: HashMap::new(),
            row_review_identities: vec![None; row_count],
            review_status_message: None,
            review_state_dirty: false,
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

    pub fn configure_commit_navigation(
        &mut self,
        source_mode: TuiSourceMode,
        endpoints: Vec<StepEndpoint>,
        endpoint_index: HashMap<String, usize>,
        cursor: Option<CommitCursor>,
        mode: StepMode,
        base_endpoint_id: Option<String>,
    ) {
        self.commit_source_mode = source_mode;
        self.navigation_endpoints = endpoints;
        self.navigation_endpoint_index = endpoint_index;
        self.commit_cursor = cursor;
        self.step_mode = mode;
        self.cumulative_base_endpoint_id = match mode {
            StepMode::Pairwise => None,
            StepMode::Cumulative => {
                base_endpoint_id.or_else(|| self.default_cumulative_base_endpoint_id())
            }
        };
        self.recompute_comparison();
        self.recompute_review_identities();
    }

    pub fn commit_source_mode(&self) -> TuiSourceMode {
        self.commit_source_mode
    }

    pub fn commit_navigation_enabled(&self) -> bool {
        self.commit_source_mode == TuiSourceMode::Commit
            || self.commit_source_mode == TuiSourceMode::Unified
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

    pub fn status_message(&self) -> Option<&str> {
        self.commit_status_message
            .as_deref()
            .or(self.review_status_message.as_deref())
    }

    pub fn review_filter(&self) -> ReviewFilter {
        self.review_filter
    }

    pub fn is_row_reviewed(&self, row_index: usize) -> bool {
        self.row_review_identities
            .get(row_index)
            .and_then(|identity| identity.as_ref())
            .map(|identity| self.reviewed_records.contains_key(identity))
            .unwrap_or(false)
    }

    pub fn apply_review_state(&mut self, state: ReviewStateData) {
        self.review_filter = state.filter;
        self.reviewed_records = state.records;
        self.review_state_dirty = false;
    }

    pub fn set_review_status_message(&mut self, message: Option<String>) {
        self.review_status_message = message;
    }

    pub fn mark_review_state_dirty(&mut self) {
        self.review_state_dirty = true;
    }

    pub fn take_review_state_dirty_snapshot(&mut self) -> Option<ReviewStateData> {
        if !self.review_state_dirty {
            return None;
        }

        self.review_state_dirty = false;
        Some(self.review_state_snapshot())
    }

    pub fn review_state_snapshot(&self) -> ReviewStateData {
        ReviewStateData {
            filter: self.review_filter,
            records: self.reviewed_records.clone(),
        }
    }

    pub fn toggle_selected_reviewed(&mut self) -> bool {
        let Some(identity) = self
            .row_review_identities
            .get(self.selected)
            .and_then(|identity| identity.as_ref())
            .cloned()
        else {
            self.review_status_message =
                Some("Review state unavailable for current comparator endpoint".to_string());
            return false;
        };

        if self.reviewed_records.contains_key(&identity) {
            self.reviewed_records.remove(&identity);
        } else {
            self.reviewed_records.insert(identity, current_updated_at());
        }

        self.review_state_dirty = true;
        self.review_status_message = None;
        true
    }

    pub fn cycle_review_filter(&mut self) {
        self.review_filter = self.review_filter.cycle();
        self.review_state_dirty = true;
    }

    pub fn set_commit_loading(&mut self, loading: bool) {
        self.commit_loading = loading;
    }

    pub fn comparison_line(&self) -> Option<(String, String, String, String)> {
        if !self.commit_navigation_enabled() {
            return None;
        }

        let comparison = self.comparison.as_ref()?;
        let from = self.endpoint_display_label(&comparison.from_endpoint_id)?;
        let to = self.endpoint_display_label(&comparison.to_endpoint_id)?;
        let (left_label, right_label) = match self.step_mode {
            StepMode::Pairwise => ("previous".to_string(), "current".to_string()),
            StepMode::Cumulative => ("base".to_string(), "cursor".to_string()),
        };
        Some((left_label, from, right_label, to))
    }

    pub fn queue_commit_action(&mut self, action: CommitStepAction) {
        if !self.commit_navigation_enabled() {
            return;
        }
        self.pending_navigation_request = Some(PendingNavigationRequest::Step(action));
        self.commit_status_message = None;
    }

    pub fn toggle_step_mode(&mut self) {
        if !self.commit_navigation_enabled() {
            return;
        }
        self.step_mode = match self.step_mode {
            StepMode::Pairwise => StepMode::Cumulative,
            StepMode::Cumulative => StepMode::Pairwise,
        };
        self.cumulative_base_endpoint_id = if self.step_mode == StepMode::Cumulative {
            self.default_cumulative_base_endpoint_id()
        } else {
            None
        };
        self.recompute_comparison();
        self.pending_navigation_request = Some(PendingNavigationRequest::Refresh);
    }

    pub fn take_pending_navigation_request(&mut self) -> Option<PendingNavigationRequest> {
        self.pending_navigation_request.take()
    }

    pub fn step_mode(&self) -> StepMode {
        self.step_mode
    }

    pub fn cumulative_base_endpoint_id(&self) -> Option<String> {
        self.cumulative_base_endpoint_id.clone()
    }

    pub fn apply_commit_step_response(&mut self, response: CommitStepResponse) {
        match response.status {
            CommitLoadStatus::Loaded => {
                if let Some(snapshot) = response.snapshot {
                    self.apply_commit_snapshot(snapshot);
                }
                self.commit_status_message = None;
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
                self.commit_status_message = Some("Step boundary reached".to_string());
                self.commit_loading = false;
            }
            CommitLoadStatus::IgnoredStaleResult => {
                self.commit_status_message = Some("Ignored stale reload result".to_string());
            }
        }
    }

    fn apply_commit_snapshot(&mut self, snapshot: CommitSnapshot) {
        self.commit_cursor = Some(snapshot.cursor);
        self.step_mode = snapshot.mode;
        self.cumulative_base_endpoint_id = snapshot.base_endpoint_id;
        self.comparison = Some(snapshot.comparison);
        self.rows = Self::rows_from_diff_result(&snapshot.result);
        self.recompute_review_identities();
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
            KeyCode::Char('m') => self.toggle_step_mode(),
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
            KeyCode::Char('m') => self.toggle_step_mode(),
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

    fn default_cumulative_base_endpoint_id(&self) -> Option<String> {
        self.navigation_endpoints
            .first()
            .map(|endpoint| endpoint.endpoint_id.clone())
            .or_else(|| {
                self.commit_cursor
                    .as_ref()
                    .map(|cursor| cursor.endpoint_id.clone())
            })
    }

    fn recompute_comparison(&mut self) {
        if !self.commit_navigation_enabled() {
            self.comparison = None;
            return;
        }
        let Some(cursor) = self.commit_cursor.as_ref() else {
            self.comparison = None;
            return;
        };
        let Some(&cursor_index) = self.navigation_endpoint_index.get(&cursor.endpoint_id) else {
            self.comparison = None;
            return;
        };
        let Some(to_endpoint) = self.navigation_endpoints.get(cursor_index) else {
            self.comparison = None;
            return;
        };
        let from_endpoint_id = match self.step_mode {
            StepMode::Pairwise => {
                if cursor_index == 0 {
                    to_endpoint.endpoint_id.clone()
                } else {
                    self.navigation_endpoints
                        .get(cursor_index - 1)
                        .map(|endpoint| endpoint.endpoint_id.clone())
                        .unwrap_or_else(|| to_endpoint.endpoint_id.clone())
                }
            }
            StepMode::Cumulative => self
                .cumulative_base_endpoint_id
                .clone()
                .unwrap_or_else(|| to_endpoint.endpoint_id.clone()),
        };
        self.comparison = Some(StepComparison {
            from_endpoint_id,
            to_endpoint_id: to_endpoint.endpoint_id.clone(),
        });
    }

    fn recompute_review_identities(&mut self) {
        self.row_review_identities = vec![None; self.rows.len()];

        let to_endpoint_id = self
            .comparison
            .as_ref()
            .map(|cmp| cmp.to_endpoint_id.as_str());
        if !endpoint_supports_review_hash(to_endpoint_id) {
            return;
        }

        let mut fallback_ordinals: HashMap<(String, String, String), usize> = HashMap::new();
        for (row_index, row) in self.rows.iter().enumerate() {
            let group_key = (
                row.file_path.clone(),
                row.entity_type.clone(),
                row.entity_name.clone(),
            );
            let ordinal = fallback_ordinals.entry(group_key).or_insert(0);
            *ordinal = ordinal.saturating_add(1);

            let logical_entity_key = build_logical_entity_key(&row.change, *ordinal);
            let Some(target_content_hash) = build_target_content_hash(&row.change) else {
                continue;
            };

            self.row_review_identities[row_index] = Some(ReviewIdentity {
                logical_entity_key,
                target_content_hash,
            });
        }
    }

    fn endpoint_display_label(&self, endpoint_id: &str) -> Option<String> {
        let &index = self.navigation_endpoint_index.get(endpoint_id)?;
        let endpoint = self.navigation_endpoints.get(index)?;
        if let Some(display_ref) = endpoint.display_ref.as_deref() {
            return Some(display_ref.to_string());
        }
        match &endpoint.kind {
            crate::commands::diff::StepEndpointKind::Commit { sha } => {
                Some(sha.chars().take(7).collect())
            }
            crate::commands::diff::StepEndpointKind::Index => Some("INDEX".to_string()),
            crate::commands::diff::StepEndpointKind::Working => Some("WORKING".to_string()),
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
    use std::collections::HashMap;

    use crate::commands::diff::StepEndpointKind;

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

    fn navigation_fixture() -> (Vec<StepEndpoint>, HashMap<String, usize>, CommitCursor) {
        let endpoints = vec![
            StepEndpoint {
                endpoint_id: "commit:aaaaaaa".to_string(),
                display_ref: Some("HEAD~1".to_string()),
                kind: StepEndpointKind::Commit {
                    sha: "aaaaaaa".to_string(),
                },
            },
            StepEndpoint {
                endpoint_id: "commit:bbbbbbb".to_string(),
                display_ref: Some("HEAD".to_string()),
                kind: StepEndpointKind::Commit {
                    sha: "bbbbbbb".to_string(),
                },
            },
        ];
        let endpoint_index = HashMap::from([
            ("commit:aaaaaaa".to_string(), 0usize),
            ("commit:bbbbbbb".to_string(), 1usize),
        ]);
        let cursor = CommitCursor {
            endpoint_id: "commit:bbbbbbb".to_string(),
            index: 1,
            rev_label: Some("HEAD".to_string()),
            sha: "bbbbbbb".to_string(),
            subject: "tip".to_string(),
            has_older: true,
            has_newer: false,
        };
        (endpoints, endpoint_index, cursor)
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
        let (endpoints, endpoint_index, cursor) = navigation_fixture();
        app.configure_commit_navigation(
            TuiSourceMode::Commit,
            endpoints,
            endpoint_index,
            Some(cursor),
            StepMode::Pairwise,
            None,
        );

        app.handle_key(KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE));
        assert_eq!(
            app.take_pending_navigation_request(),
            Some(PendingNavigationRequest::Step(CommitStepAction::Older))
        );

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE));
        assert_eq!(
            app.take_pending_navigation_request(),
            Some(PendingNavigationRequest::Step(CommitStepAction::Newer))
        );
    }

    #[test]
    fn apply_loaded_commit_snapshot_resets_selection_and_cursor_state() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 1);

        let snapshot = CommitSnapshot {
            cursor: CommitCursor {
                endpoint_id: "commit:abc1234".to_string(),
                index: 0,
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
            mode: StepMode::Pairwise,
            base_endpoint_id: None,
            comparison: StepComparison {
                from_endpoint_id: "commit:def5678".to_string(),
                to_endpoint_id: "commit:abc1234".to_string(),
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
        assert_eq!(app.commit_status_message(), None);
    }

    #[test]
    fn comparison_line_formats_for_pairwise_and_cumulative_modes() {
        let mut app = app();
        app.configure_commit_navigation(
            TuiSourceMode::Unsupported,
            vec![],
            HashMap::new(),
            None,
            StepMode::Pairwise,
            None,
        );
        assert_eq!(app.comparison_line(), None);

        let (endpoints, endpoint_index, cursor) = navigation_fixture();
        app.configure_commit_navigation(
            TuiSourceMode::Commit,
            endpoints,
            endpoint_index,
            Some(cursor),
            StepMode::Pairwise,
            None,
        );
        assert_eq!(
            app.comparison_line(),
            Some((
                "previous".to_string(),
                "HEAD~1".to_string(),
                "current".to_string(),
                "HEAD".to_string()
            ))
        );

        app.toggle_step_mode();
        app.configure_commit_navigation(
            TuiSourceMode::Commit,
            vec![
                StepEndpoint {
                    endpoint_id: "commit:aaa".to_string(),
                    display_ref: Some("HEAD~2".to_string()),
                    kind: StepEndpointKind::Commit {
                        sha: "aaa".to_string(),
                    },
                },
                StepEndpoint {
                    endpoint_id: "commit:bbb".to_string(),
                    display_ref: Some("HEAD~1".to_string()),
                    kind: StepEndpointKind::Commit {
                        sha: "bbb".to_string(),
                    },
                },
            ],
            HashMap::from([
                ("commit:aaa".to_string(), 0usize),
                ("commit:bbb".to_string(), 1usize),
            ]),
            Some(CommitCursor {
                endpoint_id: "commit:bbb".to_string(),
                index: 1,
                rev_label: Some("HEAD~1".to_string()),
                sha: "bbb".to_string(),
                subject: "feat".to_string(),
                has_older: true,
                has_newer: false,
            }),
            StepMode::Cumulative,
            Some("commit:aaa".to_string()),
        );
        assert_eq!(
            app.comparison_line(),
            Some((
                "base".to_string(),
                "HEAD~2".to_string(),
                "cursor".to_string(),
                "HEAD~1".to_string()
            ))
        );
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
                    endpoint_id: "commit:abc1234".to_string(),
                    index: 0,
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
                mode: StepMode::Pairwise,
                base_endpoint_id: None,
                comparison: StepComparison {
                    from_endpoint_id: "commit:abc1234".to_string(),
                    to_endpoint_id: "commit:abc1234".to_string(),
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
        app.configure_commit_navigation(
            TuiSourceMode::Unsupported,
            vec![],
            HashMap::new(),
            None,
            StepMode::Pairwise,
            None,
        );
        app.set_commit_loading(true);
        assert!(app.commit_loading());
        app.queue_commit_action(CommitStepAction::Older);
        assert_eq!(app.take_pending_navigation_request(), None);

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
    fn m_key_toggles_mode_and_queues_refresh() {
        let mut app = app();
        let (endpoints, endpoint_index, cursor) = navigation_fixture();
        app.configure_commit_navigation(
            TuiSourceMode::Commit,
            endpoints,
            endpoint_index,
            Some(cursor),
            StepMode::Pairwise,
            None,
        );
        assert_eq!(app.step_mode(), StepMode::Pairwise);

        app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        assert_eq!(app.step_mode(), StepMode::Cumulative);
        assert_eq!(
            app.take_pending_navigation_request(),
            Some(PendingNavigationRequest::Refresh)
        );
        assert_eq!(
            app.cumulative_base_endpoint_id(),
            Some("commit:aaaaaaa".to_string())
        );
    }

    #[test]
    fn cumulative_mode_without_explicit_base_anchors_to_first_endpoint() {
        let mut app = app();
        let (endpoints, endpoint_index, cursor) = navigation_fixture();
        app.configure_commit_navigation(
            TuiSourceMode::Commit,
            endpoints,
            endpoint_index,
            Some(cursor),
            StepMode::Cumulative,
            None,
        );

        assert_eq!(
            app.cumulative_base_endpoint_id(),
            Some("commit:aaaaaaa".to_string())
        );
        assert_eq!(
            app.comparison_line(),
            Some((
                "base".to_string(),
                "HEAD~1".to_string(),
                "cursor".to_string(),
                "HEAD".to_string()
            ))
        );
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

    #[test]
    fn review_toggle_tracks_review_records_when_hash_context_is_available() {
        let mut app = app();
        let (endpoints, endpoint_index, cursor) = navigation_fixture();
        app.configure_commit_navigation(
            TuiSourceMode::Commit,
            endpoints,
            endpoint_index,
            Some(cursor),
            StepMode::Pairwise,
            None,
        );

        assert!(app.toggle_selected_reviewed());
        let first_snapshot = app
            .take_review_state_dirty_snapshot()
            .expect("toggle should mark review state dirty");
        assert_eq!(first_snapshot.records.len(), 1);

        assert!(app.toggle_selected_reviewed());
        let second_snapshot = app
            .take_review_state_dirty_snapshot()
            .expect("second toggle should mark review state dirty");
        assert_eq!(second_snapshot.records.len(), 0);
    }

    #[test]
    fn review_toggle_noops_when_comparator_hash_source_is_unavailable() {
        let mut app = app();
        assert!(!app.toggle_selected_reviewed());
        assert_eq!(
            app.status_message(),
            Some("Review state unavailable for current comparator endpoint")
        );
    }

    #[test]
    fn review_filter_cycle_marks_persistence_dirty() {
        let mut app = app();
        assert_eq!(app.review_filter(), ReviewFilter::All);

        app.cycle_review_filter();
        let snapshot = app
            .take_review_state_dirty_snapshot()
            .expect("filter cycle should mark review state dirty");
        assert_eq!(snapshot.filter, ReviewFilter::Unreviewed);
    }
}
