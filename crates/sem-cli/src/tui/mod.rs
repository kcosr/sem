mod app;
mod detail;
mod render;

use std::collections::HashMap;
use std::io;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

use crossterm::event::{self, DisableMouseCapture, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use sem_core::parser::differ::DiffResult;

use crate::commands::diff::{
    process_commit_step_request, CommitCursor, CommitLoadStatus, CommitNavigationContext,
    CommitStepRequest, CommitStepResponse, DiffView, TuiSourceMode,
};

pub fn run_tui(
    result: &DiffResult,
    initial_view: DiffView,
    commit_navigation: Option<(CommitNavigationContext, CommitCursor)>,
) -> io::Result<()> {
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    enable_raw_mode()?;

    let guard = TerminalGuard;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app_state = app::AppState::from_diff_result(result, initial_view);
    render::prewarm_syntax_highlighting_async();
    let (context, source_mode, cursor) = if let Some((context, cursor)) = commit_navigation {
        (context, TuiSourceMode::Commit, Some(cursor))
    } else {
        (
            CommitNavigationContext {
                cwd: String::new(),
                file_exts: vec![],
                source_mode: TuiSourceMode::Unsupported,
                lineage: vec![],
                lineage_index: HashMap::new(),
            },
            TuiSourceMode::Unsupported,
            None,
        )
    };
    app_state.configure_commit_navigation(source_mode, cursor);
    let mut reload_coordinator = ReloadCoordinator::new(context);

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

        if let Some(action) = app_state.take_pending_commit_action() {
            let request = CommitStepRequest {
                request_id: reload_coordinator.next_request_id(),
                action,
                current_sha: app_state
                    .commit_cursor()
                    .map(|cursor| cursor.sha.clone())
                    .unwrap_or_default(),
                source_mode: app_state.commit_source_mode(),
            };
            reload_coordinator.queue_request(request);
            app_state.set_commit_loading(true);
        }

        while let Some(response) = reload_coordinator.try_recv_response() {
            app_state.apply_commit_step_response(response);
        }
        app_state.set_commit_loading(reload_coordinator.has_active_request());
    }

    drop(guard);
    terminal.show_cursor()?;
    Ok(())
}

struct ReloadCoordinator {
    request_tx: Sender<CommitStepRequest>,
    response_rx: Receiver<CommitStepResponse>,
    in_flight_request_id: Option<u64>,
    pending_request: Option<CommitStepRequest>,
    latest_requested_id: u64,
}

impl ReloadCoordinator {
    fn new(context: CommitNavigationContext) -> Self {
        let (request_tx, request_rx) = mpsc::channel::<CommitStepRequest>();
        let (response_tx, response_rx) = mpsc::channel::<CommitStepResponse>();

        thread::spawn(move || {
            while let Ok(request) = request_rx.recv() {
                let response = process_commit_step_request(&context, &request);
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

    fn queue_request(&mut self, request: CommitStepRequest) {
        self.latest_requested_id = self.latest_requested_id.max(request.request_id);
        if self.in_flight_request_id.is_none() {
            if self.request_tx.send(request.clone()).is_ok() {
                self.in_flight_request_id = Some(request.request_id);
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
            if self.request_tx.send(next_request.clone()).is_ok() {
                self.in_flight_request_id = Some(next_request.request_id);
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
    use super::ReloadCoordinator;
    use crate::commands::diff::{
        CommitLoadStatus, CommitNavigationContext, CommitStepAction, CommitStepRequest,
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
            lineage: vec![],
            lineage_index: HashMap::new(),
        });

        let first_request_id = coordinator.next_request_id();
        coordinator.queue_request(CommitStepRequest {
            request_id: first_request_id,
            action: CommitStepAction::Older,
            current_sha: String::new(),
            source_mode: TuiSourceMode::Unsupported,
        });
        let second_request_id = coordinator.next_request_id();
        coordinator.queue_request(CommitStepRequest {
            request_id: second_request_id,
            action: CommitStepAction::Newer,
            current_sha: String::new(),
            source_mode: TuiSourceMode::Unsupported,
        });

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
            lineage: vec![],
            lineage_index: HashMap::new(),
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
            coordinator.queue_request(CommitStepRequest {
                request_id,
                action,
                current_sha: String::new(),
                source_mode: TuiSourceMode::Unsupported,
            });
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
