use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use sem_core::parser::graph::{EntityGraph, EntityInfo};
use sem_core::parser::plugins::create_default_registry;
use serde::Serialize;

use crate::commands::graph::{find_supported_files_public, normalize_exts};

pub const IMPACT_RESPONSE_CAP_DEFAULT: usize = 10_000;
pub const PANEL_DISPLAY_CAP_DEFAULT: usize = 25;
const LOCALHOST: &str = "127.0.0.1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HttpSourceMode {
    Repository,
    Stdin,
    TwoFile,
}

impl HttpSourceMode {
    pub fn as_token(self) -> &'static str {
        match self {
            Self::Repository => "repository",
            Self::Stdin => "stdin",
            Self::TwoFile => "twoFile",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GraphUnavailableReason {
    UnsupportedSourceMode,
    GraphBuildFailed,
    SelectionNotResolvable,
}

impl GraphUnavailableReason {
    pub fn as_token(self) -> &'static str {
        match self {
            Self::UnsupportedSourceMode => "unsupportedSourceMode",
            Self::GraphBuildFailed => "graphBuildFailed",
            Self::SelectionNotResolvable => "selectionNotResolvable",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GraphEntityRef {
    pub id: String,
    pub name: String,
    pub file: String,
    pub lines: [usize; 2],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphSelection {
    pub graph_id: Option<String>,
    pub file: String,
    pub entity_type: String,
    pub entity_name: String,
    pub line_range: Option<[usize; 2]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphImpactSnapshot {
    pub graph_available: bool,
    pub reason: Option<GraphUnavailableReason>,
    pub dependencies: Vec<GraphEntityRef>,
    pub dependents: Vec<GraphEntityRef>,
    pub impact_total: usize,
    pub impact_cap: usize,
    pub impact_truncated: bool,
    pub impact_entities: Vec<GraphEntityRef>,
}

impl GraphImpactSnapshot {
    pub fn unavailable(reason: GraphUnavailableReason, impact_cap: usize) -> Self {
        Self {
            graph_available: false,
            reason: Some(reason),
            dependencies: vec![],
            dependents: vec![],
            impact_total: 0,
            impact_cap,
            impact_truncated: false,
            impact_entities: vec![],
        }
    }

    pub fn panel_summary(&self) -> String {
        format!(
            "deps:{} depBy:{} impact:{}",
            self.dependencies.len(),
            self.dependents.len(),
            self.impact_total
        )
    }

    pub fn panel_rows(
        entities: &[GraphEntityRef],
        panel_cap: usize,
    ) -> (Vec<GraphEntityRef>, usize) {
        let visible = entities.iter().take(panel_cap).cloned().collect();
        let hidden = entities.len().saturating_sub(panel_cap);
        (visible, hidden)
    }
}

enum GraphSnapshotState {
    Available(EntityGraph),
    Unavailable(GraphUnavailableReason),
}

pub struct GraphSnapshotService {
    state: GraphSnapshotState,
    impact_cap: usize,
}

impl GraphSnapshotService {
    pub fn new(cwd: &str, file_exts: &[String], source_mode: HttpSourceMode) -> Self {
        Self::with_caps(cwd, file_exts, source_mode, IMPACT_RESPONSE_CAP_DEFAULT)
    }

    pub fn with_caps(
        cwd: &str,
        file_exts: &[String],
        source_mode: HttpSourceMode,
        impact_cap: usize,
    ) -> Self {
        if source_mode != HttpSourceMode::Repository {
            return Self {
                state: GraphSnapshotState::Unavailable(
                    GraphUnavailableReason::UnsupportedSourceMode,
                ),
                impact_cap,
            };
        }

        let root = Path::new(cwd);
        if !root.is_dir() {
            return Self {
                state: GraphSnapshotState::Unavailable(GraphUnavailableReason::GraphBuildFailed),
                impact_cap,
            };
        }

        let registry = create_default_registry();
        let ext_filter = normalize_exts(file_exts);
        let file_paths = find_supported_files_public(root, &registry, &ext_filter);
        let graph = EntityGraph::build(root, &file_paths, &registry);

        Self {
            state: GraphSnapshotState::Available(graph),
            impact_cap,
        }
    }

    pub fn snapshot_for_selection(
        &self,
        selection: Option<&GraphSelection>,
    ) -> GraphImpactSnapshot {
        let graph = match &self.state {
            GraphSnapshotState::Available(graph) => graph,
            GraphSnapshotState::Unavailable(reason) => {
                return GraphImpactSnapshot::unavailable(*reason, self.impact_cap);
            }
        };

        let Some(entity_id) = resolve_selection_to_entity_id(graph, selection) else {
            return GraphImpactSnapshot::unavailable(
                GraphUnavailableReason::SelectionNotResolvable,
                self.impact_cap,
            );
        };

        let mut dependencies: Vec<GraphEntityRef> = graph
            .get_dependencies(&entity_id)
            .into_iter()
            .map(entity_info_to_ref)
            .collect();
        sort_entity_refs(&mut dependencies);

        let mut dependents: Vec<GraphEntityRef> = graph
            .get_dependents(&entity_id)
            .into_iter()
            .map(entity_info_to_ref)
            .collect();
        sort_entity_refs(&mut dependents);

        let mut impact_entities: Vec<GraphEntityRef> = graph
            .impact_analysis_capped(&entity_id, self.impact_cap)
            .into_iter()
            .map(entity_info_to_ref)
            .collect();
        sort_entity_refs(&mut impact_entities);

        let full_count = graph.impact_count(&entity_id, self.impact_cap.saturating_add(1));
        let impact_truncated = full_count > self.impact_cap;
        let impact_total = full_count.min(self.impact_cap);

        GraphImpactSnapshot {
            graph_available: true,
            reason: None,
            dependencies,
            dependents,
            impact_total,
            impact_cap: self.impact_cap,
            impact_truncated,
            impact_entities,
        }
    }

    #[cfg(test)]
    fn from_graph(graph: EntityGraph, impact_cap: usize) -> Self {
        Self {
            state: GraphSnapshotState::Available(graph),
            impact_cap,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SnapshotUiInput {
    pub mode: String,
    pub view: String,
    pub context_mode: String,
    pub hunk_index: usize,
    pub scroll: usize,
    pub anchors: [usize; 2],
}

#[derive(Clone, Debug)]
pub struct SnapshotSelectionInput {
    pub selected: bool,
    pub file: Option<String>,
    pub entity_type: Option<String>,
    pub entity_name: Option<String>,
    pub line_range: Option<[usize; 2]>,
    pub ui: SnapshotUiInput,
}

#[derive(Clone, Debug)]
pub struct SnapshotSessionInput {
    pub http_enabled: bool,
    pub http_bound: bool,
    pub host: String,
    pub port: u16,
    pub source_mode: HttpSourceMode,
    pub started_at: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct HttpStateSnapshot {
    pub session: SessionSnapshot,
    pub selection: SelectionSnapshot,
    pub graph: GraphSnapshot,
    pub impact: ImpactSnapshot,
    pub panel: PanelSnapshot,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub http: SessionHttpSnapshot,
    pub source_mode: HttpSourceMode,
    pub started_at: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SessionHttpSnapshot {
    pub enabled: bool,
    pub bound: bool,
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionSnapshot {
    pub selected: bool,
    pub file: Option<String>,
    pub entity_type: Option<String>,
    pub entity_name: Option<String>,
    pub line_range: Option<[usize; 2]>,
    pub ui: SelectionUiSnapshot,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionUiSnapshot {
    pub mode: String,
    pub view: String,
    pub context_mode: String,
    pub hunk_index: usize,
    pub scroll: usize,
    pub anchors: [usize; 2],
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphSnapshot {
    pub graph_available: bool,
    pub reason: Option<GraphUnavailableReason>,
    pub dependencies: Vec<GraphEntityRef>,
    pub dependents: Vec<GraphEntityRef>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ImpactSnapshot {
    pub total: usize,
    pub cap: usize,
    pub truncated: bool,
    pub entities: Vec<GraphEntityRef>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PanelSnapshot {
    pub expanded: bool,
    pub summary: String,
}

pub fn build_state_snapshot(
    session: &SnapshotSessionInput,
    selection: SnapshotSelectionInput,
    graph_impact: &GraphImpactSnapshot,
    panel_expanded: bool,
) -> HttpStateSnapshot {
    HttpStateSnapshot {
        session: SessionSnapshot {
            http: SessionHttpSnapshot {
                enabled: session.http_enabled,
                bound: session.http_bound,
                host: session.host.clone(),
                port: session.port,
            },
            source_mode: session.source_mode,
            started_at: session.started_at.clone(),
        },
        selection: SelectionSnapshot {
            selected: selection.selected,
            file: selection.file,
            entity_type: selection.entity_type,
            entity_name: selection.entity_name,
            line_range: selection.line_range,
            ui: SelectionUiSnapshot {
                mode: selection.ui.mode,
                view: selection.ui.view,
                context_mode: selection.ui.context_mode,
                hunk_index: selection.ui.hunk_index,
                scroll: selection.ui.scroll,
                anchors: selection.ui.anchors,
            },
        },
        graph: GraphSnapshot {
            graph_available: graph_impact.graph_available,
            reason: graph_impact.reason,
            dependencies: graph_impact.dependencies.clone(),
            dependents: graph_impact.dependents.clone(),
        },
        impact: ImpactSnapshot {
            total: graph_impact.impact_total,
            cap: graph_impact.impact_cap,
            truncated: graph_impact.impact_truncated,
            entities: graph_impact.impact_entities.clone(),
        },
        panel: PanelSnapshot {
            expanded: panel_expanded,
            summary: graph_impact.panel_summary(),
        },
    }
}

pub type SharedHttpState = Arc<RwLock<HttpStateSnapshot>>;

pub fn shared_state(initial: HttpStateSnapshot) -> SharedHttpState {
    Arc::new(RwLock::new(initial))
}

pub fn replace_shared_snapshot(state: &SharedHttpState, snapshot: HttpStateSnapshot) {
    if let Ok(mut guard) = state.write() {
        *guard = snapshot;
    }
}

pub struct HttpStateServer {
    enabled: bool,
    bound: bool,
    host: String,
    port: u16,
    bind_error: Option<String>,
    stop_flag: Arc<AtomicBool>,
    join_handle: Option<JoinHandle<()>>,
}

impl HttpStateServer {
    pub fn start(enabled: bool, requested_port: u16, state: SharedHttpState) -> Self {
        if !enabled {
            return Self {
                enabled,
                bound: false,
                host: LOCALHOST.to_string(),
                port: requested_port,
                bind_error: None,
                stop_flag: Arc::new(AtomicBool::new(false)),
                join_handle: None,
            };
        }

        let listener = match TcpListener::bind((LOCALHOST, requested_port)) {
            Ok(listener) => listener,
            Err(error) => {
                return Self {
                    enabled,
                    bound: false,
                    host: LOCALHOST.to_string(),
                    port: requested_port,
                    bind_error: Some(error.to_string()),
                    stop_flag: Arc::new(AtomicBool::new(false)),
                    join_handle: None,
                };
            }
        };

        let bound_port = listener
            .local_addr()
            .map(|address| address.port())
            .unwrap_or(requested_port);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_thread = Arc::clone(&stop_flag);

        let _ = listener.set_nonblocking(true);
        let join_handle = thread::spawn(move || {
            while !stop_flag_thread.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = handle_connection(stream, &state);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(25));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            enabled,
            bound: true,
            host: LOCALHOST.to_string(),
            port: bound_port,
            bind_error: None,
            stop_flag,
            join_handle: Some(join_handle),
        }
    }

    pub fn bound(&self) -> bool {
        self.bound
    }

    pub fn bind_error(&self) -> Option<&str> {
        self.bind_error.as_deref()
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn shutdown(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if self.bound {
            let _ = TcpStream::connect((self.host.as_str(), self.port));
        }
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for HttpStateServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn handle_connection(mut stream: TcpStream, state: &SharedHttpState) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }

    let (method, target) = parse_request_line(&request_line).unwrap_or(("", "/"));

    let mut line = String::new();
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 || line == "\r\n" {
            break;
        }
    }

    let route = target.split('?').next().unwrap_or(target);

    let (status_code, reason, body) = if route == "/state" {
        if method != "GET" {
            (
                405,
                "Method Not Allowed",
                serde_json::json!({
                    "error": "methodNotAllowed",
                    "path": "/state",
                    "method": method,
                }),
            )
        } else {
            let snapshot = state
                .read()
                .ok()
                .map(|guard| (*guard).clone())
                .unwrap_or_else(fallback_state_snapshot);
            (
                200,
                "OK",
                serde_json::to_value(snapshot).expect("state snapshot must serialize"),
            )
        }
    } else {
        (
            404,
            "Not Found",
            serde_json::json!({
                "error": "notFound",
                "path": route,
            }),
        )
    };

    let body_bytes = serde_json::to_vec(&body)?;
    let response_headers = format!(
        "HTTP/1.1 {status_code} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );

    stream.write_all(response_headers.as_bytes())?;
    stream.write_all(&body_bytes)?;
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Both);

    Ok(())
}

fn parse_request_line(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.split_whitespace();
    let method = parts.next()?;
    let target = parts.next()?;
    Some((method, target))
}

fn fallback_state_snapshot() -> HttpStateSnapshot {
    HttpStateSnapshot {
        session: SessionSnapshot {
            http: SessionHttpSnapshot {
                enabled: false,
                bound: false,
                host: LOCALHOST.to_string(),
                port: 0,
            },
            source_mode: HttpSourceMode::Repository,
            started_at: String::new(),
        },
        selection: SelectionSnapshot {
            selected: false,
            file: None,
            entity_type: None,
            entity_name: None,
            line_range: None,
            ui: SelectionUiSnapshot {
                mode: "list".to_string(),
                view: "unified".to_string(),
                context_mode: "hunk".to_string(),
                hunk_index: 0,
                scroll: 0,
                anchors: [0, 0],
            },
        },
        graph: GraphSnapshot {
            graph_available: false,
            reason: Some(GraphUnavailableReason::GraphBuildFailed),
            dependencies: vec![],
            dependents: vec![],
        },
        impact: ImpactSnapshot {
            total: 0,
            cap: IMPACT_RESPONSE_CAP_DEFAULT,
            truncated: false,
            entities: vec![],
        },
        panel: PanelSnapshot {
            expanded: false,
            summary: "deps:0 depBy:0 impact:0".to_string(),
        },
    }
}

fn resolve_selection_to_entity_id(
    graph: &EntityGraph,
    selection: Option<&GraphSelection>,
) -> Option<String> {
    let selection = selection?;

    if let Some(graph_id) = selection.graph_id.as_deref() {
        if graph.entities.contains_key(graph_id) {
            return Some(graph_id.to_string());
        }
    }

    let line_range = selection
        .line_range
        .map(|[start, end]| (start.min(end), start.max(end)));

    let mut candidates: Vec<&EntityInfo> = graph
        .entities
        .values()
        .filter(|entity| {
            entity.file_path == selection.file
                && entity.entity_type == selection.entity_type
                && entity.name == selection.entity_name
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }

    if let Some((selection_start, selection_end)) = line_range {
        candidates.retain(|entity| {
            overlap_len(
                (selection_start, selection_end),
                (entity.start_line, entity.end_line),
            ) > 0
        });

        if candidates.is_empty() {
            return None;
        }

        candidates.sort_by(|a, b| {
            let overlap_a =
                overlap_len((selection_start, selection_end), (a.start_line, a.end_line));
            let overlap_b =
                overlap_len((selection_start, selection_end), (b.start_line, b.end_line));

            overlap_b
                .cmp(&overlap_a)
                .then_with(|| a.start_line.cmp(&b.start_line))
                .then_with(|| a.id.cmp(&b.id))
        });

        return candidates.first().map(|entity| entity.id.clone());
    }

    candidates.sort_by(|a, b| {
        a.start_line
            .cmp(&b.start_line)
            .then_with(|| a.end_line.cmp(&b.end_line))
            .then_with(|| a.id.cmp(&b.id))
    });

    candidates.first().map(|entity| entity.id.clone())
}

fn overlap_len(a: (usize, usize), b: (usize, usize)) -> usize {
    let start = a.0.max(b.0);
    let end = a.1.min(b.1);
    if end < start {
        return 0;
    }
    end.saturating_sub(start).saturating_add(1)
}

fn entity_info_to_ref(entity: &EntityInfo) -> GraphEntityRef {
    GraphEntityRef {
        id: entity.id.clone(),
        name: entity.name.clone(),
        file: entity.file_path.clone(),
        lines: [entity.start_line, entity.end_line],
    }
}

fn sort_entity_refs(entities: &mut [GraphEntityRef]) {
    entities.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.lines[0].cmp(&b.lines[0]))
            .then_with(|| a.lines[1].cmp(&b.lines[1]))
            .then_with(|| a.id.cmp(&b.id))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use sem_core::parser::graph::{EntityGraph, EntityInfo};
    use serde_json::Value;

    fn entity(id: &str, name: &str, file: &str, start_line: usize, end_line: usize) -> EntityInfo {
        EntityInfo {
            id: id.to_string(),
            name: name.to_string(),
            entity_type: "function".to_string(),
            file_path: file.to_string(),
            start_line,
            end_line,
        }
    }

    fn build_test_graph() -> EntityGraph {
        let root = entity("src/a.ts::function::root", "root", "src/a.ts", 10, 20);
        let dep = entity("src/a.ts::function::dep", "dep", "src/a.ts", 2, 6);
        let dep_by = entity("src/b.ts::function::depBy", "depBy", "src/b.ts", 100, 120);

        EntityGraph {
            entities: HashMap::from([
                (root.id.clone(), root.clone()),
                (dep.id.clone(), dep.clone()),
                (dep_by.id.clone(), dep_by.clone()),
            ]),
            edges: vec![],
            dependents: HashMap::from([(root.id.clone(), vec![dep_by.id.clone()])]),
            dependencies: HashMap::from([(root.id.clone(), vec![dep.id.clone()])]),
        }
    }

    fn sample_session(bound: bool, port: u16, source_mode: HttpSourceMode) -> SnapshotSessionInput {
        SnapshotSessionInput {
            http_enabled: true,
            http_bound: bound,
            host: LOCALHOST.to_string(),
            port,
            source_mode,
            started_at: "2026-03-08T21:00:00Z".to_string(),
        }
    }

    fn sample_selection(selected: bool) -> SnapshotSelectionInput {
        SnapshotSelectionInput {
            selected,
            file: selected.then(|| "src/a.ts".to_string()),
            entity_type: selected.then(|| "function".to_string()),
            entity_name: selected.then(|| "root".to_string()),
            line_range: selected.then_some([10, 20]),
            ui: SnapshotUiInput {
                mode: "detail".to_string(),
                view: "unified".to_string(),
                context_mode: "hunk".to_string(),
                hunk_index: 0,
                scroll: 0,
                anchors: [0, 0],
            },
        }
    }

    fn send_http_request(port: u16, request: &str) -> (u16, Value) {
        let mut stream =
            TcpStream::connect((LOCALHOST, port)).expect("server should accept client");
        stream
            .write_all(request.as_bytes())
            .expect("request write should succeed");
        let _ = stream.shutdown(Shutdown::Write);

        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("response should read");

        let mut sections = response.splitn(2, "\r\n\r\n");
        let header = sections.next().expect("headers must exist");
        let body = sections.next().unwrap_or("{}");

        let status_line = header.lines().next().expect("status line must exist");
        let status_code = status_line
            .split_whitespace()
            .nth(1)
            .expect("status code must exist")
            .parse::<u16>()
            .expect("status code must parse");

        let json: Value = serde_json::from_str(body).expect("body must be json");
        (status_code, json)
    }

    fn send_http_request_raw(port: u16, request: &str) -> String {
        let mut stream =
            TcpStream::connect((LOCALHOST, port)).expect("server should accept client");
        stream
            .write_all(request.as_bytes())
            .expect("request write should succeed");
        let _ = stream.shutdown(Shutdown::Write);

        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("response should read");
        response
    }

    #[test]
    fn unavailable_for_non_repository_source_mode() {
        let service = GraphSnapshotService::with_caps(".", &[], HttpSourceMode::Stdin, 1234);
        let snapshot = service.snapshot_for_selection(None);

        assert!(!snapshot.graph_available);
        assert_eq!(
            snapshot.reason,
            Some(GraphUnavailableReason::UnsupportedSourceMode)
        );
        assert_eq!(snapshot.impact_cap, 1234);
    }

    #[test]
    fn build_failure_for_missing_repository_root() {
        let service = GraphSnapshotService::new(
            "/tmp/sem-http-state-missing-root-should-not-exist",
            &[],
            HttpSourceMode::Repository,
        );
        let snapshot = service.snapshot_for_selection(None);

        assert!(!snapshot.graph_available);
        assert_eq!(
            snapshot.reason,
            Some(GraphUnavailableReason::GraphBuildFailed)
        );
    }

    #[test]
    fn selection_not_resolvable_returns_unavailable_reason() {
        let service = GraphSnapshotService::from_graph(build_test_graph(), 10_000);

        let snapshot = service.snapshot_for_selection(Some(&GraphSelection {
            graph_id: None,
            file: "src/a.ts".to_string(),
            entity_type: "function".to_string(),
            entity_name: "doesNotExist".to_string(),
            line_range: Some([1, 2]),
        }));

        assert!(!snapshot.graph_available);
        assert_eq!(
            snapshot.reason,
            Some(GraphUnavailableReason::SelectionNotResolvable)
        );
        assert_eq!(snapshot.panel_summary(), "deps:0 depBy:0 impact:0");
    }

    #[test]
    fn selection_none_returns_not_resolvable_reason() {
        let service = GraphSnapshotService::from_graph(build_test_graph(), 10_000);
        let snapshot = service.snapshot_for_selection(None);

        assert!(!snapshot.graph_available);
        assert_eq!(
            snapshot.reason,
            Some(GraphUnavailableReason::SelectionNotResolvable)
        );
        assert_eq!(snapshot.impact_total, 0);
        assert!(!snapshot.impact_truncated);
    }

    #[test]
    fn direct_graph_id_match_takes_precedence() {
        let service = GraphSnapshotService::from_graph(build_test_graph(), 10_000);

        let snapshot = service.snapshot_for_selection(Some(&GraphSelection {
            graph_id: Some("src/a.ts::function::root".to_string()),
            file: "src/a.ts".to_string(),
            entity_type: "function".to_string(),
            entity_name: "differentName".to_string(),
            line_range: Some([999, 1000]),
        }));

        assert!(snapshot.graph_available);
        assert_eq!(snapshot.reason, None);
        assert_eq!(snapshot.dependencies.len(), 1);
        assert_eq!(snapshot.dependents.len(), 1);
        assert_eq!(snapshot.impact_total, 1);
        assert!(!snapshot.impact_truncated);
    }

    #[test]
    fn fallback_overlap_prefers_highest_overlap_then_lowest_start_line() {
        let target_a = entity("src/a.ts::function::targetA", "target", "src/a.ts", 40, 60);
        let target_b = entity("src/a.ts::function::targetB", "target", "src/a.ts", 42, 62);
        let dependent = entity("src/b.ts::function::dep", "dep", "src/b.ts", 5, 10);

        let graph = EntityGraph {
            entities: HashMap::from([
                (target_a.id.clone(), target_a.clone()),
                (target_b.id.clone(), target_b.clone()),
                (dependent.id.clone(), dependent.clone()),
            ]),
            edges: vec![],
            dependents: HashMap::from([
                (target_a.id.clone(), vec![dependent.id.clone()]),
                (target_b.id.clone(), vec![]),
            ]),
            dependencies: HashMap::new(),
        };

        let service = GraphSnapshotService::from_graph(graph, 10_000);

        let snapshot = service.snapshot_for_selection(Some(&GraphSelection {
            graph_id: None,
            file: "src/a.ts".to_string(),
            entity_type: "function".to_string(),
            entity_name: "target".to_string(),
            line_range: Some([50, 52]),
        }));

        assert!(snapshot.graph_available);
        assert_eq!(snapshot.reason, None);
        assert_eq!(snapshot.impact_total, 1);
        assert_eq!(snapshot.impact_entities[0].id, dependent.id);
    }

    #[test]
    fn fallback_without_line_range_picks_lowest_start_line_candidate() {
        let target_late = entity(
            "src/a.ts::function::targetLate",
            "target",
            "src/a.ts",
            40,
            60,
        );
        let target_early = entity(
            "src/a.ts::function::targetEarly",
            "target",
            "src/a.ts",
            10,
            20,
        );
        let dependent = entity("src/b.ts::function::dep", "dep", "src/b.ts", 5, 10);

        let graph = EntityGraph {
            entities: HashMap::from([
                (target_late.id.clone(), target_late.clone()),
                (target_early.id.clone(), target_early.clone()),
                (dependent.id.clone(), dependent.clone()),
            ]),
            edges: vec![],
            dependents: HashMap::from([(target_early.id.clone(), vec![dependent.id.clone()])]),
            dependencies: HashMap::new(),
        };

        let service = GraphSnapshotService::from_graph(graph, 10_000);

        let snapshot = service.snapshot_for_selection(Some(&GraphSelection {
            graph_id: None,
            file: "src/a.ts".to_string(),
            entity_type: "function".to_string(),
            entity_name: "target".to_string(),
            line_range: None,
        }));

        assert!(snapshot.graph_available);
        assert_eq!(snapshot.reason, None);
        assert_eq!(snapshot.impact_total, 1);
        assert_eq!(snapshot.impact_entities[0].id, dependent.id);
    }

    #[test]
    fn panel_summary_uses_locked_format_with_non_zero_counts() {
        let service = GraphSnapshotService::from_graph(build_test_graph(), 10_000);
        let snapshot = service.snapshot_for_selection(Some(&GraphSelection {
            graph_id: Some("src/a.ts::function::root".to_string()),
            file: "src/a.ts".to_string(),
            entity_type: "function".to_string(),
            entity_name: "root".to_string(),
            line_range: Some([10, 20]),
        }));

        assert_eq!(snapshot.panel_summary(), "deps:1 depBy:1 impact:1");
    }

    #[test]
    fn impact_snapshot_marks_truncation_when_count_exceeds_cap() {
        let root = entity("src/a.ts::function::root", "root", "src/a.ts", 1, 2);
        let mid = entity("src/b.ts::function::mid", "mid", "src/b.ts", 3, 4);
        let leaf = entity("src/c.ts::function::leaf", "leaf", "src/c.ts", 5, 6);

        let graph = EntityGraph {
            entities: HashMap::from([
                (root.id.clone(), root.clone()),
                (mid.id.clone(), mid.clone()),
                (leaf.id.clone(), leaf.clone()),
            ]),
            edges: vec![],
            dependents: HashMap::from([
                (root.id.clone(), vec![mid.id.clone()]),
                (mid.id.clone(), vec![leaf.id.clone()]),
            ]),
            dependencies: HashMap::new(),
        };

        let service = GraphSnapshotService::from_graph(graph, 1);
        let snapshot = service.snapshot_for_selection(Some(&GraphSelection {
            graph_id: Some(root.id.clone()),
            file: root.file_path.clone(),
            entity_type: root.entity_type.clone(),
            entity_name: root.name.clone(),
            line_range: Some([1, 2]),
        }));

        assert!(snapshot.graph_available);
        assert_eq!(snapshot.impact_total, 1);
        assert!(snapshot.impact_truncated);
        assert_eq!(snapshot.impact_entities.len(), 1);
    }

    #[test]
    fn impact_snapshot_reports_zero_for_leaf_entity() {
        let leaf = entity("src/a.ts::function::leaf", "leaf", "src/a.ts", 1, 2);
        let graph = EntityGraph {
            entities: HashMap::from([(leaf.id.clone(), leaf.clone())]),
            edges: vec![],
            dependents: HashMap::new(),
            dependencies: HashMap::new(),
        };

        let service = GraphSnapshotService::from_graph(graph, 10_000);
        let snapshot = service.snapshot_for_selection(Some(&GraphSelection {
            graph_id: Some(leaf.id.clone()),
            file: leaf.file_path.clone(),
            entity_type: leaf.entity_type.clone(),
            entity_name: leaf.name.clone(),
            line_range: Some([1, 2]),
        }));

        assert!(snapshot.graph_available);
        assert_eq!(snapshot.reason, None);
        assert_eq!(snapshot.impact_total, 0);
        assert!(!snapshot.impact_truncated);
        assert!(snapshot.impact_entities.is_empty());
        assert_eq!(snapshot.panel_summary(), "deps:0 depBy:0 impact:0");
    }

    #[test]
    fn build_state_snapshot_serializes_full_shape() {
        let graph_snapshot = GraphImpactSnapshot::unavailable(
            GraphUnavailableReason::SelectionNotResolvable,
            IMPACT_RESPONSE_CAP_DEFAULT,
        );
        let snapshot = build_state_snapshot(
            &sample_session(true, 7778, HttpSourceMode::Repository),
            sample_selection(false),
            &graph_snapshot,
            false,
        );

        let value = serde_json::to_value(snapshot).expect("snapshot must serialize");
        assert!(value.get("session").is_some());
        assert!(value.get("selection").is_some());
        assert!(value.get("graph").is_some());
        assert!(value.get("impact").is_some());
        assert!(value.get("panel").is_some());
        assert_eq!(
            value
                .pointer("/graph/graphAvailable")
                .and_then(Value::as_bool),
            Some(false)
        );
    }

    #[test]
    fn build_state_snapshot_reflects_panel_expanded_flag() {
        let graph_snapshot = GraphImpactSnapshot::unavailable(
            GraphUnavailableReason::SelectionNotResolvable,
            IMPACT_RESPONSE_CAP_DEFAULT,
        );
        let expanded_snapshot = build_state_snapshot(
            &sample_session(true, 7778, HttpSourceMode::Repository),
            sample_selection(false),
            &graph_snapshot,
            true,
        );
        let collapsed_snapshot = build_state_snapshot(
            &sample_session(true, 7778, HttpSourceMode::Repository),
            sample_selection(false),
            &graph_snapshot,
            false,
        );

        assert!(expanded_snapshot.panel.expanded);
        assert!(!collapsed_snapshot.panel.expanded);
        assert_eq!(
            expanded_snapshot.panel.summary,
            "deps:0 depBy:0 impact:0".to_string()
        );
        assert_eq!(
            expanded_snapshot.panel.summary,
            collapsed_snapshot.panel.summary
        );
    }

    #[test]
    fn build_state_snapshot_serializes_source_mode_tokens() {
        let graph_snapshot = GraphImpactSnapshot::unavailable(
            GraphUnavailableReason::SelectionNotResolvable,
            IMPACT_RESPONSE_CAP_DEFAULT,
        );
        let cases = [
            (HttpSourceMode::Repository, "repository"),
            (HttpSourceMode::Stdin, "stdin"),
            (HttpSourceMode::TwoFile, "twoFile"),
        ];

        for (source_mode, expected) in cases {
            let snapshot = build_state_snapshot(
                &sample_session(true, 7778, source_mode),
                sample_selection(false),
                &graph_snapshot,
                false,
            );
            let value = serde_json::to_value(snapshot).expect("snapshot must serialize");
            assert_eq!(
                value
                    .pointer("/session/sourceMode")
                    .and_then(Value::as_str),
                Some(expected)
            );
        }
    }

    #[test]
    fn http_server_returns_state_snapshot_for_get_state() {
        let graph_snapshot = GraphImpactSnapshot::unavailable(
            GraphUnavailableReason::SelectionNotResolvable,
            IMPACT_RESPONSE_CAP_DEFAULT,
        );
        let snapshot = build_state_snapshot(
            &sample_session(true, 0, HttpSourceMode::Repository),
            sample_selection(false),
            &graph_snapshot,
            false,
        );
        let state = shared_state(snapshot);
        let mut server = HttpStateServer::start(true, 0, state);

        let (status, payload) = send_http_request(
            server.port(),
            "GET /state HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        );

        assert_eq!(status, 200);
        assert!(payload.get("session").is_some());
        assert!(payload.get("selection").is_some());
        assert!(payload.get("graph").is_some());
        assert!(payload.get("impact").is_some());
        assert!(payload.get("panel").is_some());

        server.shutdown();
    }

    #[test]
    fn http_server_returns_updated_snapshot_after_replace() {
        let graph_snapshot = GraphImpactSnapshot::unavailable(
            GraphUnavailableReason::SelectionNotResolvable,
            IMPACT_RESPONSE_CAP_DEFAULT,
        );
        let initial_snapshot = build_state_snapshot(
            &sample_session(true, 0, HttpSourceMode::Repository),
            sample_selection(false),
            &graph_snapshot,
            false,
        );
        let state = shared_state(initial_snapshot);
        let mut server = HttpStateServer::start(true, 0, state.clone());

        let (_, before_payload) = send_http_request(
            server.port(),
            "GET /state HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        );
        assert_eq!(
            before_payload
                .pointer("/selection/selected")
                .and_then(Value::as_bool),
            Some(false)
        );

        let updated_snapshot = build_state_snapshot(
            &sample_session(true, server.port(), HttpSourceMode::Repository),
            sample_selection(true),
            &graph_snapshot,
            false,
        );
        replace_shared_snapshot(&state, updated_snapshot);

        let (_, after_payload) = send_http_request(
            server.port(),
            "GET /state HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        );
        assert_eq!(
            after_payload
                .pointer("/selection/selected")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            after_payload
                .pointer("/selection/file")
                .and_then(Value::as_str),
            Some("src/a.ts")
        );

        server.shutdown();
    }

    #[test]
    fn http_server_returns_not_found_for_unknown_route() {
        let graph_snapshot = GraphImpactSnapshot::unavailable(
            GraphUnavailableReason::UnsupportedSourceMode,
            IMPACT_RESPONSE_CAP_DEFAULT,
        );
        let snapshot = build_state_snapshot(
            &sample_session(true, 0, HttpSourceMode::Stdin),
            sample_selection(false),
            &graph_snapshot,
            false,
        );
        let state = shared_state(snapshot);
        let mut server = HttpStateServer::start(true, 0, state);

        let (status, payload) = send_http_request(
            server.port(),
            "GET /unknown HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        );

        assert_eq!(status, 404);
        assert_eq!(
            payload.get("error").and_then(Value::as_str),
            Some("notFound")
        );
        assert_eq!(
            payload.get("path").and_then(Value::as_str),
            Some("/unknown")
        );

        server.shutdown();
    }

    #[test]
    fn http_server_returns_method_not_allowed_for_non_get_state() {
        let graph_snapshot = GraphImpactSnapshot::unavailable(
            GraphUnavailableReason::UnsupportedSourceMode,
            IMPACT_RESPONSE_CAP_DEFAULT,
        );
        let snapshot = build_state_snapshot(
            &sample_session(true, 0, HttpSourceMode::Stdin),
            sample_selection(false),
            &graph_snapshot,
            false,
        );
        let state = shared_state(snapshot);
        let mut server = HttpStateServer::start(true, 0, state);

        let (status, payload) = send_http_request(
            server.port(),
            "POST /state HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        );

        assert_eq!(status, 405);
        assert_eq!(
            payload.get("error").and_then(Value::as_str),
            Some("methodNotAllowed")
        );
        assert_eq!(payload.get("path").and_then(Value::as_str), Some("/state"));
        assert_eq!(payload.get("method").and_then(Value::as_str), Some("POST"));

        server.shutdown();
    }

    #[test]
    fn http_server_sets_json_content_type_header() {
        let graph_snapshot = GraphImpactSnapshot::unavailable(
            GraphUnavailableReason::SelectionNotResolvable,
            IMPACT_RESPONSE_CAP_DEFAULT,
        );
        let snapshot = build_state_snapshot(
            &sample_session(true, 0, HttpSourceMode::Repository),
            sample_selection(false),
            &graph_snapshot,
            false,
        );
        let state = shared_state(snapshot);
        let mut server = HttpStateServer::start(true, 0, state);

        let state_response = send_http_request_raw(
            server.port(),
            "GET /state HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        );
        assert!(state_response.starts_with("HTTP/1.1 200 OK"));
        assert!(state_response.contains("Content-Type: application/json"));

        let not_found_response = send_http_request_raw(
            server.port(),
            "GET /unknown HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        );
        assert!(not_found_response.starts_with("HTTP/1.1 404 Not Found"));
        assert!(not_found_response.contains("Content-Type: application/json"));

        server.shutdown();
    }

    #[test]
    fn http_server_bind_failure_is_non_fatal() {
        let occupied = TcpListener::bind((LOCALHOST, 0)).expect("must reserve ephemeral port");
        let occupied_port = occupied
            .local_addr()
            .expect("listener should have local addr")
            .port();

        let graph_snapshot = GraphImpactSnapshot::unavailable(
            GraphUnavailableReason::UnsupportedSourceMode,
            IMPACT_RESPONSE_CAP_DEFAULT,
        );
        let snapshot = build_state_snapshot(
            &sample_session(true, occupied_port, HttpSourceMode::Stdin),
            sample_selection(false),
            &graph_snapshot,
            false,
        );
        let state = shared_state(snapshot);

        let mut server = HttpStateServer::start(true, occupied_port, state);

        assert!(server.enabled());
        assert!(!server.bound());
        assert_eq!(server.port(), occupied_port);
        assert!(server.bind_error().is_some());

        server.shutdown();
    }
}
