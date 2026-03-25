use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sem_core::model::change::SemanticChange;
use sem_core::parser::differ::DiffResult;
use similar::{ChangeTag, TextDiff};
use std::collections::HashMap;

use super::review_state::{
    build_logical_entity_key, build_target_content_hash, current_updated_at,
    endpoint_supports_review_hash, Annotation, AnnotationFilter, PersistedDiffView,
    PersistedEntityContextMode, PersistedNavigationMode, PersistedViewMode, ReviewFilter,
    ReviewIdentity, ReviewStateData, ReviewStateUiPrefs,
};
use crate::commands::diff::{
    CommitCursor, CommitLoadStatus, CommitSnapshot, CommitStepAction, CommitStepResponse, DiffView,
    StepComparison, StepEndpoint, StepMode, TuiFileSnapshot, TuiSourceMode,
};

use super::detail::{
    file_snapshot_has_visible_hunks, render_change, render_file_snapshot, EntityContextMode,
    FileEntityLineRanges, FileLineRange, FileScopeHunkFilter, LineKind, RenderedDiff,
    SideBySideLine,
};

const MIN_SIDE_BY_SIDE_WIDTH: u16 = 120;
const MIN_SPLIT_PREVIEW_WIDTH: u16 = 80;
const MAX_ANNOTATION_LENGTH: usize = 256;

#[derive(Clone, Debug)]
pub struct ScopeRow {
    pub row_kind: ScopeRowKind,
    pub file_path: String,
    pub entity_type: String,
    pub entity_name: String,
    pub added_lines: usize,
    pub removed_lines: usize,
    pub range_label: Option<String>,
    pub change: Option<SemanticChange>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeRowKind {
    File,
    Entity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    List,
    Split,
    Detail,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowNavigationMode {
    #[default]
    Mixed,
    Entity,
    File,
}

impl RowNavigationMode {
    fn toggled(self) -> Self {
        match self {
            Self::Mixed => Self::Entity,
            Self::Entity => Self::File,
            Self::File => Self::Mixed,
        }
    }

    pub fn as_token(self) -> &'static str {
        match self {
            Self::Mixed => "mixed",
            Self::Entity => "entity",
            Self::File => "file",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowReviewState {
    Unavailable,
    Unreviewed,
    Reviewed,
    Mixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SplitFocus {
    Sidebar,
    Preview,
}

impl SplitFocus {
    fn toggled(self) -> Self {
        match self {
            Self::Sidebar => Self::Preview,
            Self::Preview => Self::Sidebar,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AnnotationInputState {
    text: String,
    cursor_position: usize,
    target_logical_entity_key: String,
    target_content_hash: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnnotationDeleteAction {
    Cancel,
    Delete,
}

impl AnnotationDeleteAction {
    fn toggle(self) -> Self {
        match self {
            Self::Cancel => Self::Delete,
            Self::Delete => Self::Cancel,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AnnotationDeleteConfirmationState {
    target_logical_entity_key: String,
    selected_action: AnnotationDeleteAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnnotationDeleteModal<'a> {
    pub selected_action: AnnotationDeleteAction,
    pub target_logical_entity_key: &'a str,
}

#[derive(Debug)]
pub struct AppState {
    rows: Vec<ScopeRow>,
    selected: usize,
    mode: Mode,
    last_non_detail_mode: Mode,
    requested_view: DiffView,
    entity_context_mode: EntityContextMode,
    navigation_mode: RowNavigationMode,
    split_focus: SplitFocus,
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
    file_snapshots: HashMap<String, TuiFileSnapshot>,
    commit_loading: bool,
    commit_status_message: Option<String>,
    pending_navigation_request: Option<PendingNavigationRequest>,
    review_filter: ReviewFilter,
    annotation_filter: AnnotationFilter,
    reviewed_records: HashMap<ReviewIdentity, String>,
    annotations: HashMap<String, Annotation>,
    row_review_identities: Vec<Option<ReviewIdentity>>,
    row_annotation_keys: Vec<Option<String>>,
    annotation_input: Option<AnnotationInputState>,
    annotation_delete_confirmation: Option<AnnotationDeleteConfirmationState>,
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
        Self::from_diff_result_with_snapshots(
            result,
            file_snapshots_from_diff_result(result),
            initial_view,
        )
    }

    pub fn from_diff_result_with_snapshots(
        result: &DiffResult,
        file_snapshots: HashMap<String, TuiFileSnapshot>,
        initial_view: DiffView,
    ) -> Self {
        let rows = Self::rows_from_diff_result(result);
        let row_count = rows.len();
        let selected = Self::initial_selected_index(&rows);

        let mut app = Self {
            rows,
            selected,
            mode: Mode::List,
            last_non_detail_mode: Mode::List,
            requested_view: initial_view,
            entity_context_mode: EntityContextMode::Hunk,
            navigation_mode: RowNavigationMode::Mixed,
            split_focus: SplitFocus::Sidebar,
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
            file_snapshots,
            commit_loading: false,
            commit_status_message: None,
            pending_navigation_request: None,
            review_filter: ReviewFilter::All,
            annotation_filter: AnnotationFilter::All,
            reviewed_records: HashMap::new(),
            annotations: HashMap::new(),
            row_review_identities: vec![None; row_count],
            row_annotation_keys: vec![None; row_count],
            annotation_input: None,
            annotation_delete_confirmation: None,
            review_status_message: None,
            review_state_dirty: false,
        };
        app.recompute_annotation_keys();
        app
    }

    fn rows_from_diff_result(result: &DiffResult) -> Vec<ScopeRow> {
        let mut changes = result.changes.clone();
        // Stable sort groups by file while preserving semantic order within each file.
        changes.sort_by(|a, b| a.file_path.cmp(&b.file_path));

        let mut rows = Vec::new();
        let mut index = 0usize;
        while index < changes.len() {
            let file_path = changes[index].file_path.clone();
            let mut file_rows = Vec::new();
            let mut file_added_lines = 0usize;
            let mut file_removed_lines = 0usize;

            while index < changes.len() && changes[index].file_path == file_path {
                let change = changes[index].clone();
                let (row_added_lines, row_removed_lines) = change_line_counts(&change);
                file_rows.push(ScopeRow {
                    row_kind: ScopeRowKind::Entity,
                    file_path: change.file_path.clone(),
                    entity_type: change.entity_type.clone(),
                    entity_name: change.entity_name.clone(),
                    added_lines: row_added_lines,
                    removed_lines: row_removed_lines,
                    range_label: range_label(&change),
                    change: Some(change),
                });
                file_added_lines = file_added_lines.saturating_add(row_added_lines);
                file_removed_lines = file_removed_lines.saturating_add(row_removed_lines);
                index = index.saturating_add(1);
            }

            rows.push(ScopeRow {
                row_kind: ScopeRowKind::File,
                file_path: file_path.clone(),
                entity_type: "file".to_string(),
                entity_name: file_path,
                added_lines: file_added_lines,
                removed_lines: file_removed_lines,
                range_label: None,
                change: None,
            });
            rows.extend(file_rows);
        }

        rows
    }

    fn initial_selected_index(rows: &[ScopeRow]) -> usize {
        rows.iter()
            .position(|row| row.row_kind == ScopeRowKind::Entity)
            .unwrap_or(0)
    }

    pub fn set_viewport(&mut self, width: u16, height: u16) {
        self.viewport_width = width;
        self.viewport_height = height;
        if self.mode == Mode::Split && !self.split_preview_available() {
            self.split_focus = SplitFocus::Sidebar;
        }
    }

    pub fn rows(&self) -> &[ScopeRow] {
        &self.rows
    }

    pub fn visible_row_indices(&self) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter_map(|(row_index, row)| match row.row_kind {
                ScopeRowKind::File => self.file_row_has_visible_child(row_index).then_some(row_index),
                ScopeRowKind::Entity => self.entity_row_matches_filter(row_index).then_some(row_index),
            })
            .collect()
    }

    pub fn selected_row(&self) -> Option<&ScopeRow> {
        self.selected_row_index()
            .and_then(|row_index| self.rows.get(row_index))
    }

    fn selected_entity_row_index(&self) -> Option<usize> {
        let row_index = self.selected_row_index()?;
        (self.rows.get(row_index)?.row_kind == ScopeRowKind::Entity).then_some(row_index)
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
        self.recompute_annotation_keys();
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

    #[cfg(test)]
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

    pub fn annotation_filter(&self) -> AnnotationFilter {
        self.annotation_filter
    }

    pub fn annotation_input_active(&self) -> bool {
        self.annotation_input.is_some()
    }

    pub fn annotation_delete_modal_active(&self) -> bool {
        self.annotation_delete_confirmation.is_some()
    }

    pub fn is_row_reviewed(&self, row_index: usize) -> bool {
        self.row_review_state(row_index) == RowReviewState::Reviewed
    }

    pub fn row_review_state(&self, row_index: usize) -> RowReviewState {
        let Some(row) = self.rows.get(row_index) else {
            return RowReviewState::Unavailable;
        };

        match row.row_kind {
            ScopeRowKind::Entity => self.entity_row_review_state(row_index),
            ScopeRowKind::File => self.file_row_review_state(row_index),
        }
    }

    pub fn apply_review_state(&mut self, state: ReviewStateData) {
        let ReviewStateData {
            filter,
            annotation_filter,
            ui_prefs,
            annotations,
            records,
        } = state;
        let prior_selected = self.selected;
        self.review_filter = filter;
        self.annotation_filter = annotation_filter;
        self.reviewed_records = records;
        self.annotations = annotations;

        if let Some(annotation_filter) = ui_prefs.annotation_filter {
            self.annotation_filter = annotation_filter;
        }
        if let Some(view) = ui_prefs.diff_view {
            self.requested_view = from_persisted_diff_view(view);
        }
        if let Some(context_mode) = ui_prefs.entity_context_mode {
            self.entity_context_mode = from_persisted_entity_context_mode(context_mode);
        }
        if let Some(navigation_mode) = ui_prefs.navigation_mode {
            self.navigation_mode = from_persisted_navigation_mode(navigation_mode);
        }
        self.realign_selection_after_visibility_change(prior_selected);
        if let Some(view_mode) = ui_prefs.view_mode {
            self.apply_persisted_view_mode(from_persisted_view_mode(view_mode));
        }

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
            annotation_filter: self.annotation_filter,
            ui_prefs: ReviewStateUiPrefs {
                annotation_filter: Some(self.annotation_filter),
                view_mode: Some(to_persisted_view_mode(self.mode)),
                diff_view: Some(to_persisted_diff_view(self.requested_view)),
                entity_context_mode: Some(to_persisted_entity_context_mode(
                    self.entity_context_mode,
                )),
                navigation_mode: Some(to_persisted_navigation_mode(self.navigation_mode)),
            },
            annotations: self.annotations.clone(),
            records: self.reviewed_records.clone(),
        }
    }

    pub fn toggle_selected_reviewed(&mut self) -> bool {
        let prior_selected = self.selected;
        let Some(row_index) = self.selected_row_index() else {
            return false;
        };
        let toggled = match self.rows.get(row_index).map(|row| row.row_kind) {
            Some(ScopeRowKind::Entity) => self.toggle_entity_row_reviewed(row_index),
            Some(ScopeRowKind::File) => self.toggle_file_row_reviewed(row_index),
            None => false,
        };
        if !toggled {
            return false;
        }

        self.review_state_dirty = true;
        self.realign_selection_after_visibility_change(prior_selected);
        if self.mode == Mode::Detail {
            self.refresh_detail();
        }
        true
    }

    pub fn cycle_review_filter(&mut self) {
        let prior_selected = self.selected;
        self.review_filter = self.review_filter.cycle();
        self.review_state_dirty = true;
        self.realign_selection_after_visibility_change(prior_selected);
        if self.mode == Mode::Detail {
            self.refresh_detail();
        }
    }

    pub fn cycle_annotation_filter(&mut self) {
        let prior_selected = self.selected;
        self.annotation_filter = self.annotation_filter.cycle();
        self.review_state_dirty = true;
        self.realign_selection_after_visibility_change(prior_selected);
        if self.mode == Mode::Detail {
            self.refresh_detail();
        }
    }

    pub fn toggle_navigation_mode(&mut self) {
        let prior_selected = self.selected;
        self.navigation_mode = self.navigation_mode.toggled();
        self.review_state_dirty = true;
        self.realign_selection_after_visibility_change(prior_selected);
        if self.mode == Mode::Detail {
            self.refresh_detail();
        }
    }

    pub fn set_commit_loading(&mut self, loading: bool) {
        self.commit_loading = loading;
    }

    pub fn is_row_annotated(&self, row_index: usize) -> bool {
        self.row_annotation_keys
            .get(row_index)
            .and_then(|key| key.as_ref())
            .map(|key| self.annotations.contains_key(key))
            .unwrap_or(false)
    }

    pub fn row_annotation_text(&self, row_index: usize) -> Option<&str> {
        self.row_annotation_keys
            .get(row_index)
            .and_then(|key| key.as_ref())
            .and_then(|key| self.annotations.get(key))
            .map(|annotation| annotation.text.as_str())
    }

    pub fn row_annotation_hash_matches(&self, row_index: usize) -> bool {
        let Some(logical_entity_key) = self
            .row_annotation_keys
            .get(row_index)
            .and_then(|key| key.as_ref())
        else {
            return true;
        };
        let Some(annotation) = self.annotations.get(logical_entity_key) else {
            return true;
        };
        let Some(row) = self.rows.get(row_index) else {
            return true;
        };
        let current_hash = row.change.as_ref().and_then(build_target_content_hash);
        match (
            annotation.content_hash_at_creation.as_deref(),
            current_hash.as_deref(),
        ) {
            (Some(annotation_hash), Some(current_hash)) => annotation_hash == current_hash,
            _ => true,
        }
    }

    pub fn annotation_input_text(&self) -> Option<&str> {
        self.annotation_input
            .as_ref()
            .map(|input| input.text.as_str())
    }

    pub fn annotation_input_cursor_position(&self) -> Option<usize> {
        self.annotation_input
            .as_ref()
            .map(|input| input.cursor_position)
    }

    pub fn selected_row_annotation(&self) -> Option<(&str, bool)> {
        let row_index = self.selected_row_index()?;
        let text = self.row_annotation_text(row_index)?;
        Some((text, self.row_annotation_hash_matches(row_index)))
    }

    pub fn annotation_delete_modal(&self) -> Option<AnnotationDeleteModal<'_>> {
        let confirmation = self.annotation_delete_confirmation.as_ref()?;
        Some(AnnotationDeleteModal {
            selected_action: confirmation.selected_action,
            target_logical_entity_key: &confirmation.target_logical_entity_key,
        })
    }

    fn selected_row_annotation_key(&self) -> Option<String> {
        let row_index = self.selected_entity_row_index()?;
        self.row_annotation_keys
            .get(row_index)
            .and_then(|key| key.clone())
    }

    fn begin_annotation_input(&mut self) {
        let Some(row_index) = self.selected_entity_row_index() else {
            return;
        };
        let Some(logical_entity_key) = self
            .row_annotation_keys
            .get(row_index)
            .and_then(|key| key.clone())
        else {
            return;
        };

        let existing_text = self
            .annotations
            .get(&logical_entity_key)
            .map(|annotation| annotation.text.clone())
            .unwrap_or_default();
        let target_content_hash = self
            .rows
            .get(row_index)
            .and_then(|row| row.change.as_ref())
            .and_then(build_target_content_hash);
        self.annotation_input = Some(AnnotationInputState {
            cursor_position: existing_text.len(),
            text: existing_text,
            target_logical_entity_key: logical_entity_key,
            target_content_hash,
        });
        self.annotation_delete_confirmation = None;
        self.review_status_message = None;
    }

    fn confirm_annotation(&mut self) -> bool {
        let Some(input) = self.annotation_input.take() else {
            return false;
        };

        if input.text.trim().is_empty() {
            return false;
        }

        let updated_at = current_updated_at();
        let created_at = self
            .annotations
            .get(&input.target_logical_entity_key)
            .map(|annotation| annotation.created_at.clone())
            .unwrap_or_else(|| updated_at.clone());

        self.annotations.insert(
            input.target_logical_entity_key,
            Annotation {
                text: input.text,
                content_hash_at_creation: input.target_content_hash,
                created_at,
                updated_at,
            },
        );
        self.review_state_dirty = true;
        self.review_status_message = None;
        true
    }

    fn cancel_annotation(&mut self) {
        self.annotation_input = None;
    }

    fn begin_annotation_delete_confirmation(&mut self) -> bool {
        let Some(_) = self.selected_row_index() else {
            return false;
        };
        let Some(logical_entity_key) = self.selected_row_annotation_key() else {
            return false;
        };
        if !self.annotations.contains_key(&logical_entity_key) {
            return false;
        }

        self.annotation_delete_confirmation = Some(AnnotationDeleteConfirmationState {
            target_logical_entity_key: logical_entity_key,
            selected_action: AnnotationDeleteAction::Cancel,
        });
        self.review_status_message = None;
        true
    }

    fn confirm_annotation_delete(&mut self) -> bool {
        let Some(confirmation) = self.annotation_delete_confirmation.take() else {
            return false;
        };

        if confirmation.selected_action == AnnotationDeleteAction::Cancel {
            self.review_status_message = Some("annotation delete cancelled".to_string());
            return false;
        }

        if self
            .annotations
            .remove(&confirmation.target_logical_entity_key)
            .is_some()
        {
            self.review_state_dirty = true;
            self.review_status_message = Some("annotation removed".to_string());
            return true;
        }

        false
    }

    fn cancel_annotation_delete_confirmation(&mut self) {
        self.annotation_delete_confirmation = None;
        self.review_status_message = Some("annotation delete cancelled".to_string());
    }

    fn move_annotation_delete_confirmation_left(&mut self) {
        let Some(confirmation) = self.annotation_delete_confirmation.as_mut() else {
            return;
        };
        confirmation.selected_action = AnnotationDeleteAction::Cancel;
    }

    fn move_annotation_delete_confirmation_right(&mut self) {
        let Some(confirmation) = self.annotation_delete_confirmation.as_mut() else {
            return;
        };
        confirmation.selected_action = AnnotationDeleteAction::Delete;
    }

    fn toggle_annotation_delete_confirmation_action(&mut self) {
        let Some(confirmation) = self.annotation_delete_confirmation.as_mut() else {
            return;
        };
        confirmation.selected_action = confirmation.selected_action.toggle();
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
        if self.annotation_input.take().is_some() {
            self.review_status_message =
                Some("annotation input cancelled: commit step applied".to_string());
        } else if self.annotation_delete_confirmation.take().is_some() {
            self.review_status_message =
                Some("annotation delete cancelled: commit step applied".to_string());
        }
        self.commit_cursor = Some(snapshot.cursor);
        self.step_mode = snapshot.mode;
        self.cumulative_base_endpoint_id = snapshot.base_endpoint_id;
        self.comparison = Some(snapshot.comparison);
        self.file_snapshots = snapshot.file_snapshots;
        self.rows = Self::rows_from_diff_result(&snapshot.result);
        self.recompute_review_identities();
        self.recompute_annotation_keys();
        self.selected = Self::initial_selected_index(&self.rows);
        self.realign_selection_after_visibility_change(self.selected);
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

    pub fn mode_token(&self) -> &'static str {
        match self.mode {
            Mode::List => "list",
            Mode::Split => "split",
            Mode::Detail => "detail",
        }
    }

    pub fn viewport_size(&self) -> (u16, u16) {
        (self.viewport_width, self.viewport_height)
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

    pub fn entity_context_mode(&self) -> EntityContextMode {
        self.entity_context_mode
    }

    pub fn navigation_mode(&self) -> RowNavigationMode {
        self.navigation_mode
    }

    pub fn fallback_active(&self) -> bool {
        self.requested_view == DiffView::SideBySide && self.effective_view() == DiffView::Unified
    }

    fn split_preview_available(&self) -> bool {
        self.viewport_width >= MIN_SPLIT_PREVIEW_WIDTH
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

    pub fn handle_list_mouse_scroll(&mut self, scroll_down: bool) {
        if self.mode != Mode::List {
            return;
        }

        if scroll_down {
            if let Some(position) = self.next_navigable_visible_position(false) {
                self.selected = position;
            }
        } else if let Some(position) = self.previous_navigable_visible_position(false) {
            self.selected = position;
        }
    }

    pub fn handle_list_click(&mut self, list_selection: usize) {
        if self.mode != Mode::List {
            return;
        }

        let visible_len = self.visible_row_indices().len();
        if list_selection >= visible_len {
            return;
        }
        self.selected = list_selection;
        self.open_detail();
    }

    pub fn handle_split_mouse_scroll(&mut self, preview_pane: bool, scroll_down: bool) {
        if self.mode != Mode::Split {
            return;
        }

        if preview_pane && self.split_preview_available() {
            self.split_focus = SplitFocus::Preview;
            self.sync_active_selection_detail();
            if scroll_down {
                self.scroll_line_down();
            } else {
                self.scroll_line_up();
            }
            return;
        }

        self.split_focus = SplitFocus::Sidebar;
        if scroll_down {
            self.split_sidebar_move_down();
        } else {
            self.split_sidebar_move_up();
        }
    }

    pub fn handle_split_sidebar_click(&mut self, sidebar_selection: usize) {
        if self.mode != Mode::Split {
            return;
        }

        let visible_len = self.visible_row_indices().len();
        if sidebar_selection >= visible_len {
            return;
        }

        self.selected = sidebar_selection;
        self.split_focus = SplitFocus::Sidebar;
    }

    pub fn detail_title(&self) -> String {
        let Some(row) = self.selected_row() else {
            return "Detail".to_string();
        };

        match row.row_kind {
            ScopeRowKind::File => format!("file {}", row.file_path),
            ScopeRowKind::Entity => match &row.range_label {
                Some(range) => format!("entity {} {} {}", row.file_path, row.entity_name, range),
                None => format!("entity {} {}", row.file_path, row.entity_name),
            },
        }
    }

    pub fn render_selected_scope(&self) -> Option<RenderedDiff> {
        let row_index = self.selected_row_index()?;
        self.rows
            .get(row_index)
            .map(|row| self.render_scope_row(row_index, row))
    }

    fn render_scope_row(&self, row_index: usize, row: &ScopeRow) -> RenderedDiff {
        match row.row_kind {
            ScopeRowKind::File => {
                let snapshot = self.file_snapshots.get(&row.file_path);
                let hunk_filter = (self.entity_context_mode == EntityContextMode::Hunk)
                    .then(|| self.file_scope_hunk_filter(row_index));
                render_file_snapshot(
                    snapshot.and_then(|snapshot| snapshot.before_content.as_deref()),
                    snapshot.and_then(|snapshot| snapshot.after_content.as_deref()),
                    self.entity_context_mode,
                    hunk_filter.as_ref(),
                )
            }
            ScopeRowKind::Entity => row
                .change
                .as_ref()
                .map(|change| render_change(change, self.entity_context_mode))
                .unwrap_or_else(RenderedDiff::unavailable),
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

        if self.annotation_input.is_some() {
            self.handle_annotation_input_key(key);
            return;
        }

        if self.annotation_delete_confirmation.is_some() {
            self.handle_annotation_delete_confirmation_key(key);
            return;
        }

        if key.code == KeyCode::Char('v') {
            self.cycle_view_mode();
            return;
        }

        match self.mode {
            Mode::List => self.handle_list_key(key),
            Mode::Split => self.handle_split_key(key),
            Mode::Detail => self.handle_detail_key(key),
        }
    }

    fn handle_list_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('a') => self.begin_annotation_input(),
            KeyCode::Char('A') => self.cycle_annotation_filter(),
            KeyCode::Char('D') => {
                let _ = self.begin_annotation_delete_confirmation();
            }
            KeyCode::Char('[') => self.queue_commit_action(CommitStepAction::Older),
            KeyCode::Char(']') => self.queue_commit_action(CommitStepAction::Newer),
            KeyCode::Char('m') => self.toggle_step_mode(),
            KeyCode::Char('e') => self.toggle_entity_context_mode(),
            KeyCode::Char('f') => self.toggle_navigation_mode(),
            KeyCode::Char(' ') => {
                let _ = self.toggle_selected_reviewed();
            }
            KeyCode::Char('r') => self.cycle_review_filter(),
            KeyCode::Up | KeyCode::Char('k') => self.move_up(),
            KeyCode::Down | KeyCode::Char('j') => self.move_down(),
            KeyCode::Char('g') => self.select_first_navigable(),
            KeyCode::Char('G') => {
                self.select_last_navigable();
            }
            KeyCode::Enter => self.open_detail(),
            _ => {}
        }
    }

    fn handle_split_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('a') => self.begin_annotation_input(),
            KeyCode::Char('A') => self.cycle_annotation_filter(),
            KeyCode::Char('D') => {
                let _ = self.begin_annotation_delete_confirmation();
            }
            KeyCode::Char('[') => self.queue_commit_action(CommitStepAction::Older),
            KeyCode::Char(']') => self.queue_commit_action(CommitStepAction::Newer),
            KeyCode::Char('m') => self.toggle_step_mode(),
            KeyCode::Char('e') => self.toggle_entity_context_mode(),
            KeyCode::Char('f') => self.toggle_navigation_mode(),
            KeyCode::Char(' ') => {
                let _ = self.toggle_selected_reviewed();
            }
            KeyCode::Char('r') => self.cycle_review_filter(),
            KeyCode::Up | KeyCode::Char('k') => self.split_move_up(),
            KeyCode::Down | KeyCode::Char('j') => self.split_move_down(),
            KeyCode::Left => self.split_previous_scope(),
            KeyCode::Right => self.split_next_scope(),
            KeyCode::Tab => self.toggle_split_focus(),
            KeyCode::Char('s') => self.toggle_view(),
            KeyCode::Char('n') => self.next_hunk(),
            KeyCode::Char('p') => self.previous_hunk(),
            KeyCode::Char('g') => self.split_jump_top(),
            KeyCode::Char('G') => {
                self.split_jump_bottom();
            }
            KeyCode::Enter => self.open_detail(),
            _ => {}
        }
    }

    fn handle_detail_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('a') => self.begin_annotation_input(),
            KeyCode::Char('A') => self.cycle_annotation_filter(),
            KeyCode::Char('D') => {
                let _ = self.begin_annotation_delete_confirmation();
            }
            KeyCode::Char('[') => self.queue_commit_action(CommitStepAction::Older),
            KeyCode::Char(']') => self.queue_commit_action(CommitStepAction::Newer),
            KeyCode::Char('m') => self.toggle_step_mode(),
            KeyCode::Char('e') => self.toggle_entity_context_mode(),
            KeyCode::Char('f') => self.toggle_navigation_mode(),
            KeyCode::Char(' ') => {
                let _ = self.toggle_selected_reviewed();
            }
            KeyCode::Char('r') => self.cycle_review_filter(),
            KeyCode::Esc => self.close_detail(),
            KeyCode::Left => self.previous_scope(),
            KeyCode::Right => self.next_scope(),
            KeyCode::Char('s') => self.toggle_view(),
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

    fn handle_annotation_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.confirm_annotation();
            }
            KeyCode::Esc => self.cancel_annotation(),
            KeyCode::Left => self.move_annotation_cursor_left(),
            KeyCode::Right => self.move_annotation_cursor_right(),
            KeyCode::Home => self.move_annotation_cursor_home(),
            KeyCode::End => self.move_annotation_cursor_end(),
            KeyCode::Backspace => self.delete_annotation_char_before_cursor(),
            KeyCode::Delete => self.delete_annotation_char_at_cursor(),
            KeyCode::Char(character)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.insert_annotation_char(character);
            }
            _ => {}
        }
    }

    fn handle_annotation_delete_confirmation_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.cancel_annotation_delete_confirmation(),
            KeyCode::Enter => {
                self.confirm_annotation_delete();
            }
            KeyCode::Left | KeyCode::Up | KeyCode::Char('h') | KeyCode::Char('k') => {
                self.move_annotation_delete_confirmation_left();
            }
            KeyCode::Right | KeyCode::Down | KeyCode::Char('l') | KeyCode::Char('j') => {
                self.move_annotation_delete_confirmation_right();
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.toggle_annotation_delete_confirmation_action();
            }
            _ => {}
        }
    }

    fn cycle_view_mode(&mut self) {
        match self.mode {
            Mode::List => {
                self.mode = Mode::Split;
                self.last_non_detail_mode = Mode::Split;
                self.split_focus = SplitFocus::Sidebar;
                self.detail_scroll = 0;
                self.detail_hunk_index = 0;
                self.detail = None;
            }
            Mode::Split => {
                self.last_non_detail_mode = Mode::Split;
                self.mode = Mode::Detail;
                self.refresh_detail();
            }
            Mode::Detail => {
                self.mode = Mode::List;
                self.last_non_detail_mode = Mode::List;
                self.detail_scroll = 0;
                self.detail_hunk_index = 0;
                self.detail = None;
            }
        }
        self.review_state_dirty = true;
    }

    fn move_up(&mut self) {
        if let Some(position) = self.previous_navigable_visible_position(true) {
            self.selected = position;
        }
    }

    fn move_down(&mut self) {
        if let Some(position) = self.next_navigable_visible_position(true) {
            self.selected = position;
        }
    }

    fn open_detail(&mut self) {
        if self.visible_row_indices().is_empty() {
            return;
        }
        self.last_non_detail_mode = self.current_non_detail_mode();
        self.mode = Mode::Detail;
        self.refresh_detail();
        self.review_state_dirty = true;
    }

    fn next_scope(&mut self) {
        if let Some(position) = self.next_navigable_visible_position(true) {
            self.selected = position;
            self.refresh_detail();
        }
    }

    fn previous_scope(&mut self) {
        if let Some(position) = self.previous_navigable_visible_position(true) {
            self.selected = position;
            self.refresh_detail();
        }
    }

    fn refresh_detail(&mut self) {
        if let Some(row_index) = self.selected_row_index() {
            if let Some(row) = self.rows.get(row_index) {
                self.detail = Some(self.render_scope_row(row_index, row));
            } else {
                self.detail = None;
            }
        } else {
            self.detail = None;
        }

        self.detail_scroll = 0;
        self.detail_hunk_index = 0;
        self.jump_to_hunk();
    }

    fn close_detail(&mut self) {
        self.mode = self.last_non_detail_mode;
        self.detail_scroll = 0;
        self.detail_hunk_index = 0;
        self.detail = None;
        self.review_state_dirty = true;
    }

    fn toggle_view(&mut self) {
        self.requested_view = match self.requested_view {
            DiffView::Unified => DiffView::SideBySide,
            DiffView::SideBySide => DiffView::Unified,
        };
        self.review_state_dirty = true;

        if self.mode == Mode::Detail {
            self.detail_hunk_index = 0;
            self.jump_to_hunk();
        }
    }

    fn toggle_split_focus(&mut self) {
        if self.mode != Mode::Split {
            return;
        }
        if !self.split_preview_available() {
            self.split_focus = SplitFocus::Sidebar;
            return;
        }
        self.split_focus = self.split_focus.toggled();
        if self.split_focus == SplitFocus::Preview {
            self.sync_active_selection_detail();
        }
    }

    fn split_move_up(&mut self) {
        if !self.split_preview_available() {
            self.split_sidebar_move_up();
            return;
        }
        match self.split_focus {
            SplitFocus::Sidebar => self.split_sidebar_move_up(),
            SplitFocus::Preview => {
                self.sync_active_selection_detail();
                self.scroll_line_up();
            }
        }
    }

    fn split_move_down(&mut self) {
        if !self.split_preview_available() {
            self.split_sidebar_move_down();
            return;
        }
        match self.split_focus {
            SplitFocus::Sidebar => self.split_sidebar_move_down(),
            SplitFocus::Preview => {
                self.sync_active_selection_detail();
                self.scroll_line_down();
            }
        }
    }

    fn split_sidebar_move_up(&mut self) {
        if let Some(position) = self.previous_navigable_visible_position(false) {
            self.selected = position;
        }
    }

    fn split_sidebar_move_down(&mut self) {
        if let Some(position) = self.next_navigable_visible_position(false) {
            self.selected = position;
        }
    }

    fn split_jump_top(&mut self) {
        if self.split_preview_available() && self.split_focus == SplitFocus::Preview {
            self.sync_active_selection_detail();
            self.detail_scroll = 0;
            return;
        }

        self.select_first_navigable();
    }

    fn split_jump_bottom(&mut self) {
        if self.split_preview_available() && self.split_focus == SplitFocus::Preview {
            self.sync_active_selection_detail();
            self.detail_scroll = self.max_scroll();
            return;
        }

        self.select_last_navigable();
    }

    fn split_previous_scope(&mut self) {
        if self.split_preview_available() && self.split_focus == SplitFocus::Preview {
            self.previous_scope();
        }
    }

    fn split_next_scope(&mut self) {
        if self.split_preview_available() && self.split_focus == SplitFocus::Preview {
            self.next_scope();
        }
    }

    fn toggle_entity_context_mode(&mut self) {
        self.entity_context_mode = self.entity_context_mode.toggled();
        self.review_state_dirty = true;

        if self.mode == Mode::Detail {
            let prior_hunk_index = self.detail_hunk_index;
            if let Some(row_index) = self.selected_row_index() {
                if let Some(row) = self.rows.get(row_index) {
                    self.detail = Some(self.render_scope_row(row_index, row));
                } else {
                    self.detail = None;
                }
            } else {
                self.detail = None;
            }
            let hunk_count = self.hunk_positions().len();
            self.detail_hunk_index = prior_hunk_index.min(hunk_count.saturating_sub(1));
            self.jump_to_hunk();
        }
    }

    fn next_hunk(&mut self) {
        if self.mode == Mode::Split {
            self.sync_active_selection_detail();
        }
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
        if self.mode == Mode::Split {
            self.sync_active_selection_detail();
        }
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

    fn sync_active_selection_detail(&mut self) {
        if let Some(row_index) = self.selected_row_index() {
            if let Some(row) = self.rows.get(row_index) {
                self.detail = Some(self.render_scope_row(row_index, row));
            } else {
                self.detail = None;
            }
        } else {
            self.detail = None;
        }
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

    fn selected_row_index(&self) -> Option<usize> {
        let visible = self.visible_row_indices();
        visible.get(self.selected).copied()
    }

    fn entity_row_matches_filter(&self, row_index: usize) -> bool {
        let review_matches = match self.review_filter {
            ReviewFilter::All => true,
            ReviewFilter::Unreviewed => !self.is_row_reviewed(row_index),
            ReviewFilter::Reviewed => self.is_row_reviewed(row_index),
        };
        let annotation_matches = match self.annotation_filter {
            AnnotationFilter::All => true,
            AnnotationFilter::Annotated => self.is_row_annotated(row_index),
            AnnotationFilter::Unannotated => !self.is_row_annotated(row_index),
        };
        review_matches && annotation_matches
    }

    fn entity_row_review_state(&self, row_index: usize) -> RowReviewState {
        self.row_review_identities
            .get(row_index)
            .and_then(|identity| identity.as_ref())
            .map(|identity| {
                if self.reviewed_records.contains_key(identity) {
                    RowReviewState::Reviewed
                } else {
                    RowReviewState::Unreviewed
                }
            })
            .unwrap_or(RowReviewState::Unavailable)
    }

    fn file_row_review_state(&self, row_index: usize) -> RowReviewState {
        let mut any_reviewable = false;
        let mut any_reviewed = false;
        let mut all_reviewed = true;

        for child_index in self.file_child_entity_indices(row_index) {
            let child_state = self.entity_row_review_state(child_index);
            if child_state == RowReviewState::Unavailable {
                continue;
            }
            any_reviewable = true;
            let reviewed = child_state == RowReviewState::Reviewed;
            any_reviewed |= reviewed;
            all_reviewed &= reviewed;
        }

        if !any_reviewable {
            RowReviewState::Unavailable
        } else if all_reviewed {
            RowReviewState::Reviewed
        } else if !any_reviewed {
            RowReviewState::Unreviewed
        } else {
            RowReviewState::Mixed
        }
    }

    fn file_row_has_visible_child(&self, row_index: usize) -> bool {
        let Some(row) = self.rows.get(row_index) else {
            return false;
        };
        if row.row_kind != ScopeRowKind::File {
            return false;
        }

        self.rows
            .iter()
            .enumerate()
            .skip(row_index + 1)
            .take_while(|(_, child)| child.row_kind != ScopeRowKind::File)
            .any(|(child_index, _)| self.entity_row_matches_filter(child_index))
            || self.file_row_has_visible_residual_hunks(row_index)
    }

    fn file_child_entity_indices(&self, row_index: usize) -> Vec<usize> {
        let Some(row) = self.rows.get(row_index) else {
            return Vec::new();
        };
        if row.row_kind != ScopeRowKind::File {
            return Vec::new();
        }

        self.rows
            .iter()
            .enumerate()
            .skip(row_index + 1)
            .take_while(|(_, child)| child.row_kind != ScopeRowKind::File)
            .filter_map(|(child_index, child)| {
                (child.row_kind == ScopeRowKind::Entity).then_some(child_index)
            })
            .collect()
    }

    fn reviewable_file_child_identities(&self, row_index: usize) -> Vec<ReviewIdentity> {
        self.file_child_entity_indices(row_index)
            .into_iter()
            .filter_map(|child_index| {
                self.row_review_identities
                    .get(child_index)
                    .and_then(|identity| identity.as_ref())
                    .cloned()
            })
            .collect()
    }

    fn file_scope_hunk_filter(&self, row_index: usize) -> FileScopeHunkFilter {
        let child_indices = self.file_child_entity_indices(row_index);
        let all_entity_ranges: Vec<_> = child_indices
            .iter()
            .filter_map(|child_index| self.file_entity_line_ranges(*child_index))
            .collect();
        let visible_entity_ranges: Vec<_> = child_indices
            .iter()
            .copied()
            .filter(|child_index| self.entity_row_matches_filter(*child_index))
            .filter_map(|child_index| self.file_entity_line_ranges(child_index))
            .collect();

        FileScopeHunkFilter {
            visible_entity_ranges,
            all_entity_ranges,
            show_residual: self.review_filter != ReviewFilter::Reviewed
                && self.annotation_filter != AnnotationFilter::Annotated,
        }
    }

    fn file_row_has_visible_residual_hunks(&self, row_index: usize) -> bool {
        if self.review_filter == ReviewFilter::Reviewed
            || self.annotation_filter == AnnotationFilter::Annotated
        {
            return false;
        }

        let Some(row) = self.rows.get(row_index) else {
            return false;
        };
        let Some(snapshot) = self.file_snapshots.get(&row.file_path) else {
            return false;
        };
        let filter = self.file_scope_hunk_filter(row_index);
        file_snapshot_has_visible_hunks(
            snapshot.before_content.as_deref(),
            snapshot.after_content.as_deref(),
            &filter,
        )
    }

    fn file_entity_line_ranges(&self, row_index: usize) -> Option<FileEntityLineRanges> {
        let change = self.rows.get(row_index)?.change.as_ref()?;
        Some(FileEntityLineRanges {
            old_range: file_line_range(change.before_start_line, change.before_end_line),
            new_range: file_line_range(change.after_start_line, change.after_end_line),
        })
    }

    fn row_index_matches_navigation_mode(&self, row_index: usize) -> bool {
        match self.navigation_mode {
            RowNavigationMode::Mixed => true,
            RowNavigationMode::Entity => {
                self.rows.get(row_index).map(|row| row.row_kind) == Some(ScopeRowKind::Entity)
            }
            RowNavigationMode::File => {
                self.rows.get(row_index).map(|row| row.row_kind) == Some(ScopeRowKind::File)
            }
        }
    }

    fn navigable_visible_positions(&self) -> Vec<usize> {
        self.visible_row_indices()
            .iter()
            .enumerate()
            .filter_map(|(visible_position, row_index)| {
                self.row_index_matches_navigation_mode(*row_index)
                    .then_some(visible_position)
            })
            .collect()
    }

    fn next_navigable_visible_position(&self, wrap: bool) -> Option<usize> {
        let navigable = self.navigable_visible_positions();
        if navigable.is_empty() {
            return None;
        }

        navigable
            .iter()
            .copied()
            .find(|position| *position > self.selected)
            .or_else(|| wrap.then(|| navigable[0]))
    }

    fn previous_navigable_visible_position(&self, wrap: bool) -> Option<usize> {
        let navigable = self.navigable_visible_positions();
        if navigable.is_empty() {
            return None;
        }

        navigable
            .iter()
            .rev()
            .copied()
            .find(|position| *position < self.selected)
            .or_else(|| wrap.then(|| *navigable.last().expect("navigable checked non-empty")))
    }

    fn select_first_navigable(&mut self) {
        if let Some(position) = self.navigable_visible_positions().into_iter().next() {
            self.selected = position;
        }
    }

    fn select_last_navigable(&mut self) {
        if let Some(position) = self.navigable_visible_positions().into_iter().last() {
            self.selected = position;
        }
    }

    fn realign_selection_after_visibility_change(&mut self, prior_selected: usize) {
        let visible_len = self.visible_row_indices().len();
        if visible_len == 0 {
            self.selected = 0;
            return;
        }

        let target = if prior_selected >= visible_len {
            0
        } else {
            prior_selected
        };
        let navigable = self.navigable_visible_positions();
        self.selected = navigable
            .iter()
            .copied()
            .find(|position| *position >= target)
            .or_else(|| navigable.first().copied())
            .unwrap_or(target);
    }

    fn toggle_entity_row_reviewed(&mut self, row_index: usize) -> bool {
        let Some(identity) = self
            .row_review_identities
            .get(row_index)
            .and_then(|identity| identity.as_ref())
            .cloned()
        else {
            self.review_status_message =
                Some("Review state unavailable for current comparator endpoint".to_string());
            return false;
        };

        self.review_status_message = None;

        if self.reviewed_records.contains_key(&identity) {
            self.reviewed_records.remove(&identity);
        } else {
            self.reviewed_records.insert(identity, current_updated_at());
        }

        true
    }

    fn toggle_file_row_reviewed(&mut self, row_index: usize) -> bool {
        let identities = self.reviewable_file_child_identities(row_index);
        if identities.is_empty() {
            self.review_status_message =
                Some("Review state unavailable for current comparator endpoint".to_string());
            return false;
        }

        self.review_status_message = None;
        let mark_all_reviewed = self.file_row_review_state(row_index) != RowReviewState::Reviewed;
        let updated_at = current_updated_at();

        for identity in identities {
            if mark_all_reviewed {
                self.reviewed_records.insert(identity, updated_at.clone());
            } else {
                self.reviewed_records.remove(&identity);
            }
        }

        true
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
        let prior_selected = self.selected;
        self.row_review_identities = vec![None; self.rows.len()];

        let to_endpoint_id = self
            .comparison
            .as_ref()
            .map(|cmp| cmp.to_endpoint_id.as_str());
        if !endpoint_supports_review_hash(to_endpoint_id) {
            self.realign_selection_after_visibility_change(prior_selected);
            return;
        }

        let mut fallback_ordinals: HashMap<(String, String, String), usize> = HashMap::new();
        for (row_index, row) in self.rows.iter().enumerate() {
            if row.row_kind != ScopeRowKind::Entity {
                continue;
            }
            let group_key = (
                row.file_path.clone(),
                row.entity_type.clone(),
                row.entity_name.clone(),
            );
            let ordinal = fallback_ordinals.entry(group_key).or_insert(0);
            *ordinal = ordinal.saturating_add(1);

            let Some(change) = row.change.as_ref() else {
                continue;
            };
            let logical_entity_key = build_logical_entity_key(change, *ordinal);
            let Some(target_content_hash) = build_target_content_hash(change) else {
                continue;
            };

            self.row_review_identities[row_index] = Some(ReviewIdentity {
                logical_entity_key,
                target_content_hash,
            });
        }

        self.realign_selection_after_visibility_change(prior_selected);
    }

    fn recompute_annotation_keys(&mut self) {
        let prior_selected = self.selected;
        self.row_annotation_keys = vec![None; self.rows.len()];

        let mut fallback_ordinals: HashMap<(String, String, String), usize> = HashMap::new();
        for (row_index, row) in self.rows.iter().enumerate() {
            if row.row_kind != ScopeRowKind::Entity {
                continue;
            }
            let group_key = (
                row.file_path.clone(),
                row.entity_type.clone(),
                row.entity_name.clone(),
            );
            let ordinal = fallback_ordinals.entry(group_key).or_insert(0);
            *ordinal = ordinal.saturating_add(1);
            let Some(change) = row.change.as_ref() else {
                continue;
            };
            self.row_annotation_keys[row_index] = Some(build_logical_entity_key(change, *ordinal));
        }

        self.realign_selection_after_visibility_change(prior_selected);
    }

    fn insert_annotation_char(&mut self, character: char) {
        let Some(input) = self.annotation_input.as_mut() else {
            return;
        };
        if input.text.chars().count() >= MAX_ANNOTATION_LENGTH {
            return;
        }
        input.text.insert(input.cursor_position, character);
        input.cursor_position += character.len_utf8();
    }

    fn move_annotation_cursor_left(&mut self) {
        let Some(input) = self.annotation_input.as_mut() else {
            return;
        };
        input.cursor_position = prev_char_boundary(&input.text, input.cursor_position);
    }

    fn move_annotation_cursor_right(&mut self) {
        let Some(input) = self.annotation_input.as_mut() else {
            return;
        };
        input.cursor_position = next_char_boundary(&input.text, input.cursor_position);
    }

    fn move_annotation_cursor_home(&mut self) {
        let Some(input) = self.annotation_input.as_mut() else {
            return;
        };
        input.cursor_position = 0;
    }

    fn move_annotation_cursor_end(&mut self) {
        let Some(input) = self.annotation_input.as_mut() else {
            return;
        };
        input.cursor_position = input.text.len();
    }

    fn delete_annotation_char_before_cursor(&mut self) {
        let Some(input) = self.annotation_input.as_mut() else {
            return;
        };
        if input.cursor_position == 0 {
            return;
        }
        let previous = prev_char_boundary(&input.text, input.cursor_position);
        input.text.drain(previous..input.cursor_position);
        input.cursor_position = previous;
    }

    fn delete_annotation_char_at_cursor(&mut self) {
        let Some(input) = self.annotation_input.as_mut() else {
            return;
        };
        if input.cursor_position >= input.text.len() {
            return;
        }
        let next = next_char_boundary(&input.text, input.cursor_position);
        input.text.drain(input.cursor_position..next);
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

    fn current_non_detail_mode(&self) -> Mode {
        match self.mode {
            Mode::List => Mode::List,
            Mode::Split => Mode::Split,
            Mode::Detail => Mode::List,
        }
    }

    fn apply_persisted_view_mode(&mut self, mode: Mode) {
        match mode {
            Mode::List => {
                self.mode = Mode::List;
                self.last_non_detail_mode = Mode::List;
                self.split_focus = SplitFocus::Sidebar;
                self.detail_scroll = 0;
                self.detail_hunk_index = 0;
                self.detail = None;
            }
            Mode::Split => {
                self.mode = Mode::Split;
                self.last_non_detail_mode = Mode::Split;
                self.split_focus = SplitFocus::Sidebar;
                self.detail_scroll = 0;
                self.detail_hunk_index = 0;
                self.detail = None;
                if !self.split_preview_available() {
                    self.split_focus = SplitFocus::Sidebar;
                }
            }
            Mode::Detail => {
                if self.visible_row_indices().is_empty() {
                    self.mode = Mode::List;
                    self.last_non_detail_mode = Mode::List;
                    self.split_focus = SplitFocus::Sidebar;
                    self.detail_scroll = 0;
                    self.detail_hunk_index = 0;
                    self.detail = None;
                    return;
                }
                self.mode = Mode::Detail;
                self.last_non_detail_mode = Mode::List;
                self.split_focus = SplitFocus::Sidebar;
                self.refresh_detail();
            }
        }
    }
}

fn to_persisted_view_mode(mode: Mode) -> PersistedViewMode {
    match mode {
        Mode::List => PersistedViewMode::List,
        Mode::Split => PersistedViewMode::Split,
        Mode::Detail => PersistedViewMode::Detail,
    }
}

fn from_persisted_view_mode(mode: PersistedViewMode) -> Mode {
    match mode {
        PersistedViewMode::List => Mode::List,
        PersistedViewMode::Split => Mode::Split,
        PersistedViewMode::Detail => Mode::Detail,
    }
}

fn to_persisted_diff_view(view: DiffView) -> PersistedDiffView {
    match view {
        DiffView::Unified => PersistedDiffView::Unified,
        DiffView::SideBySide => PersistedDiffView::SideBySide,
    }
}

fn from_persisted_diff_view(view: PersistedDiffView) -> DiffView {
    match view {
        PersistedDiffView::Unified => DiffView::Unified,
        PersistedDiffView::SideBySide => DiffView::SideBySide,
    }
}

fn to_persisted_entity_context_mode(mode: EntityContextMode) -> PersistedEntityContextMode {
    match mode {
        EntityContextMode::Hunk => PersistedEntityContextMode::Hunk,
        EntityContextMode::Entity => PersistedEntityContextMode::Entity,
    }
}

fn from_persisted_entity_context_mode(mode: PersistedEntityContextMode) -> EntityContextMode {
    match mode {
        PersistedEntityContextMode::Hunk => EntityContextMode::Hunk,
        PersistedEntityContextMode::Entity => EntityContextMode::Entity,
    }
}

fn to_persisted_navigation_mode(mode: RowNavigationMode) -> PersistedNavigationMode {
    match mode {
        RowNavigationMode::Mixed => PersistedNavigationMode::Mixed,
        RowNavigationMode::Entity => PersistedNavigationMode::Entity,
        RowNavigationMode::File => PersistedNavigationMode::File,
    }
}

fn from_persisted_navigation_mode(mode: PersistedNavigationMode) -> RowNavigationMode {
    match mode {
        PersistedNavigationMode::Mixed => RowNavigationMode::Mixed,
        PersistedNavigationMode::Entity => RowNavigationMode::Entity,
        PersistedNavigationMode::File => RowNavigationMode::File,
    }
}

fn file_snapshots_from_diff_result(result: &DiffResult) -> HashMap<String, TuiFileSnapshot> {
    let _ = result;
    HashMap::new()
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

fn file_line_range(start: Option<usize>, end: Option<usize>) -> Option<FileLineRange> {
    match (start, end) {
        (Some(start), Some(end)) if start > 0 && end >= start => Some(FileLineRange { start, end }),
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

fn prev_char_boundary(text: &str, index: usize) -> usize {
    text[..index]
        .char_indices()
        .last()
        .map(|(offset, _)| offset)
        .unwrap_or(0)
}

fn next_char_boundary(text: &str, index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }

    text[index..]
        .char_indices()
        .nth(1)
        .map(|(offset, _)| index + offset)
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use sem_core::model::change::{ChangeType, SemanticChange};
    use std::collections::HashMap;

    use crate::commands::diff::StepEndpointKind;

    const BASELINE_BEFORE: &str =
        "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\n";
    const BASELINE_AFTER: &str =
        "line1\nline2 changed\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11 changed\nline12\n";

    fn change(file: &str, name: &str, before: &str, after: &str) -> SemanticChange {
        change_with_identity(
            file,
            name,
            &format!("{file}::{name}"),
            ChangeType::Modified,
            Some(before),
            Some(after),
        )
    }

    fn change_with_range(
        file: &str,
        name: &str,
        entity_id: &str,
        before: Option<&str>,
        after: Option<&str>,
        before_range: Option<(usize, usize)>,
        after_range: Option<(usize, usize)>,
    ) -> SemanticChange {
        SemanticChange {
            id: format!("change::{name}"),
            entity_id: entity_id.to_string(),
            change_type: ChangeType::Modified,
            entity_type: "function".to_string(),
            entity_name: name.to_string(),
            file_path: file.to_string(),
            old_file_path: None,
            before_content: before.map(str::to_string),
            after_content: after.map(str::to_string),
            commit_sha: None,
            author: None,
            timestamp: None,
            structural_change: Some(true),
            before_start_line: before_range.map(|range| range.0),
            before_end_line: before_range.map(|range| range.1),
            after_start_line: after_range.map(|range| range.0),
            after_end_line: after_range.map(|range| range.1),
        }
    }

    fn change_with_identity(
        file: &str,
        name: &str,
        entity_id: &str,
        change_type: ChangeType,
        before: Option<&str>,
        after: Option<&str>,
    ) -> SemanticChange {
        SemanticChange {
            id: format!("change::{name}"),
            entity_id: entity_id.to_string(),
            change_type,
            entity_type: "function".to_string(),
            entity_name: name.to_string(),
            file_path: file.to_string(),
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

    fn app() -> AppState {
        let result = DiffResult {
            changes: vec![
                change("b.ts", "beta", BASELINE_BEFORE, BASELINE_AFTER),
                change("a.ts", "alpha", BASELINE_BEFORE, BASELINE_AFTER),
            ],
            file_count: 2,
            added_count: 0,
            modified_count: 2,
            deleted_count: 0,
            moved_count: 0,
            renamed_count: 0,
        };

        AppState::from_diff_result_with_snapshots(
            &result,
            file_snapshots_for_changes(&result.changes),
            DiffView::Unified,
        )
    }

    fn file_snapshots_for_changes(changes: &[SemanticChange]) -> HashMap<String, TuiFileSnapshot> {
        changes
            .iter()
            .map(|change| {
                (
                    change.file_path.clone(),
                    TuiFileSnapshot {
                        before_content: change.before_content.clone(),
                        after_content: change.after_content.clone(),
                    },
                )
            })
            .collect()
    }

    fn multi_entity_file_app() -> AppState {
        let result = DiffResult {
            changes: vec![
                change("src/file.rs", "alpha", "one\nold\n", "one\nnew\n"),
                change("src/file.rs", "beta", "tail\n", "tail\nextra\n"),
            ],
            file_count: 1,
            added_count: 1,
            modified_count: 1,
            deleted_count: 0,
            moved_count: 0,
            renamed_count: 0,
        };

        AppState::from_diff_result_with_snapshots(
            &result,
            HashMap::from([(
                "src/file.rs".to_string(),
                TuiFileSnapshot {
                    before_content: Some(BASELINE_BEFORE.to_string()),
                    after_content: Some(BASELINE_AFTER.to_string()),
                },
            )]),
            DiffView::Unified,
        )
    }

    fn residual_file_app() -> AppState {
        let before = "// header\n\n\n\n\n\n\n\n\nfn alpha() {\n  old_alpha();\n}\n\nfn beta() {\n  old_beta();\n}\n";
        let after = "// header updated\n\n\n\n\n\n\n\n\nfn alpha() {\n  new_alpha();\n}\n\nfn beta() {\n  new_beta();\n}\n";
        let result = DiffResult {
            changes: vec![
                change_with_range(
                    "src/file.rs",
                    "alpha",
                    "src/file.rs::alpha",
                    Some("fn alpha() {\n  old_alpha();\n}\n"),
                    Some("fn alpha() {\n  new_alpha();\n}\n"),
                    Some((10, 12)),
                    Some((10, 12)),
                ),
                change_with_range(
                    "src/file.rs",
                    "beta",
                    "src/file.rs::beta",
                    Some("fn beta() {\n  old_beta();\n}\n"),
                    Some("fn beta() {\n  new_beta();\n}\n"),
                    Some((14, 16)),
                    Some((14, 16)),
                ),
            ],
            file_count: 1,
            added_count: 0,
            modified_count: 2,
            deleted_count: 0,
            moved_count: 0,
            renamed_count: 0,
        };

        AppState::from_diff_result_with_snapshots(
            &result,
            HashMap::from([(
                "src/file.rs".to_string(),
                TuiFileSnapshot {
                    before_content: Some(before.to_string()),
                    after_content: Some(after.to_string()),
                },
            )]),
            DiffView::Unified,
        )
    }

    fn entity_row_indices(app: &AppState) -> Vec<usize> {
        app.rows()
            .iter()
            .enumerate()
            .filter_map(|(index, row)| (row.row_kind == ScopeRowKind::Entity).then_some(index))
            .collect()
    }

    fn first_entity_row_index(app: &AppState) -> usize {
        entity_row_indices(app)
            .into_iter()
            .next()
            .expect("expected at least one entity row")
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

    fn loaded_response(snapshot: CommitSnapshot) -> CommitStepResponse {
        CommitStepResponse {
            applied_request_id: 42,
            status: CommitLoadStatus::Loaded,
            snapshot: Some(snapshot),
            error: None,
            retain_previous_snapshot: false,
        }
    }

    fn type_annotation(app: &mut AppState, text: &str) {
        for character in text.chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
        }
    }

    #[test]
    fn app_state_sorts_rows_by_file_path() {
        let app = app();
        assert_eq!(app.rows()[0].file_path, "a.ts");
        assert_eq!(app.rows()[2].file_path, "b.ts");
    }

    #[test]
    fn app_state_moves_selection_with_j_k() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 2);
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 3);
        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 2);
    }

    #[test]
    fn f_key_cycles_navigation_mode() {
        let mut app = app();

        assert_eq!(app.navigation_mode(), RowNavigationMode::Mixed);

        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
        assert_eq!(app.navigation_mode(), RowNavigationMode::Entity);

        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
        assert_eq!(app.navigation_mode(), RowNavigationMode::File);

        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
        assert_eq!(app.navigation_mode(), RowNavigationMode::Mixed);
    }

    #[test]
    fn navigation_mode_changes_keyboard_selection_granularity() {
        let mut app = app();
        assert_eq!(app.selected(), 1);

        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 2);
        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 1);

        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
        assert_eq!(app.navigation_mode(), RowNavigationMode::Entity);
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 3);

        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
        assert_eq!(app.navigation_mode(), RowNavigationMode::File);
        assert_eq!(app.selected(), 0);
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 2);
        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 0);
    }

    #[test]
    fn list_mouse_scroll_moves_selection() {
        let mut app = app();
        assert_eq!(app.mode(), Mode::List);
        assert_eq!(app.selected(), 1);

        app.handle_list_mouse_scroll(true);
        assert_eq!(app.selected(), 2);
        app.handle_list_mouse_scroll(true);
        assert_eq!(app.selected(), 3);
        app.handle_list_mouse_scroll(false);
        assert_eq!(app.selected(), 2);
    }

    #[test]
    fn list_click_selects_visible_entity_and_ignores_out_of_bounds() {
        let mut app = app();
        assert_eq!(app.mode(), Mode::List);
        assert_eq!(app.selected(), 1);

        app.handle_list_click(1);
        assert_eq!(app.mode(), Mode::Detail);
        assert_eq!(app.selected(), 1);

        app.handle_list_click(99);
        assert_eq!(app.selected(), 1);
        assert_eq!(app.mode(), Mode::Detail);
    }

    #[test]
    fn list_click_opens_detail_for_valid_selection() {
        let mut app = app();
        assert_eq!(app.mode(), Mode::List);

        app.handle_list_click(0);
        assert_eq!(app.mode(), Mode::Detail);

        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::List);
    }

    #[test]
    fn list_mouse_handlers_noop_outside_list_mode() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);
        assert_eq!(app.selected(), 1);

        app.handle_list_mouse_scroll(true);
        app.handle_list_click(1);
        assert_eq!(app.selected(), 1);
    }

    #[test]
    fn rows_include_added_and_removed_counts() {
        let app = multi_entity_file_app();
        assert_eq!(app.rows()[1].added_lines, 1);
        assert_eq!(app.rows()[1].removed_lines, 1);
        assert_eq!(app.rows()[2].added_lines, 1);
        assert_eq!(app.rows()[2].removed_lines, 0);
        assert_eq!(app.rows()[0].added_lines, 2);
        assert_eq!(app.rows()[0].removed_lines, 1);
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
    fn v_key_cycles_modes_with_wrap() {
        let mut app = app();
        assert_eq!(app.mode(), Mode::List);

        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);

        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Detail);

        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::List);
    }

    #[test]
    fn enter_from_split_opens_detail_and_escape_returns_to_split() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Detail);

        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);
    }

    #[test]
    fn detail_v_cycle_wrap_returns_to_list() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Detail);

        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::List);
    }

    #[test]
    fn escape_from_detail_entered_via_v_cycle_returns_to_split() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Detail);

        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);
    }

    #[test]
    fn escape_in_split_mode_is_noop() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);

        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);
    }

    #[test]
    fn cycle_round_trip_list_enter_detail_v_list_v_split_is_deterministic() {
        let mut app = app();
        assert_eq!(app.mode(), Mode::List);

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Detail);

        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::List);

        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);
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
    fn startup_entity_context_mode_defaults_to_hunk() {
        let app = app();
        assert_eq!(app.entity_context_mode(), EntityContextMode::Hunk);
    }

    #[test]
    fn e_key_toggles_entity_context_mode_in_list_mode() {
        let mut app = app();
        assert_eq!(app.entity_context_mode(), EntityContextMode::Hunk);

        app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert_eq!(app.entity_context_mode(), EntityContextMode::Entity);
        assert_eq!(app.mode(), Mode::List);

        app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert_eq!(app.entity_context_mode(), EntityContextMode::Hunk);
        assert_eq!(app.mode(), Mode::List);
    }

    #[test]
    fn e_key_toggles_entity_context_mode_in_detail_and_preserves_selected_hunk() {
        let mut app = app();
        app.set_viewport(120, 12);
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        assert!(app.detail_hunk_index() > 0);
        let prior_hunk_index = app.detail_hunk_index();

        app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert_eq!(app.entity_context_mode(), EntityContextMode::Entity);
        assert_eq!(app.detail_hunk_index(), prior_hunk_index);
        assert!(app.detail_scroll() > 0);
        assert_eq!(app.mode(), Mode::Detail);
    }

    #[test]
    fn e_key_toggle_round_trip_in_detail_preserves_selected_hunk_each_time() {
        let mut app = app();
        app.set_viewport(120, 12);
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        let prior_hunk_index = app.detail_hunk_index();
        assert!(prior_hunk_index > 0);

        app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert_eq!(app.entity_context_mode(), EntityContextMode::Entity);
        assert_eq!(app.detail_hunk_index(), prior_hunk_index);
        assert!(app.detail_scroll() > 0);

        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
        assert_eq!(app.detail_hunk_index(), 0);
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        let restored_hunk_index = app.detail_hunk_index();
        assert!(restored_hunk_index > 0);

        app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert_eq!(app.entity_context_mode(), EntityContextMode::Hunk);
        assert_eq!(app.detail_hunk_index(), restored_hunk_index);
        assert!(app.detail_scroll() > 0);
    }

    #[test]
    fn e_key_toggle_in_detail_preserves_current_entity_selection() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(app.detail_title().contains("file b.ts"));

        app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert!(app.detail_title().contains("file b.ts"));
        assert_eq!(app.entity_context_mode(), EntityContextMode::Entity);
    }

    #[test]
    fn e_key_toggle_handles_unavailable_detail_content() {
        let mut app = AppState::from_diff_result(
            &DiffResult {
                changes: vec![change_with_identity(
                    "missing.ts",
                    "missing",
                    "missing.ts::missing",
                    ChangeType::Modified,
                    None,
                    None,
                )],
                file_count: 1,
                added_count: 0,
                modified_count: 1,
                deleted_count: 0,
                moved_count: 0,
                renamed_count: 0,
            },
            DiffView::Unified,
        );
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            app.unified_lines().first().map(|line| line.1.as_str()),
            Some("content unavailable")
        );

        app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert_eq!(app.entity_context_mode(), EntityContextMode::Entity);
        assert_eq!(
            app.unified_lines().first().map(|line| line.1.as_str()),
            Some("content unavailable")
        );
        assert_eq!(app.mode(), Mode::Detail);
    }

    #[test]
    fn file_scope_detail_uses_file_snapshots_for_hunk_navigation_and_full_scope_toggle() {
        let mut app = multi_entity_file_app();
        app.selected = 0;
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(app.mode(), Mode::Detail);
        assert_eq!(app.hunk_positions().len(), 2);
        let hunk_line_count = app.unified_lines().len();

        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert_eq!(app.detail_hunk_index(), 1);
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert_eq!(app.detail_hunk_index(), 1);
        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
        assert_eq!(app.detail_hunk_index(), 0);

        app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert_eq!(app.entity_context_mode(), EntityContextMode::Entity);
        assert!(app.unified_lines().len() > hunk_line_count);
    }

    #[test]
    fn file_scope_without_snapshots_shows_unavailable_placeholder() {
        let result = DiffResult {
            changes: vec![change("missing.ts", "missing", "before\n", "after\n")],
            file_count: 1,
            added_count: 0,
            modified_count: 1,
            deleted_count: 0,
            moved_count: 0,
            renamed_count: 0,
        };
        let mut app = AppState::from_diff_result_with_snapshots(
            &result,
            HashMap::new(),
            DiffView::Unified,
        );

        app.selected = 0;
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            app.unified_lines().first().map(|line| line.1.as_str()),
            Some("content unavailable")
        );
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
    fn entity_mode_navigation_uses_active_view_anchor_sets() {
        let mut app = app();
        app.set_viewport(200, 30);
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert_eq!(app.entity_context_mode(), EntityContextMode::Entity);
        assert_eq!(app.detail_hunk_index(), 0);
        assert!(app.detail_scroll() > 0);

        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert_eq!(app.detail_hunk_index(), 1);
        assert!(app.detail_scroll() > 0);
        let unified_second_anchor_scroll = app.detail_scroll();
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert_eq!(app.detail_hunk_index(), 1);
        assert_eq!(app.detail_scroll(), unified_second_anchor_scroll);

        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
        assert_eq!(app.detail_hunk_index(), 0);
        let unified_first_anchor_scroll = app.detail_scroll();
        assert!(unified_first_anchor_scroll < unified_second_anchor_scroll);
        assert!(unified_first_anchor_scroll > 0);

        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        assert_eq!(app.effective_view(), DiffView::SideBySide);
        assert_eq!(app.detail_hunk_index(), 0);
        let side_first_anchor_scroll = app.detail_scroll();
        assert!(side_first_anchor_scroll > 0);

        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert_eq!(app.detail_hunk_index(), 1);
        let side_second_anchor_scroll = app.detail_scroll();
        assert!(side_second_anchor_scroll > side_first_anchor_scroll);
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert_eq!(app.detail_hunk_index(), 1);
        assert_eq!(app.detail_scroll(), side_second_anchor_scroll);

        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
        assert_eq!(app.detail_hunk_index(), 0);
        assert_eq!(app.detail_scroll(), side_first_anchor_scroll);
    }

    #[test]
    fn entity_mode_identical_content_navigation_is_boundary_noop() {
        let content = "line1\nline2\nline3\n";
        let mut app = AppState::from_diff_result(
            &DiffResult {
                changes: vec![change_with_identity(
                    "same.ts",
                    "same",
                    "same.ts::same",
                    ChangeType::Modified,
                    Some(content),
                    Some(content),
                )],
                file_count: 1,
                added_count: 0,
                modified_count: 1,
                deleted_count: 0,
                moved_count: 0,
                renamed_count: 0,
            },
            DiffView::Unified,
        );
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert_eq!(app.entity_context_mode(), EntityContextMode::Entity);
        assert_eq!(app.detail_hunk_index(), 0);
        assert_eq!(app.detail_scroll(), 0);

        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
        assert_eq!(app.detail_hunk_index(), 0);
        assert_eq!(app.detail_scroll(), 0);
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
    fn s_toggles_requested_view_in_detail_mode() {
        let mut app = app();
        app.set_viewport(200, 40);
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.effective_view(), DiffView::Unified);
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        assert_eq!(app.effective_view(), DiffView::SideBySide);
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        assert_eq!(app.effective_view(), DiffView::Unified);
    }

    #[test]
    fn s_toggles_requested_view_in_split_mode_without_mutating_detail_cursor_state() {
        let mut app = app();
        app.set_viewport(200, 40);
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);
        app.detail_scroll = 7;
        app.detail_hunk_index = 2;
        assert_eq!(app.effective_view(), DiffView::Unified);

        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        assert_eq!(app.effective_view(), DiffView::SideBySide);
        assert_eq!(app.detail_scroll(), 7);
        assert_eq!(app.detail_hunk_index(), 2);

        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        assert_eq!(app.effective_view(), DiffView::Unified);
        assert_eq!(app.detail_scroll(), 7);
        assert_eq!(app.detail_hunk_index(), 2);
    }

    #[test]
    fn tab_toggles_split_focus_and_up_down_follow_active_pane() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);
        assert_eq!(app.selected(), 1);
        assert_eq!(app.detail_scroll(), 0);

        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.selected(), 1);
        assert!(app.detail_scroll() > 0);

        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.selected(), 1);
        assert_eq!(app.detail_scroll(), 0);

        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.selected(), 2);
        assert_eq!(app.detail_scroll(), 0);
    }

    #[test]
    fn split_mode_hunk_keys_navigate_preview_and_paging_keys_preserve_preview_state() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);

        assert_eq!(app.detail_hunk_index(), 0);
        assert_eq!(app.detail_scroll(), 0);
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        let hunk_after_next = app.detail_hunk_index();
        let scroll_after_next = app.detail_scroll();
        assert!(hunk_after_next > 0);
        assert!(scroll_after_next > 0);

        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
        assert_eq!(app.detail_hunk_index(), 0);
        assert_eq!(app.detail_scroll(), 0);

        let baseline_selected = app.selected();
        app.detail_scroll = 5;
        app.detail_hunk_index = 1;

        for key in [
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
        ] {
            app.handle_key(key);
            assert_eq!(app.mode(), Mode::Split);
            assert_eq!(app.selected(), baseline_selected);
            assert_eq!(app.detail_scroll(), 5);
            assert_eq!(app.detail_hunk_index(), 1);
        }
    }

    #[test]
    fn split_mode_left_right_navigate_scopes_when_preview_is_focused() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);
        assert_eq!(app.selected(), 1);

        app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert_eq!(app.selected(), 1);

        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(app.selected(), 2);

        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(app.selected(), 3);

        app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert_eq!(app.selected(), 2);
    }

    #[test]
    fn split_mode_g_and_g_follow_focused_pane() {
        let mut app = app();
        app.set_viewport(200, 12);
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);

        app.handle_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 3);

        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 3);
        assert!(app.detail_scroll() > 0);

        app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(app.selected(), 3);
        assert_eq!(app.detail_scroll(), 0);
    }

    #[test]
    fn split_mouse_scroll_on_sidebar_moves_selection() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);
        assert_eq!(app.selected(), 1);

        app.handle_split_mouse_scroll(false, true);
        assert_eq!(app.selected(), 2);

        app.handle_split_mouse_scroll(false, false);
        assert_eq!(app.selected(), 1);
    }

    #[test]
    fn split_sidebar_navigation_does_not_wrap_at_bounds() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);

        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.selected(), 0);
        app.handle_split_mouse_scroll(false, false);
        assert_eq!(app.selected(), 0);

        app.handle_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
        let last = app.selected();
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.selected(), last);
        app.handle_split_mouse_scroll(false, true);
        assert_eq!(app.selected(), last);
    }

    #[test]
    fn split_narrow_fallback_forces_sidebar_focus_for_navigation() {
        let mut app = app();
        app.set_viewport(200, 40);
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);

        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.selected(), 1);
        assert!(app.detail_scroll() > 0);

        app.set_viewport(70, 40);
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.selected(), 2);
    }

    #[test]
    fn split_mouse_scroll_on_preview_scrolls_preview_without_changing_selection() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);
        let baseline_selected = app.selected();
        assert_eq!(app.detail_scroll(), 0);

        app.handle_split_mouse_scroll(true, true);
        assert_eq!(app.selected(), baseline_selected);
        assert!(app.detail_scroll() > 0);

        app.handle_split_mouse_scroll(true, false);
        assert_eq!(app.selected(), baseline_selected);
        assert_eq!(app.detail_scroll(), 0);
    }

    #[test]
    fn split_mouse_scroll_noops_outside_split_mode() {
        let mut app = app();
        assert_eq!(app.mode(), Mode::List);
        assert_eq!(app.selected(), 1);
        assert_eq!(app.detail_scroll(), 0);

        app.handle_split_mouse_scroll(false, true);
        app.handle_split_mouse_scroll(true, true);
        assert_eq!(app.selected(), 1);
        assert_eq!(app.detail_scroll(), 0);
    }

    #[test]
    fn split_sidebar_click_selects_entity_and_restores_sidebar_focus() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);
        assert_eq!(app.selected(), 1);

        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert!(app.detail_scroll() > 0);
        let preview_scroll = app.detail_scroll();

        app.handle_split_sidebar_click(3);
        assert_eq!(app.selected(), 3);

        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.selected(), 3);
        assert_eq!(app.detail_scroll(), preview_scroll);
    }

    #[test]
    fn split_sidebar_click_ignores_out_of_bounds_selection() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);
        assert_eq!(app.selected(), 1);

        app.handle_split_sidebar_click(99);
        assert_eq!(app.selected(), 1);
    }

    #[test]
    fn e_key_toggles_entity_context_mode_in_split_mode() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);
        assert_eq!(app.entity_context_mode(), EntityContextMode::Hunk);

        app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert_eq!(app.entity_context_mode(), EntityContextMode::Entity);
        assert_eq!(app.mode(), Mode::Split);

        app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert_eq!(app.entity_context_mode(), EntityContextMode::Hunk);
        assert_eq!(app.mode(), Mode::Split);
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
        assert!(app.detail_title().contains("file b.ts"));

        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(app.detail_title().contains("beta"));

        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(app.detail_title().contains("file a.ts"));

        app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert!(app.detail_title().contains("beta"));
    }

    #[test]
    fn detail_navigation_respects_file_navigation_mode() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
        assert_eq!(app.navigation_mode(), RowNavigationMode::File);
        assert_eq!(app.selected(), 2);

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.detail_title().contains("file b.ts"));

        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(app.detail_title().contains("file a.ts"));

        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(app.detail_title().contains("file b.ts"));
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
        assert_eq!(app.selected(), app.visible_row_indices().len() - 1);

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
            file_snapshots: HashMap::new(),
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

        assert_eq!(app.selected(), 1);
        assert_eq!(app.rows().len(), 2);
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
                file_snapshots: HashMap::new(),
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
    fn file_row_review_toggle_applies_to_all_child_entities_and_tracks_mixed_state() {
        let mut app = multi_entity_file_app();
        let (endpoints, endpoint_index, cursor) = navigation_fixture();
        app.configure_commit_navigation(
            TuiSourceMode::Commit,
            endpoints,
            endpoint_index,
            Some(cursor),
            StepMode::Pairwise,
            None,
        );

        app.selected = 1;
        assert!(app.toggle_selected_reviewed());
        assert_eq!(app.row_review_state(0), RowReviewState::Mixed);
        assert_eq!(app.row_review_state(1), RowReviewState::Reviewed);
        assert_eq!(app.row_review_state(2), RowReviewState::Unreviewed);

        app.selected = 0;
        assert!(app.toggle_selected_reviewed());
        assert_eq!(app.row_review_state(0), RowReviewState::Reviewed);
        assert_eq!(app.row_review_state(1), RowReviewState::Reviewed);
        assert_eq!(app.row_review_state(2), RowReviewState::Reviewed);

        let snapshot = app
            .take_review_state_dirty_snapshot()
            .expect("file toggle should mark review state dirty");
        assert_eq!(snapshot.records.len(), 2);

        assert!(app.toggle_selected_reviewed());
        assert_eq!(app.row_review_state(0), RowReviewState::Unreviewed);
        assert_eq!(app.row_review_state(1), RowReviewState::Unreviewed);
        assert_eq!(app.row_review_state(2), RowReviewState::Unreviewed);
    }

    #[test]
    fn file_row_stays_visible_for_unreviewed_residual_hunks_after_all_entities_are_reviewed() {
        let mut app = residual_file_app();
        let (endpoints, endpoint_index, cursor) = navigation_fixture();
        app.configure_commit_navigation(
            TuiSourceMode::Commit,
            endpoints,
            endpoint_index,
            Some(cursor),
            StepMode::Pairwise,
            None,
        );

        app.selected = 1;
        assert!(app.toggle_selected_reviewed());
        app.selected = 2;
        assert!(app.toggle_selected_reviewed());

        app.cycle_review_filter();
        assert_eq!(app.review_filter(), ReviewFilter::Unreviewed);
        assert_eq!(app.visible_row_indices(), vec![0]);

        app.selected = 0;
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let rendered_hunk = app
            .unified_lines()
            .iter()
            .map(|(_, line)| line.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered_hunk.contains("header updated"));
        assert!(!rendered_hunk.contains("new_alpha"));
        assert!(!rendered_hunk.contains("new_beta"));

        app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        let rendered_full = app
            .unified_lines()
            .iter()
            .map(|(_, line)| line.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered_full.contains("new_alpha"));
        assert!(rendered_full.contains("new_beta"));
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

    #[test]
    fn annotation_add_flow_stores_note_and_marks_persistence_dirty() {
        let mut app = app();
        let entity_row = first_entity_row_index(&app);

        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert!(app.annotation_input_active());
        type_annotation(&mut app, "needs follow-up");
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(app.row_annotation_text(entity_row), Some("needs follow-up"));
        let snapshot = app
            .take_review_state_dirty_snapshot()
            .expect("annotation add should mark persistence dirty");
        assert_eq!(snapshot.annotations.len(), 1);
        assert_eq!(
            snapshot
                .annotations
                .values()
                .next()
                .map(|annotation| annotation.text.as_str()),
            Some("needs follow-up")
        );
    }

    #[test]
    fn annotation_replace_preserves_created_at() {
        let mut app = app();
        let entity_row = first_entity_row_index(&app);
        let logical_entity_key = app
            .row_annotation_keys
            .get(entity_row)
            .and_then(|key| key.clone())
            .expect("row should have annotation key");
        app.annotations.insert(
            logical_entity_key.clone(),
            Annotation {
                text: "first".to_string(),
                content_hash_at_creation: Some(
                    "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                        .to_string(),
                ),
                created_at: "2026-03-08T20:00:00Z".to_string(),
                updated_at: "2026-03-08T20:00:00Z".to_string(),
            },
        );

        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        app.annotation_input
            .as_mut()
            .expect("annotation input should start")
            .text = "second".to_string();
        app.annotation_input
            .as_mut()
            .expect("annotation input should remain active")
            .cursor_position = "second".len();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        let annotation = app
            .annotations
            .get(&logical_entity_key)
            .expect("annotation should exist after replace");
        assert_eq!(annotation.text, "second");
        assert_eq!(annotation.created_at, "2026-03-08T20:00:00Z");
        assert_ne!(annotation.updated_at, "2026-03-08T20:00:00Z");
    }

    #[test]
    fn whitespace_annotation_confirm_is_treated_as_cancel() {
        let mut app = app();

        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        type_annotation(&mut app, "   ");
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(app.annotations.is_empty());
        assert!(app.take_review_state_dirty_snapshot().is_none());
    }

    #[test]
    fn annotation_delete_modal_supports_escape_navigation_and_enter_confirm() {
        let mut app = app();
        let entity_row = first_entity_row_index(&app);

        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        type_annotation(&mut app, "needs follow-up");
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.row_annotation_text(entity_row), Some("needs follow-up"));
        let _ = app.take_review_state_dirty_snapshot();

        app.handle_key(KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT));
        assert!(app.annotation_delete_modal_active());
        assert_eq!(app.row_annotation_text(entity_row), Some("needs follow-up"));
        assert!(app.take_review_state_dirty_snapshot().is_none());

        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.annotation_delete_modal_active());
        assert_eq!(app.row_annotation_text(entity_row), Some("needs follow-up"));
        assert_eq!(app.status_message(), Some("annotation delete cancelled"));

        app.handle_key(KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT));
        assert_eq!(
            app.annotation_delete_modal()
                .map(|modal| modal.selected_action),
            Some(AnnotationDeleteAction::Cancel)
        );
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(
            app.annotation_delete_modal()
                .map(|modal| modal.selected_action),
            Some(AnnotationDeleteAction::Delete)
        );
        app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert_eq!(
            app.annotation_delete_modal()
                .map(|modal| modal.selected_action),
            Some(AnnotationDeleteAction::Cancel)
        );
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(
            app.annotation_delete_modal()
                .map(|modal| modal.selected_action),
            Some(AnnotationDeleteAction::Delete)
        );
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(!app.annotation_delete_modal_active());
        assert_eq!(app.row_annotation_text(entity_row), None);
        assert_eq!(app.status_message(), Some("annotation removed"));
        assert!(
            app.take_review_state_dirty_snapshot().is_some(),
            "confirmed delete should mark persistence dirty"
        );
    }

    #[test]
    fn annotation_keys_exist_without_review_hash_support() {
        let app = app();

        assert!(app
            .row_review_identities
            .iter()
            .all(|identity| identity.is_none()));
        for (row_index, row) in app.rows().iter().enumerate() {
            match row.row_kind {
                ScopeRowKind::File => assert!(app.row_annotation_keys[row_index].is_none()),
                ScopeRowKind::Entity => assert!(app.row_annotation_keys[row_index].is_some()),
            }
        }
    }

    #[test]
    fn annotation_filter_composes_with_review_filter() {
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
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        type_annotation(&mut app, "note");
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        app.handle_key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT));
        app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));

        assert_eq!(app.annotation_filter(), AnnotationFilter::Annotated);
        assert_eq!(app.review_filter(), ReviewFilter::Reviewed);
        assert!(app.visible_row_indices().is_empty());
    }

    #[test]
    fn commit_snapshot_cancels_active_annotation_input() {
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
        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        type_annotation(&mut app, "pending");
        assert!(app.annotation_input_active());

        let snapshot = CommitSnapshot {
            cursor: CommitCursor {
                endpoint_id: "working".to_string(),
                index: 2,
                rev_label: Some("WORKING".to_string()),
                sha: "working".to_string(),
                subject: "working tree".to_string(),
                has_older: true,
                has_newer: false,
            },
            result: DiffResult {
                changes: vec![change("a.ts", "alpha", BASELINE_BEFORE, BASELINE_AFTER)],
                file_count: 1,
                added_count: 0,
                modified_count: 1,
                deleted_count: 0,
                moved_count: 0,
                renamed_count: 0,
            },
            file_snapshots: HashMap::new(),
            mode: StepMode::Cumulative,
            base_endpoint_id: Some("commit:aaaaaaa".to_string()),
            comparison: StepComparison {
                from_endpoint_id: "commit:aaaaaaa".to_string(),
                to_endpoint_id: "working".to_string(),
            },
        };

        app.apply_commit_step_response(loaded_response(snapshot));
        assert!(!app.annotation_input_active());
        assert_eq!(
            app.status_message(),
            Some("annotation input cancelled: commit step applied")
        );
    }

    #[test]
    fn review_state_snapshot_includes_view_and_context_preferences() {
        let mut app = app();
        app.set_viewport(200, 40);
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));

        let snapshot = app.review_state_snapshot();
        assert_eq!(snapshot.ui_prefs.view_mode, Some(PersistedViewMode::Split));
        assert_eq!(
            snapshot.ui_prefs.diff_view,
            Some(PersistedDiffView::SideBySide)
        );
        assert_eq!(
            snapshot.ui_prefs.entity_context_mode,
            Some(PersistedEntityContextMode::Entity)
        );
        assert_eq!(
            snapshot.ui_prefs.navigation_mode,
            Some(PersistedNavigationMode::Mixed)
        );
    }

    #[test]
    fn apply_review_state_restores_view_and_context_preferences() {
        let mut app = app();
        app.set_viewport(200, 40);

        app.apply_review_state(ReviewStateData {
            filter: ReviewFilter::All,
            annotation_filter: AnnotationFilter::All,
            ui_prefs: ReviewStateUiPrefs {
                annotation_filter: Some(AnnotationFilter::All),
                view_mode: Some(PersistedViewMode::Detail),
                diff_view: Some(PersistedDiffView::SideBySide),
                entity_context_mode: Some(PersistedEntityContextMode::Entity),
                navigation_mode: Some(PersistedNavigationMode::File),
            },
            annotations: HashMap::new(),
            records: HashMap::new(),
        });

        assert_eq!(app.mode(), Mode::Detail);
        assert_eq!(app.effective_view(), DiffView::SideBySide);
        assert_eq!(app.entity_context_mode(), EntityContextMode::Entity);
        assert_eq!(app.navigation_mode(), RowNavigationMode::File);
        assert!(!app.unified_lines().is_empty());
    }

    #[test]
    fn filtered_navigation_skips_hidden_rows_in_list_mode() {
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
        app.cycle_review_filter();
        assert_eq!(app.review_filter(), ReviewFilter::Unreviewed);
        assert_eq!(app.visible_row_indices().len(), 2);
        assert_eq!(
            app.selected_row().map(|row| row.entity_name.as_str()),
            Some("beta")
        );

        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(
            app.selected_row().map(|row| row.entity_name.as_str()),
            Some("b.ts")
        );
    }

    #[test]
    fn split_mode_filter_cycle_retargets_when_selected_entity_becomes_hidden() {
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
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Split);
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(
            app.selected_row().map(|row| row.entity_name.as_str()),
            Some("b.ts")
        );

        app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        assert_eq!(app.review_filter(), ReviewFilter::Reviewed);
        assert_eq!(app.mode(), Mode::Split);
        assert_eq!(
            app.selected_row().map(|row| row.entity_name.as_str()),
            Some("a.ts")
        );
    }

    #[test]
    fn toggle_under_active_filter_can_result_in_no_match_state() {
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
        app.cycle_review_filter();
        app.cycle_review_filter();
        assert_eq!(app.review_filter(), ReviewFilter::Reviewed);
        assert_eq!(app.visible_row_indices().len(), 2);
        assert_eq!(
            app.selected_row().map(|row| row.entity_name.as_str()),
            Some("alpha")
        );

        assert!(app.toggle_selected_reviewed());
        assert!(app.visible_row_indices().is_empty());
        assert!(app.selected_row().is_none());

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::List);
    }

    #[test]
    fn detail_left_right_navigation_respects_active_filter_visibility() {
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
        app.cycle_review_filter();
        app.cycle_review_filter();
        assert_eq!(app.review_filter(), ReviewFilter::Reviewed);

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.detail_title().contains("alpha"));
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(app.detail_title().contains("file a.ts"));

        app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        assert_eq!(app.review_filter(), ReviewFilter::All);
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(app.detail_title().contains("alpha"));
    }

    #[test]
    fn detail_mode_space_toggles_reviewed_state_for_opened_entity() {
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

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.mode(), Mode::Detail);
        assert!(app.detail_title().contains("alpha"));

        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        let snapshot = app
            .take_review_state_dirty_snapshot()
            .expect("detail-mode space toggle should mark review state dirty");
        assert_eq!(snapshot.records.len(), 1);
        assert!(app.detail_title().contains("alpha"));
    }

    #[test]
    fn detail_mode_filter_cycle_retargets_when_focused_entity_becomes_hidden() {
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

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.detail_title().contains("alpha"));

        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        assert_eq!(app.review_filter(), ReviewFilter::Unreviewed);
        assert!(app.detail_title().contains("beta"));
    }

    #[test]
    fn reviewed_state_carries_across_snapshot_when_identity_and_hash_match() {
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
        let reviewed_row = first_entity_row_index(&app);
        assert!(app.is_row_reviewed(reviewed_row));

        let snapshot = CommitSnapshot {
            cursor: CommitCursor {
                endpoint_id: "working".to_string(),
                index: 2,
                rev_label: Some("WORKING".to_string()),
                sha: "working".to_string(),
                subject: "working tree".to_string(),
                has_older: true,
                has_newer: false,
            },
            result: DiffResult {
                changes: vec![change(
                    "a.ts",
                    "alpha",
                    "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\n",
                    "line1\nline2 changed\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11 changed\nline12\n",
                )],
                file_count: 1,
                added_count: 0,
                modified_count: 1,
                deleted_count: 0,
                moved_count: 0,
                renamed_count: 0,
            },
            file_snapshots: HashMap::new(),
            mode: StepMode::Cumulative,
            base_endpoint_id: Some("commit:aaaaaaa".to_string()),
            comparison: StepComparison {
                from_endpoint_id: "commit:aaaaaaa".to_string(),
                to_endpoint_id: "working".to_string(),
            },
        };

        app.apply_commit_step_response(loaded_response(snapshot));
        assert_eq!(app.step_mode(), StepMode::Cumulative);
        assert_eq!(app.rows().len(), 2);
        assert!(app.is_row_reviewed(first_entity_row_index(&app)));
    }

    #[test]
    fn reviewed_state_does_not_carry_when_target_hash_changes() {
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
        assert!(app.is_row_reviewed(first_entity_row_index(&app)));

        let snapshot = CommitSnapshot {
            cursor: CommitCursor {
                endpoint_id: "working".to_string(),
                index: 2,
                rev_label: Some("WORKING".to_string()),
                sha: "working".to_string(),
                subject: "working tree".to_string(),
                has_older: true,
                has_newer: false,
            },
            result: DiffResult {
                changes: vec![change(
                    "a.ts",
                    "alpha",
                    "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\n",
                    "line1\nline2 changed\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11 changed AGAIN\nline12\n",
                )],
                file_count: 1,
                added_count: 0,
                modified_count: 1,
                deleted_count: 0,
                moved_count: 0,
                renamed_count: 0,
            },
            file_snapshots: HashMap::new(),
            mode: StepMode::Cumulative,
            base_endpoint_id: Some("commit:aaaaaaa".to_string()),
            comparison: StepComparison {
                from_endpoint_id: "commit:aaaaaaa".to_string(),
                to_endpoint_id: "working".to_string(),
            },
        };

        app.apply_commit_step_response(loaded_response(snapshot));
        assert_eq!(app.rows().len(), 2);
        assert!(!app.is_row_reviewed(first_entity_row_index(&app)));
    }

    #[test]
    fn added_entity_carries_review_state_with_index_endpoint_target() {
        let mut app = AppState::from_diff_result(
            &DiffResult {
                changes: vec![change_with_identity(
                    "new.ts",
                    "new_fn",
                    "new.ts::new_fn",
                    ChangeType::Added,
                    None,
                    Some("fn new_fn() {\n  created();\n}\n"),
                )],
                file_count: 1,
                added_count: 1,
                modified_count: 0,
                deleted_count: 0,
                moved_count: 0,
                renamed_count: 0,
            },
            DiffView::Unified,
        );
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
        assert!(app.is_row_reviewed(first_entity_row_index(&app)));

        let snapshot = CommitSnapshot {
            cursor: CommitCursor {
                endpoint_id: "index".to_string(),
                index: 2,
                rev_label: Some("INDEX".to_string()),
                sha: "index".to_string(),
                subject: "index snapshot".to_string(),
                has_older: true,
                has_newer: false,
            },
            result: DiffResult {
                changes: vec![change_with_identity(
                    "new.ts",
                    "new_fn",
                    "new.ts::new_fn",
                    ChangeType::Added,
                    None,
                    Some("fn new_fn() {\n  created();\n}\n"),
                )],
                file_count: 1,
                added_count: 1,
                modified_count: 0,
                deleted_count: 0,
                moved_count: 0,
                renamed_count: 0,
            },
            file_snapshots: HashMap::new(),
            mode: StepMode::Cumulative,
            base_endpoint_id: Some("commit:aaaaaaa".to_string()),
            comparison: StepComparison {
                from_endpoint_id: "commit:aaaaaaa".to_string(),
                to_endpoint_id: "index".to_string(),
            },
        };

        app.apply_commit_step_response(loaded_response(snapshot));
        assert!(app.is_row_reviewed(first_entity_row_index(&app)));
    }

    #[test]
    fn deleted_entity_carries_review_state_when_before_content_matches() {
        let mut app = AppState::from_diff_result(
            &DiffResult {
                changes: vec![change_with_identity(
                    "old.ts",
                    "legacy",
                    "old.ts::legacy",
                    ChangeType::Deleted,
                    Some("fn legacy() {\n  old_logic();\n}\n"),
                    None,
                )],
                file_count: 1,
                added_count: 0,
                modified_count: 0,
                deleted_count: 1,
                moved_count: 0,
                renamed_count: 0,
            },
            DiffView::Unified,
        );
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
        assert!(app.is_row_reviewed(first_entity_row_index(&app)));

        let snapshot = CommitSnapshot {
            cursor: CommitCursor {
                endpoint_id: "working".to_string(),
                index: 2,
                rev_label: Some("WORKING".to_string()),
                sha: "working".to_string(),
                subject: "working tree".to_string(),
                has_older: true,
                has_newer: false,
            },
            result: DiffResult {
                changes: vec![change_with_identity(
                    "old.ts",
                    "legacy",
                    "old.ts::legacy",
                    ChangeType::Deleted,
                    Some("fn legacy() {\n  old_logic();\n}\n"),
                    None,
                )],
                file_count: 1,
                added_count: 0,
                modified_count: 0,
                deleted_count: 1,
                moved_count: 0,
                renamed_count: 0,
            },
            file_snapshots: HashMap::new(),
            mode: StepMode::Cumulative,
            base_endpoint_id: Some("commit:aaaaaaa".to_string()),
            comparison: StepComparison {
                from_endpoint_id: "commit:aaaaaaa".to_string(),
                to_endpoint_id: "working".to_string(),
            },
        };

        app.apply_commit_step_response(loaded_response(snapshot));
        assert!(app.is_row_reviewed(first_entity_row_index(&app)));
    }

    #[test]
    fn fallback_identity_ordinals_keep_duplicate_entities_distinct() {
        let mut app = AppState::from_diff_result(
            &DiffResult {
                changes: vec![
                    change_with_identity(
                        "dup.ts",
                        "dup",
                        "",
                        ChangeType::Modified,
                        Some("fn dup() {\n  one();\n}\n"),
                        Some("fn dup() {\n  one_changed();\n}\n"),
                    ),
                    change_with_identity(
                        "dup.ts",
                        "dup",
                        "",
                        ChangeType::Modified,
                        Some("fn dup() {\n  two();\n}\n"),
                        Some("fn dup() {\n  two_changed();\n}\n"),
                    ),
                ],
                file_count: 1,
                added_count: 0,
                modified_count: 2,
                deleted_count: 0,
                moved_count: 0,
                renamed_count: 0,
            },
            DiffView::Unified,
        );
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
        let entity_rows = entity_row_indices(&app);
        assert!(app.is_row_reviewed(entity_rows[0]));
        assert!(!app.is_row_reviewed(entity_rows[1]));
    }
}
