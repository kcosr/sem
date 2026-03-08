use std::collections::HashMap;
use std::path::Path;

use sem_core::parser::graph::{EntityGraph, EntityInfo};
use sem_core::parser::plugins::create_default_registry;

use crate::commands::graph::{find_supported_files_public, normalize_exts};

pub const IMPACT_RESPONSE_CAP_DEFAULT: usize = 10_000;
pub const PANEL_DISPLAY_CAP_DEFAULT: usize = 25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
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
        assert_eq!(snapshot.reason, Some(GraphUnavailableReason::SelectionNotResolvable));
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
        let target_late = entity("src/a.ts::function::targetLate", "target", "src/a.ts", 40, 60);
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
}
