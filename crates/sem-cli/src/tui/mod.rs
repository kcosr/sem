mod app;
mod detail;
pub(crate) mod http_state;
mod render;
mod review_state;

use std::collections::HashMap;
use std::io;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{SecondsFormat, Utc};
use crossterm::event::{self, DisableMouseCapture, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use sem_core::parser::differ::DiffResult;

use crate::commands::diff::{
    process_commit_refresh_request, process_commit_step_request, CommitLoadStatus,
    CommitNavigationContext, CommitRefreshRequest, CommitStepRequest, CommitStepResponse, DiffView,
    StepMode, StepNavigationBootstrap, TuiSourceMode,
};
use app::PendingNavigationRequest;
use http_state::{
    build_state_snapshot, replace_shared_snapshot, shared_state, HttpSourceMode, HttpStateServer,
    SnapshotHunkInput, SnapshotReplayInput, SnapshotSelectionInput, SnapshotSessionInput,
    SnapshotUiInput,
};
use review_state::ReviewStateStoreInit;

const REVIEW_STATE_DEBOUNCE_MS: u64 = 500;

#[derive(Clone, Debug)]
pub struct TuiRuntimeOptions {
    pub http_enabled: bool,
    pub http_port: u16,
    pub source_mode: HttpSourceMode,
}

pub fn run_tui(
    result: &DiffResult,
    initial_view: DiffView,
    navigation_bootstrap: Option<StepNavigationBootstrap>,
    runtime_options: TuiRuntimeOptions,
) -> io::Result<()> {
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    enable_raw_mode()?;

    let guard = TerminalGuard;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app_state = app::AppState::from_diff_result(result, initial_view);
    render::prewarm_syntax_highlighting_async();
    let (context, source_mode, cursor, mode, base_endpoint_id) =
        if let Some(bootstrap) = navigation_bootstrap {
            let source_mode = bootstrap.context.source_mode;
            (
                bootstrap.context,
                source_mode,
                Some(bootstrap.cursor),
                bootstrap.mode,
                bootstrap.base_endpoint_id,
            )
        } else {
            (
                CommitNavigationContext {
                    cwd: String::new(),
                    file_exts: vec![],
                    source_mode: TuiSourceMode::Unsupported,
                    endpoints: vec![],
                    endpoint_index: HashMap::new(),
                },
                TuiSourceMode::Unsupported,
                None,
                StepMode::Pairwise,
                None,
            )
        };
    app_state.configure_commit_navigation(
        source_mode,
        context.endpoints.clone(),
        context.endpoint_index.clone(),
        cursor,
        mode,
        base_endpoint_id,
    );
    let started_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let initial_session = SnapshotSessionInput {
        http_enabled: runtime_options.http_enabled,
        http_bound: false,
        source_mode: runtime_options.source_mode,
        started_at: started_at.clone(),
    };
    let initial_snapshot = snapshot_for_http_state(&app_state, &initial_session);
    let shared_http_state = shared_state(initial_snapshot);
    let mut http_server = HttpStateServer::start(
        runtime_options.http_enabled,
        runtime_options.http_port,
        shared_http_state.clone(),
    );
    let session = SnapshotSessionInput {
        http_enabled: http_server.enabled(),
        http_bound: http_server.bound(),
        source_mode: runtime_options.source_mode,
        started_at,
    };
    if runtime_options.http_enabled && !http_server.bound() {
        if let Some(error) = http_server.bind_error() {
            app_state.set_review_status_message(Some(format!(
                "tui-http unavailable on localhost: {error}"
            )));
        }
    }
    sync_http_state(&app_state, &session, &shared_http_state);
    let review_cwd = context.cwd.clone();
    let mut reload_coordinator = ReloadCoordinator::new(context);
    let review_store = match review_state::ReviewStateStore::initialize(&review_cwd) {
        ReviewStateStoreInit::Available(store) => Some(store),
        ReviewStateStoreInit::Unavailable(_) => None,
    };
    let mut pending_review_save = None;
    let mut review_save_deadline = None;

    if let Some(store) = review_store.as_ref() {
        let load_result = store.load();
        app_state.apply_review_state(load_result.state);
        if let Some(warning) = load_result.warning {
            app_state.set_review_status_message(Some(warning));
        }
        if load_result.compacted {
            app_state.mark_review_state_dirty();
        }
    }

    if let Ok(size) = terminal.size() {
        app_state.set_viewport(size.width, size.height);
    }

    while !app_state.should_quit() {
        terminal.draw(|frame| {
            app_state.set_viewport(frame.area().width, frame.area().height);
            render::draw(frame, &app_state);
        })?;

        if event::poll(Duration::from_millis(200))? {
            match event::read()? {
                Event::Key(key) => app_state.handle_key(key),
                Event::Resize(width, height) => app_state.set_viewport(width, height),
                _ => {}
            }
        }

        if let Some(request_kind) = app_state.take_pending_navigation_request() {
            let request_id = reload_coordinator.next_request_id();
            let current_endpoint_id = app_state
                .commit_cursor()
                .map(|cursor| cursor.endpoint_id.clone())
                .unwrap_or_default();
            let current_index = app_state
                .commit_cursor()
                .map(|cursor| cursor.index)
                .unwrap_or(0);
            let source_mode = app_state.commit_source_mode();
            let mode = app_state.step_mode();
            let base_endpoint_id = app_state.cumulative_base_endpoint_id();
            match request_kind {
                PendingNavigationRequest::Step(action) => {
                    reload_coordinator.queue_request(WorkerRequest::Step(CommitStepRequest {
                        request_id,
                        action,
                        current_endpoint_id,
                        current_index,
                        source_mode,
                        mode,
                        base_endpoint_id,
                    }));
                }
                PendingNavigationRequest::Refresh => {
                    reload_coordinator.queue_request(WorkerRequest::Refresh(
                        CommitRefreshRequest {
                            request_id,
                            current_endpoint_id,
                            current_index,
                            source_mode,
                            mode,
                            base_endpoint_id,
                        },
                    ));
                }
            }
            app_state.set_commit_loading(true);
        }

        while let Some(response) = reload_coordinator.try_recv_response() {
            app_state.apply_commit_step_response(response);
        }
        app_state.set_commit_loading(reload_coordinator.has_active_request());

        if let Some(snapshot) = app_state.take_review_state_dirty_snapshot() {
            pending_review_save = Some(snapshot);
            review_save_deadline =
                Some(Instant::now() + Duration::from_millis(REVIEW_STATE_DEBOUNCE_MS));
        }

        if let (Some(store), Some(snapshot), Some(deadline)) = (
            review_store.as_ref(),
            pending_review_save.as_ref(),
            review_save_deadline,
        ) {
            if Instant::now() >= deadline {
                if let Err(error) = store.save(snapshot) {
                    app_state.set_review_status_message(Some(format!(
                        "review persistence write failed: {error}"
                    )));
                }
                pending_review_save = None;
                review_save_deadline = None;
            }
        }

        sync_http_state(&app_state, &session, &shared_http_state);
    }

    if let Some(snapshot) = app_state.take_review_state_dirty_snapshot() {
        pending_review_save = Some(snapshot);
    }
    if let (Some(store), Some(snapshot)) = (review_store.as_ref(), pending_review_save.as_ref()) {
        if let Err(error) = store.save(snapshot) {
            app_state.set_review_status_message(Some(format!(
                "review persistence write failed: {error}"
            )));
        }
    }

    http_server.shutdown();
    drop(guard);
    terminal.show_cursor()?;
    Ok(())
}

fn snapshot_for_http_state(
    app_state: &app::AppState,
    session: &SnapshotSessionInput,
) -> http_state::HttpStateSnapshot {
    let selection = snapshot_inputs_from_app_state(app_state);
    let replay = snapshot_replay_input(app_state);
    build_state_snapshot(session, selection, replay)
}

fn sync_http_state(
    app_state: &app::AppState,
    session: &SnapshotSessionInput,
    shared_http_state: &http_state::SharedHttpState,
) {
    let snapshot = snapshot_for_http_state(app_state, session);
    replace_shared_snapshot(shared_http_state, snapshot);
}

fn snapshot_inputs_from_app_state(app_state: &app::AppState) -> SnapshotSelectionInput {
    let anchors = app_state.detail_anchor_state();
    let ui = SnapshotUiInput {
        mode: mode_token(app_state.mode()).to_string(),
        view: view_token(app_state.effective_view()).to_string(),
        context_mode: app_state.entity_context_mode().as_token().to_string(),
        hunk_index: app_state.detail_hunk_index(),
        scroll: app_state.detail_scroll(),
        anchors,
    };

    let Some(row) = app_state.selected_row() else {
        return SnapshotSelectionInput {
            selected: false,
            file: None,
            entity_type: None,
            entity_name: None,
            line_range: None,
            hunk: None,
            ui,
        };
    };

    let line_range = row_line_range(row);
    let hunk = selected_hunk_snapshot(app_state, anchors);
    SnapshotSelectionInput {
        selected: true,
        file: Some(row.file_path.clone()),
        entity_type: Some(row.entity_type.clone()),
        entity_name: Some(row.entity_name.clone()),
        line_range,
        hunk,
        ui,
    }
}

fn snapshot_replay_input(app_state: &app::AppState) -> SnapshotReplayInput {
    if let Some((_, from, _, to)) = app_state.comparison_line() {
        let git_command = git_replay_command(&from, &to);
        let (available, reason) = if git_command.is_some() {
            (true, None)
        } else {
            (false, Some("unsupportedEndpointPair".to_string()))
        };
        return SnapshotReplayInput {
            available,
            git_command,
            from: Some(from),
            to: Some(to),
            reason,
        };
    }

    let reason = match app_state.commit_source_mode() {
        TuiSourceMode::Unsupported => "unsupportedSourceMode",
        _ => "navigationStateUnavailable",
    };
    SnapshotReplayInput {
        available: false,
        git_command: None,
        from: None,
        to: None,
        reason: Some(reason.to_string()),
    }
}

fn git_replay_command(from: &str, to: &str) -> Option<String> {
    let from_is_pseudo = from == "INDEX" || from == "WORKING";
    let to_is_pseudo = to == "INDEX" || to == "WORKING";

    match (from, to) {
        ("INDEX", "WORKING") => Some("git diff".to_string()),
        (from, "WORKING") if !from_is_pseudo => Some(format!("git diff {from}")),
        (from, "INDEX") if !from_is_pseudo => Some(format!("git diff --cached {from}")),
        (from, to) if !from_is_pseudo && !to_is_pseudo => Some(format!("git diff {from}..{to}")),
        _ => None,
    }
}

fn row_line_range(row: &app::EntityRow) -> Option<[usize; 2]> {
    match (
        row.change.after_start_line,
        row.change.after_end_line,
        row.change.before_start_line,
        row.change.before_end_line,
    ) {
        (Some(start), Some(end), _, _) => Some([start.min(end), start.max(end)]),
        (_, _, Some(start), Some(end)) => Some([start.min(end), start.max(end)]),
        _ => None,
    }
}

fn selected_hunk_snapshot(
    app_state: &app::AppState,
    anchors: [usize; 2],
) -> Option<SnapshotHunkInput> {
    let [index, total] = anchors;
    if index == 0 || total == 0 {
        return None;
    }

    let header = app_state.selected_hunk_header()?.to_string();
    let (old_start, old_count, new_start, new_count) = parse_hunk_header(&header)?;
    Some(SnapshotHunkInput {
        index,
        total,
        header,
        old_start,
        old_count,
        new_start,
        new_count,
    })
}

fn parse_hunk_header(header: &str) -> Option<(usize, usize, usize, usize)> {
    let mut parts = header.split_whitespace();
    if parts.next()? != "@@" {
        return None;
    }
    let old = parse_hunk_range(parts.next()?)?;
    let new = parse_hunk_range(parts.next()?)?;
    if parts.next()? != "@@" {
        return None;
    }
    Some((old.0, old.1, new.0, new.1))
}

fn parse_hunk_range(token: &str) -> Option<(usize, usize)> {
    let range = token
        .strip_prefix('-')
        .or_else(|| token.strip_prefix('+'))?;
    let mut parts = range.split(',');
    let start = parts.next()?.parse::<usize>().ok()?;
    let count = parts.next()?.parse::<usize>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((start, count))
}

fn mode_token(mode: app::Mode) -> &'static str {
    match mode {
        app::Mode::List => "list",
        app::Mode::Detail => "detail",
    }
}

fn view_token(view: DiffView) -> &'static str {
    match view {
        DiffView::Unified => "unified",
        DiffView::SideBySide => "sideBySide",
    }
}

struct ReloadCoordinator {
    request_tx: Sender<WorkerRequest>,
    response_rx: Receiver<CommitStepResponse>,
    in_flight_request_id: Option<u64>,
    pending_request: Option<WorkerRequest>,
    latest_requested_id: u64,
}

#[derive(Clone, Debug)]
enum WorkerRequest {
    Step(CommitStepRequest),
    Refresh(CommitRefreshRequest),
}

impl ReloadCoordinator {
    fn new(context: CommitNavigationContext) -> Self {
        let (request_tx, request_rx) = mpsc::channel::<WorkerRequest>();
        let (response_tx, response_rx) = mpsc::channel::<CommitStepResponse>();

        thread::spawn(move || {
            while let Ok(request) = request_rx.recv() {
                let response = match request {
                    WorkerRequest::Step(request) => process_commit_step_request(&context, &request),
                    WorkerRequest::Refresh(request) => {
                        process_commit_refresh_request(&context, &request)
                    }
                };
                if response_tx.send(response).is_err() {
                    break;
                }
            }
        });

        Self {
            request_tx,
            response_rx,
            in_flight_request_id: None,
            pending_request: None,
            latest_requested_id: 0,
        }
    }

    fn next_request_id(&mut self) -> u64 {
        self.latest_requested_id = self.latest_requested_id.saturating_add(1);
        self.latest_requested_id
    }

    fn queue_request(&mut self, request: WorkerRequest) {
        let request_id = match &request {
            WorkerRequest::Step(request) => request.request_id,
            WorkerRequest::Refresh(request) => request.request_id,
        };
        self.latest_requested_id = self.latest_requested_id.max(request_id);
        if self.in_flight_request_id.is_none() {
            if self.request_tx.send(request.clone()).is_ok() {
                self.in_flight_request_id = Some(request_id);
            }
            return;
        }

        self.pending_request = Some(request);
    }

    fn try_recv_response(&mut self) -> Option<CommitStepResponse> {
        let response = match self.response_rx.try_recv() {
            Ok(response) => response,
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => return None,
        };

        self.in_flight_request_id = None;
        if let Some(next_request) = self.pending_request.take() {
            let next_request_id = match &next_request {
                WorkerRequest::Step(request) => request.request_id,
                WorkerRequest::Refresh(request) => request.request_id,
            };
            if self.request_tx.send(next_request.clone()).is_ok() {
                self.in_flight_request_id = Some(next_request_id);
            }
        }

        if response.applied_request_id < self.latest_requested_id {
            return Some(CommitStepResponse {
                applied_request_id: response.applied_request_id,
                status: CommitLoadStatus::IgnoredStaleResult,
                snapshot: None,
                error: None,
                retain_previous_snapshot: true,
            });
        }

        Some(response)
    }

    fn has_active_request(&self) -> bool {
        self.in_flight_request_id.is_some() || self.pending_request.is_some()
    }
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
    }
}

#[cfg(test)]
mod tests {
    use super::http_state::{HttpSourceMode, SnapshotSessionInput};
    use super::{snapshot_for_http_state, ReloadCoordinator, WorkerRequest};
    use crate::commands::diff::{
        CommitCursor, CommitLoadStatus, CommitNavigationContext, CommitStepAction,
        CommitStepRequest, DiffView, StepEndpoint, StepEndpointKind, StepMode, TuiSourceMode,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use sem_core::model::change::{ChangeType, SemanticChange};
    use sem_core::parser::differ::DiffResult;
    use std::collections::HashMap;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn reload_coordinator_drops_stale_results_and_keeps_latest_request() {
        let mut coordinator = ReloadCoordinator::new(CommitNavigationContext {
            cwd: String::new(),
            file_exts: vec![],
            source_mode: TuiSourceMode::Unsupported,
            endpoints: vec![],
            endpoint_index: HashMap::new(),
        });

        let first_request_id = coordinator.next_request_id();
        coordinator.queue_request(WorkerRequest::Step(CommitStepRequest {
            request_id: first_request_id,
            action: CommitStepAction::Older,
            current_endpoint_id: String::new(),
            current_index: 0,
            source_mode: TuiSourceMode::Unsupported,
            mode: StepMode::Pairwise,
            base_endpoint_id: None,
        }));
        let second_request_id = coordinator.next_request_id();
        coordinator.queue_request(WorkerRequest::Step(CommitStepRequest {
            request_id: second_request_id,
            action: CommitStepAction::Newer,
            current_endpoint_id: String::new(),
            current_index: 0,
            source_mode: TuiSourceMode::Unsupported,
            mode: StepMode::Pairwise,
            base_endpoint_id: None,
        }));

        let first = wait_for_response(&mut coordinator);
        assert_eq!(first.status, CommitLoadStatus::IgnoredStaleResult);

        let second = wait_for_response(&mut coordinator);
        assert_eq!(second.applied_request_id, 2);
        assert_eq!(second.status, CommitLoadStatus::UnsupportedMode);
    }

    #[test]
    fn reload_coordinator_coalesces_rapid_step_sequence_to_latest_pending_request() {
        let mut coordinator = ReloadCoordinator::new(CommitNavigationContext {
            cwd: String::new(),
            file_exts: vec![],
            source_mode: TuiSourceMode::Unsupported,
            endpoints: vec![],
            endpoint_index: HashMap::new(),
        });

        let sequence = [
            CommitStepAction::Older,
            CommitStepAction::Older,
            CommitStepAction::Older,
            CommitStepAction::Newer,
            CommitStepAction::Newer,
        ];
        for action in sequence {
            let request_id = coordinator.next_request_id();
            coordinator.queue_request(WorkerRequest::Step(CommitStepRequest {
                request_id,
                action,
                current_endpoint_id: String::new(),
                current_index: 0,
                source_mode: TuiSourceMode::Unsupported,
                mode: StepMode::Pairwise,
                base_endpoint_id: None,
            }));
        }

        let first = wait_for_response(&mut coordinator);
        assert_eq!(first.applied_request_id, 1);
        assert_eq!(first.status, CommitLoadStatus::IgnoredStaleResult);

        let second = wait_for_response(&mut coordinator);
        assert_eq!(second.applied_request_id, 5);
        assert_eq!(second.status, CommitLoadStatus::UnsupportedMode);
    }

    #[test]
    fn snapshot_tracks_selection_mode_across_list_detail_transitions() {
        let mut app = super::app::AppState::from_diff_result(&sample_result(), DiffView::Unified);
        let session = SnapshotSessionInput {
            http_enabled: true,
            http_bound: true,
            source_mode: HttpSourceMode::Stdin,
            started_at: "2026-03-08T21:00:00Z".to_string(),
        };

        let list_snapshot = snapshot_for_http_state(&app, &session);
        assert_eq!(list_snapshot.selection.ui.mode, "list");

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let detail_snapshot = snapshot_for_http_state(&app, &session);
        assert_eq!(detail_snapshot.selection.ui.mode, "detail");

        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        let list_after_close = snapshot_for_http_state(&app, &session);
        assert_eq!(list_after_close.selection.ui.mode, "list");
    }

    #[test]
    fn snapshot_includes_replay_command_when_commit_comparison_is_available() {
        let mut app = super::app::AppState::from_diff_result(&sample_result(), DiffView::Unified);
        let endpoints = vec![
            StepEndpoint {
                endpoint_id: "commit:aaa".to_string(),
                display_ref: Some("HEAD~1".to_string()),
                kind: StepEndpointKind::Commit {
                    sha: "aaa".to_string(),
                },
            },
            StepEndpoint {
                endpoint_id: "commit:bbb".to_string(),
                display_ref: Some("HEAD".to_string()),
                kind: StepEndpointKind::Commit {
                    sha: "bbb".to_string(),
                },
            },
        ];
        let endpoint_index = HashMap::from([
            ("commit:aaa".to_string(), 0usize),
            ("commit:bbb".to_string(), 1usize),
        ]);
        app.configure_commit_navigation(
            TuiSourceMode::Unified,
            endpoints,
            endpoint_index,
            Some(CommitCursor {
                endpoint_id: "commit:bbb".to_string(),
                index: 1,
                rev_label: Some("HEAD".to_string()),
                sha: "bbb".to_string(),
                subject: "tip".to_string(),
                has_older: true,
                has_newer: false,
            }),
            StepMode::Pairwise,
            None,
        );

        let session = SnapshotSessionInput {
            http_enabled: true,
            http_bound: true,
            source_mode: HttpSourceMode::Repository,
            started_at: "2026-03-08T21:00:00Z".to_string(),
        };
        let snapshot = snapshot_for_http_state(&app, &session);

        assert!(snapshot.replay.available);
        assert_eq!(
            snapshot.replay.git_command.as_deref(),
            Some("git diff HEAD~1..HEAD")
        );
        assert_eq!(snapshot.replay.from.as_deref(), Some("HEAD~1"));
        assert_eq!(snapshot.replay.to.as_deref(), Some("HEAD"));
        assert_eq!(snapshot.replay.reason, None);
    }

    #[test]
    fn snapshot_includes_selected_hunk_range_details() {
        let mut app = super::app::AppState::from_diff_result(&sample_result(), DiffView::Unified);
        let session = SnapshotSessionInput {
            http_enabled: true,
            http_bound: true,
            source_mode: HttpSourceMode::Repository,
            started_at: "2026-03-08T21:00:00Z".to_string(),
        };

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let snapshot = snapshot_for_http_state(&app, &session);
        let hunk = snapshot
            .selection
            .hunk
            .expect("selected hunk details should be present in detail mode");

        assert_eq!(hunk.index, 1);
        assert_eq!(hunk.total, 1);
        assert_eq!(hunk.header, "@@ -1,3 +1,3 @@");
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.old_count, 3);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.new_count, 3);
    }

    fn wait_for_response(
        coordinator: &mut ReloadCoordinator,
    ) -> crate::commands::diff::CommitStepResponse {
        for _ in 0..200 {
            if let Some(response) = coordinator.try_recv_response() {
                return response;
            }
            thread::sleep(Duration::from_millis(5));
        }

        panic!("timed out waiting for coordinator response");
    }

    fn sample_result() -> DiffResult {
        DiffResult {
            changes: vec![SemanticChange {
                id: "change::x".to_string(),
                entity_id: "src/x.rs::function::x".to_string(),
                change_type: ChangeType::Modified,
                entity_type: "function".to_string(),
                entity_name: "x".to_string(),
                file_path: "src/x.rs".to_string(),
                old_file_path: None,
                before_content: Some("line1\nline2\nline3\n".to_string()),
                after_content: Some("line1\nline2 changed\nline3\n".to_string()),
                commit_sha: None,
                author: None,
                timestamp: None,
                structural_change: Some(true),
                before_start_line: Some(1),
                before_end_line: Some(3),
                after_start_line: Some(1),
                after_end_line: Some(3),
            }],
            file_count: 1,
            added_count: 0,
            modified_count: 1,
            deleted_count: 0,
            moved_count: 0,
            renamed_count: 0,
        }
    }
}
