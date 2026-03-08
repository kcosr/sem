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
    build_state_snapshot, replace_shared_snapshot, shared_state, GraphSelection,
    GraphSnapshotService, HttpSourceMode, HttpStateServer, SnapshotSelectionInput,
    SnapshotSessionInput, SnapshotUiInput,
};
use review_state::ReviewStateStoreInit;

const REVIEW_STATE_DEBOUNCE_MS: u64 = 500;

#[derive(Clone, Debug)]
pub struct TuiRuntimeOptions {
    pub http_enabled: bool,
    pub http_port: u16,
    pub source_mode: HttpSourceMode,
    pub cwd: String,
    pub file_exts: Vec<String>,
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
    let graph_snapshot_service = GraphSnapshotService::new(
        &runtime_options.cwd,
        &runtime_options.file_exts,
        runtime_options.source_mode,
    );
    let started_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let initial_session = SnapshotSessionInput {
        http_enabled: runtime_options.http_enabled,
        http_bound: false,
        host: "127.0.0.1".to_string(),
        port: runtime_options.http_port,
        source_mode: runtime_options.source_mode,
        started_at: started_at.clone(),
    };
    let initial_snapshot =
        build_snapshot(&app_state, &graph_snapshot_service, &initial_session, false);
    let shared_http_state = shared_state(initial_snapshot);
    let mut http_server = HttpStateServer::start(
        runtime_options.http_enabled,
        runtime_options.http_port,
        shared_http_state.clone(),
    );
    let session = SnapshotSessionInput {
        http_enabled: http_server.enabled(),
        http_bound: http_server.bound(),
        host: http_server.host().to_string(),
        port: http_server.port(),
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
    replace_shared_snapshot(
        &shared_http_state,
        build_snapshot(&app_state, &graph_snapshot_service, &session, false),
    );
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

        replace_shared_snapshot(
            &shared_http_state,
            build_snapshot(&app_state, &graph_snapshot_service, &session, false),
        );
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

fn build_snapshot(
    app_state: &app::AppState,
    graph_snapshot_service: &GraphSnapshotService,
    session: &SnapshotSessionInput,
    panel_expanded: bool,
) -> http_state::HttpStateSnapshot {
    let (selection, graph_selection) = snapshot_inputs_from_app_state(app_state);
    let graph_impact = graph_snapshot_service.snapshot_for_selection(graph_selection.as_ref());
    build_state_snapshot(session, selection, &graph_impact, panel_expanded)
}

fn snapshot_inputs_from_app_state(
    app_state: &app::AppState,
) -> (SnapshotSelectionInput, Option<GraphSelection>) {
    let ui = SnapshotUiInput {
        mode: mode_token(app_state.mode()).to_string(),
        view: view_token(app_state.effective_view()).to_string(),
        context_mode: app_state.entity_context_mode().as_token().to_string(),
        hunk_index: app_state.detail_hunk_index(),
        scroll: app_state.detail_scroll(),
        anchors: app_state.detail_anchor_state(),
    };

    let Some(row) = app_state.selected_row() else {
        return (
            SnapshotSelectionInput {
                selected: false,
                file: None,
                entity_type: None,
                entity_name: None,
                line_range: None,
                ui,
            },
            None,
        );
    };

    let line_range = row_line_range(row);
    let graph_id = (!row.change.entity_id.is_empty()).then(|| row.change.entity_id.clone());

    (
        SnapshotSelectionInput {
            selected: true,
            file: Some(row.file_path.clone()),
            entity_type: Some(row.entity_type.clone()),
            entity_name: Some(row.entity_name.clone()),
            line_range,
            ui,
        },
        Some(GraphSelection {
            graph_id,
            file: row.file_path.clone(),
            entity_type: row.entity_type.clone(),
            entity_name: row.entity_name.clone(),
            line_range,
        }),
    )
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
    use super::{ReloadCoordinator, WorkerRequest};
    use crate::commands::diff::{
        CommitLoadStatus, CommitNavigationContext, CommitStepAction, CommitStepRequest, StepMode,
        TuiSourceMode,
    };
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
}
