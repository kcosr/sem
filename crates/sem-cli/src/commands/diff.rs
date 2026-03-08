use std::io::Read;
use std::path::Path;
use std::process;
use std::time::Instant;
use std::{collections::HashMap, fmt};

use clap::ValueEnum;
use git2::{Delta, DiffOptions as GitDiffOptions, Repository, Tree};
use sem_core::git::bridge::GitBridge;
use sem_core::git::types::{DiffScope, FileChange, FileStatus};
use sem_core::parser::differ::{compute_semantic_diff, DiffResult};
use sem_core::parser::plugins::create_default_registry;

use crate::formatters::{json::format_json, terminal::format_terminal};
use crate::tui;

pub struct DiffOptions {
    pub cwd: String,
    pub format: OutputFormat,
    pub tui: bool,
    pub diff_view: DiffView,
    pub staged: bool,
    pub commit: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub stdin: bool,
    pub profile: bool,
    pub file_exts: Vec<String>,
    pub files: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Terminal,
    Json,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum DiffView {
    Unified,
    #[value(name = "side-by-side")]
    SideBySide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TuiSourceMode {
    Unified,
    Commit,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepAction {
    Older,
    Newer,
}

impl fmt::Display for StepAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StepAction::Older => write!(f, "stepOlder"),
            StepAction::Newer => write!(f, "stepNewer"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepCursor {
    pub endpoint_id: String,
    pub index: usize,
    pub sha: String,
    pub rev_label: Option<String>,
    pub subject: String,
    pub has_older: bool,
    pub has_newer: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepEndpointKind {
    Commit { sha: String },
    Index,
    Working,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepEndpoint {
    pub endpoint_id: String,
    pub display_ref: Option<String>,
    pub kind: StepEndpointKind,
}

#[derive(Clone, Debug)]
pub struct StepNavigationContext {
    pub cwd: String,
    pub file_exts: Vec<String>,
    pub source_mode: TuiSourceMode,
    pub(crate) endpoints: Vec<StepEndpoint>,
    pub(crate) endpoint_index: HashMap<String, usize>,
}

#[derive(Clone, Debug)]
pub struct TuiRangeContext {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug)]
pub struct StepSnapshot {
    pub cursor: StepCursor,
    pub result: DiffResult,
}

#[derive(Clone, Debug)]
pub struct StepRequest {
    pub request_id: u64,
    pub action: StepAction,
    pub current_endpoint_id: String,
    pub current_index: usize,
    pub source_mode: TuiSourceMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepLoadStatus {
    Loaded,
    LoadFailed,
    UnsupportedMode,
    BoundaryNoop,
    IgnoredStaleResult,
}

#[derive(Clone, Debug)]
pub struct StepResponse {
    pub applied_request_id: u64,
    pub status: StepLoadStatus,
    pub snapshot: Option<StepSnapshot>,
    pub error: Option<String>,
    pub retain_previous_snapshot: bool,
}

pub type CommitStepAction = StepAction;
pub type CommitCursor = StepCursor;
pub type CommitNavigationContext = StepNavigationContext;
pub type CommitSnapshot = StepSnapshot;
pub type CommitStepRequest = StepRequest;
pub type CommitLoadStatus = StepLoadStatus;
pub type CommitStepResponse = StepResponse;

struct InputPhase {
    file_changes: Vec<FileChange>,
    from_stdin: bool,
    input_ms: f64,
}

struct ComputePhase {
    result: DiffResult,
    registry_ms: f64,
    parse_diff_ms: f64,
}

pub fn diff_command(opts: DiffOptions) {
    let total_start = Instant::now();

    let input = collect_diff_input_with_stdin(&opts, None).unwrap_or_else(|message| {
        eprintln!("\x1b[31mError: {message}\x1b[0m");
        process::exit(1);
    });

    let file_changes = filter_file_changes(input.file_changes, &opts.file_exts);

    if file_changes.is_empty() {
        println!("\x1b[2mNo changes detected.\x1b[0m");
        return;
    }

    let compute = compute_diff_result(&file_changes);

    let t4 = Instant::now();
    let output = execute_output_phase(&opts, &compute.result).unwrap_or_else(|message| {
        eprintln!("\x1b[31mError: {message}\x1b[0m");
        process::exit(1);
    });
    let format_ms = t4.elapsed().as_secs_f64() * 1000.0;

    if let Some(text) = output {
        println!("{text}");
    }

    if opts.profile {
        let total_ms = total_start.elapsed().as_secs_f64() * 1000.0;
        eprintln!();
        eprintln!("\x1b[2m── Profile ──────────────────────────────────\x1b[0m");
        eprintln!(
            "\x1b[2m  input ({})  {:>8.2}ms\x1b[0m",
            if input.from_stdin { "stdin" } else { "git" },
            input.input_ms
        );
        eprintln!(
            "\x1b[2m  registry init        {:>8.2}ms\x1b[0m",
            compute.registry_ms
        );
        eprintln!(
            "\x1b[2m  parse + match        {:>8.2}ms\x1b[0m",
            compute.parse_diff_ms
        );
        eprintln!("\x1b[2m  format output        {:>8.2}ms\x1b[0m", format_ms);
        eprintln!("\x1b[2m  ─────────────────────────────────\x1b[0m");
        eprintln!("\x1b[2m  total                {:>8.2}ms\x1b[0m", total_ms);
        eprintln!(
            "\x1b[2m  files: {}  entities: {}  changes: {}\x1b[0m",
            file_changes.len(),
            compute.result.changes.len(),
            compute.result.added_count
                + compute.result.modified_count
                + compute.result.deleted_count
                + compute.result.moved_count
                + compute.result.renamed_count
        );
        eprintln!("\x1b[2m─────────────────────────────────────────────\x1b[0m");
    }
}

fn collect_diff_input_with_stdin(
    opts: &DiffOptions,
    stdin_override: Option<&str>,
) -> Result<InputPhase, String> {
    let start = Instant::now();

    let (file_changes, from_stdin) = if opts.files.len() == 2 {
        let path_a = Path::new(&opts.files[0]);
        let path_b = Path::new(&opts.files[1]);

        let content_a = std::fs::read_to_string(path_a)
            .map_err(|error| format!("reading {}: {error}", path_a.display()))?;
        let content_b = std::fs::read_to_string(path_b)
            .map_err(|error| format!("reading {}: {error}", path_b.display()))?;

        let change = FileChange {
            file_path: opts.files[1].clone(),
            old_file_path: None,
            status: FileStatus::Modified,
            before_content: Some(content_a),
            after_content: Some(content_b),
        };

        (vec![change], false)
    } else if opts.files.len() == 1 {
        return Err("provide two files to compare, or none for git diff.".to_string());
    } else if opts.stdin {
        let input = if let Some(override_input) = stdin_override {
            override_input.to_string()
        } else {
            let mut buffer = String::new();
            std::io::stdin()
                .read_to_string(&mut buffer)
                .map_err(|error| format!("reading stdin: {error}"))?;
            buffer
        };

        let changes: Vec<FileChange> =
            serde_json::from_str(&input).map_err(|error| format!("parsing stdin JSON: {error}"))?;
        (changes, true)
    } else {
        let git = GitBridge::open(Path::new(&opts.cwd))
            .map_err(|_| "Not inside a Git repository.".to_string())?;

        let (_scope, changes) = if let Some(ref sha) = opts.commit {
            let scope = DiffScope::Commit { sha: sha.clone() };
            let changes = git
                .get_changed_files(&scope)
                .map_err(|error| error.to_string())?;
            (scope, changes)
        } else if let (Some(ref from), Some(ref to)) = (&opts.from, &opts.to) {
            let scope = DiffScope::Range {
                from: from.clone(),
                to: to.clone(),
            };
            let changes = git
                .get_changed_files(&scope)
                .map_err(|error| error.to_string())?;
            (scope, changes)
        } else if opts.staged {
            let scope = DiffScope::Staged;
            let changes = git
                .get_changed_files(&scope)
                .map_err(|error| error.to_string())?;
            (scope, changes)
        } else {
            git.detect_and_get_files()
                .map_err(|_| "Not inside a Git repository.".to_string())?
        };

        (changes, false)
    };

    Ok(InputPhase {
        file_changes,
        from_stdin,
        input_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

fn filter_file_changes(file_changes: Vec<FileChange>, file_exts: &[String]) -> Vec<FileChange> {
    if file_exts.is_empty() {
        return file_changes;
    }

    let normalized_exts: Vec<String> = file_exts
        .iter()
        .map(|extension| {
            if extension.starts_with('.') {
                extension.clone()
            } else {
                format!(".{extension}")
            }
        })
        .collect();

    file_changes
        .into_iter()
        .filter(|change| {
            normalized_exts
                .iter()
                .any(|extension| change.file_path.ends_with(extension.as_str()))
        })
        .collect()
}

fn compute_diff_result(file_changes: &[FileChange]) -> ComputePhase {
    let t2 = Instant::now();
    let registry = create_default_registry();
    let registry_ms = t2.elapsed().as_secs_f64() * 1000.0;

    let t3 = Instant::now();
    let result = compute_semantic_diff(file_changes, &registry, None, None);
    let parse_diff_ms = t3.elapsed().as_secs_f64() * 1000.0;

    ComputePhase {
        result,
        registry_ms,
        parse_diff_ms,
    }
}

fn execute_output_phase(opts: &DiffOptions, result: &DiffResult) -> Result<Option<String>, String> {
    if opts.tui {
        let commit_navigation = build_commit_navigation_context(opts)?;
        let range_context = build_tui_range_context(opts)?;
        if result.changes.is_empty() && commit_navigation.is_none() {
            return Ok(Some(format_terminal(result)));
        }

        tui::run_tui(result, opts.diff_view, commit_navigation, range_context)
            .map_err(|error| format!("failed to start TUI: {error}"))?;
        return Ok(None);
    }

    let output = match opts.format {
        OutputFormat::Json => format_json(result),
        OutputFormat::Terminal => format_terminal(result),
    };

    Ok(Some(output))
}

pub fn is_commit_navigation_mode(opts: &DiffOptions) -> bool {
    opts.tui
        && opts.commit.is_some()
        && opts.files.is_empty()
        && !opts.stdin
        && !opts.staged
        && opts.from.is_none()
        && opts.to.is_none()
}

pub fn build_commit_navigation_context(
    opts: &DiffOptions,
) -> Result<Option<(StepNavigationContext, StepCursor)>, String> {
    if !is_commit_navigation_mode(opts) {
        return Ok(None);
    }

    let commit_ref = opts
        .commit
        .as_deref()
        .ok_or_else(|| "commit navigation requires --commit <rev>".to_string())?;
    let git = GitBridge::open(Path::new(&opts.cwd))
        .map_err(|_| "Not inside a Git repository.".to_string())?;

    let session_head_sha = git
        .get_head_sha()
        .map_err(|error| format!("resolving HEAD: {error}"))?;
    let lineage = git
        .get_first_parent_lineage(&session_head_sha)
        .map_err(|error| format!("building first-parent lineage: {error}"))?;
    let rev_labels: HashMap<String, String> = lineage
        .iter()
        .enumerate()
        .map(|(index, sha)| (sha.clone(), format!("HEAD~{index}")))
        .collect();
    let endpoints: Vec<StepEndpoint> = lineage
        .iter()
        .rev()
        .map(|sha| StepEndpoint {
            endpoint_id: commit_endpoint_id(sha),
            display_ref: rev_labels.get(sha).cloned(),
            kind: StepEndpointKind::Commit { sha: sha.clone() },
        })
        .collect();
    let endpoint_index: HashMap<String, usize> = endpoints
        .iter()
        .enumerate()
        .map(|(index, endpoint)| (endpoint.endpoint_id.clone(), index))
        .collect();
    let current_sha = git
        .resolve_commit_sha(commit_ref)
        .map_err(|error| format!("resolving commit {commit_ref}: {error}"))?;
    let current_endpoint_id = commit_endpoint_id(&current_sha);

    let context = StepNavigationContext {
        cwd: opts.cwd.clone(),
        file_exts: opts.file_exts.clone(),
        source_mode: TuiSourceMode::Commit,
        endpoints,
        endpoint_index,
    };
    let cursor = build_step_cursor(&git, &context, &current_endpoint_id)?;
    Ok(Some((context, cursor)))
}

pub fn build_tui_range_context(opts: &DiffOptions) -> Result<Option<TuiRangeContext>, String> {
    let (from_ref, to_ref) = match (&opts.from, &opts.to) {
        (Some(from_ref), Some(to_ref))
            if opts.tui
                && opts.files.is_empty()
                && !opts.stdin
                && !opts.staged
                && opts.commit.is_none() =>
        {
            (from_ref.as_str(), to_ref.as_str())
        }
        _ => return Ok(None),
    };

    let git = GitBridge::open(Path::new(&opts.cwd))
        .map_err(|_| "Not inside a Git repository.".to_string())?;
    let from = describe_ref_with_details(&git, from_ref)?;
    let to = describe_ref_with_details(&git, to_ref)?;

    Ok(Some(TuiRangeContext { from, to }))
}

fn describe_ref_with_details(git: &GitBridge, refspec: &str) -> Result<String, String> {
    let sha = git
        .resolve_commit_sha(refspec)
        .map_err(|error| format!("resolving commit {refspec}: {error}"))?;
    let short_sha: String = sha.chars().take(7).collect();
    let subject = git
        .get_commit_subject(&sha)
        .map_err(|error| format!("resolving subject for {refspec}: {error}"))?;
    Ok(format!("{refspec}  {short_sha}  {subject}"))
}

pub fn process_step_request(
    context: &StepNavigationContext,
    request: &StepRequest,
) -> StepResponse {
    let context_supported = context.source_mode == TuiSourceMode::Unified
        || context.source_mode == TuiSourceMode::Commit;
    let request_supported = request.source_mode == TuiSourceMode::Unified
        || request.source_mode == TuiSourceMode::Commit;
    if !context_supported || !request_supported {
        return StepResponse {
            applied_request_id: request.request_id,
            status: StepLoadStatus::UnsupportedMode,
            snapshot: None,
            error: None,
            retain_previous_snapshot: true,
        };
    }

    let git = match GitBridge::open(Path::new(&context.cwd)) {
        Ok(git) => git,
        Err(error) => {
            return StepResponse {
                applied_request_id: request.request_id,
                status: StepLoadStatus::LoadFailed,
                snapshot: None,
                error: Some(error.to_string()),
                retain_previous_snapshot: true,
            };
        }
    };

    let target_endpoint_id = match resolve_step_target_endpoint_id(context, request) {
        Ok(Some(target_endpoint_id)) => target_endpoint_id,
        Ok(None) => {
            return StepResponse {
                applied_request_id: request.request_id,
                status: StepLoadStatus::BoundaryNoop,
                snapshot: None,
                error: None,
                retain_previous_snapshot: true,
            };
        }
        Err(error) => {
            return StepResponse {
                applied_request_id: request.request_id,
                status: StepLoadStatus::LoadFailed,
                snapshot: None,
                error: Some(error),
                retain_previous_snapshot: true,
            };
        }
    };

    let target_index = context
        .endpoint_index
        .get(&target_endpoint_id)
        .copied()
        .unwrap_or_default();
    let from_endpoint = if target_index == 0 {
        context
            .endpoints
            .get(target_index)
            .expect("target endpoint index must be valid")
    } else {
        context
            .endpoints
            .get(target_index - 1)
            .expect("pairwise source endpoint index must be valid")
    };
    let to_endpoint = context
        .endpoints
        .get(target_index)
        .expect("target endpoint index must be valid");

    let result = match load_endpoint_diff_result(
        &context.cwd,
        from_endpoint,
        to_endpoint,
        &context.file_exts,
    ) {
        Ok(result) => result,
        Err(error) => {
            return StepResponse {
                applied_request_id: request.request_id,
                status: StepLoadStatus::LoadFailed,
                snapshot: None,
                error: Some(error),
                retain_previous_snapshot: true,
            };
        }
    };

    match build_step_cursor(&git, context, &target_endpoint_id) {
        Ok(cursor) => StepResponse {
            applied_request_id: request.request_id,
            status: StepLoadStatus::Loaded,
            snapshot: Some(StepSnapshot { cursor, result }),
            error: None,
            retain_previous_snapshot: false,
        },
        Err(error) => StepResponse {
            applied_request_id: request.request_id,
            status: StepLoadStatus::LoadFailed,
            snapshot: None,
            error: Some(error),
            retain_previous_snapshot: true,
        },
    }
}

pub fn process_commit_step_request(
    context: &StepNavigationContext,
    request: &StepRequest,
) -> StepResponse {
    process_step_request(context, request)
}

fn resolve_step_target_endpoint_id(
    context: &StepNavigationContext,
    request: &StepRequest,
) -> Result<Option<String>, String> {
    let index = context
        .endpoint_index
        .get(&request.current_endpoint_id)
        .copied()
        .or_else(|| {
            context
                .endpoints
                .get(request.current_index)
                .map(|_| request.current_index)
        })
        .ok_or_else(|| {
            format!(
                "current endpoint {} not found in active path",
                request.current_endpoint_id
            )
        })?;

    if index != request.current_index {
        return Err(format!(
            "cursor index mismatch for endpoint {}: request={}, resolved={index}",
            request.current_endpoint_id, request.current_index
        ));
    }

    let max_index = context.endpoints.len().saturating_sub(1);

    match request.action {
        StepAction::Older => {
            if index == 0 {
                Ok(None)
            } else {
                Ok(context
                    .endpoints
                    .get(index - 1)
                    .map(|endpoint| endpoint.endpoint_id.clone()))
            }
        }
        StepAction::Newer => {
            if index >= max_index {
                Ok(None)
            } else {
                Ok(context
                    .endpoints
                    .get(index + 1)
                    .map(|endpoint| endpoint.endpoint_id.clone()))
            }
        }
    }
}

fn build_step_cursor(
    git: &GitBridge,
    context: &StepNavigationContext,
    endpoint_id: &str,
) -> Result<StepCursor, String> {
    let index = context
        .endpoint_index
        .get(endpoint_id)
        .copied()
        .ok_or_else(|| format!("endpoint {endpoint_id} not found in active path"))?;
    let endpoint = context
        .endpoints
        .get(index)
        .ok_or_else(|| format!("endpoint index {index} out of bounds"))?;
    let subject = match &endpoint.kind {
        StepEndpointKind::Commit { sha } => git
            .get_commit_subject(sha)
            .map_err(|error| format!("resolving subject for {sha}: {error}"))?,
        StepEndpointKind::Index => "INDEX".to_string(),
        StepEndpointKind::Working => "WORKING".to_string(),
    };
    let has_older = index > 0;
    let has_newer = index + 1 < context.endpoints.len();

    let sha = match &endpoint.kind {
        StepEndpointKind::Commit { sha } => sha.clone(),
        StepEndpointKind::Index => "index".to_string(),
        StepEndpointKind::Working => "working".to_string(),
    };

    Ok(StepCursor {
        endpoint_id: endpoint.endpoint_id.clone(),
        index,
        sha,
        rev_label: endpoint.display_ref.clone(),
        subject,
        has_older,
        has_newer,
    })
}

fn commit_endpoint_id(sha: &str) -> String {
    format!("commit:{sha}")
}

fn endpoint_id_to_kind(endpoint_id: &str) -> Result<StepEndpointKind, String> {
    if endpoint_id.eq_ignore_ascii_case("index") {
        return Ok(StepEndpointKind::Index);
    }
    if endpoint_id.eq_ignore_ascii_case("working") {
        return Ok(StepEndpointKind::Working);
    }
    let Some(sha) = endpoint_id.strip_prefix("commit:") else {
        return Err(format!("unsupported endpoint id: {endpoint_id}"));
    };
    Ok(StepEndpointKind::Commit {
        sha: sha.to_string(),
    })
}

fn load_endpoint_diff_result(
    cwd: &str,
    from: &StepEndpoint,
    to: &StepEndpoint,
    file_exts: &[String],
) -> Result<DiffResult, String> {
    if from.endpoint_id == to.endpoint_id {
        return Ok(compute_diff_result(&[]).result);
    }

    let file_changes = load_changed_files_between_endpoints(cwd, &from.kind, &to.kind)?;
    let filtered = filter_file_changes(file_changes, file_exts);
    Ok(compute_diff_result(&filtered).result)
}

fn load_changed_files_between_endpoints(
    cwd: &str,
    from: &StepEndpointKind,
    to: &StepEndpointKind,
) -> Result<Vec<FileChange>, String> {
    let repo =
        Repository::discover(cwd).map_err(|error| format!("opening git repository: {error}"))?;
    let repo_root = repo
        .workdir()
        .ok_or_else(|| "Not inside a Git repository.".to_string())?
        .to_path_buf();
    let from_snapshot = resolve_endpoint_snapshot(&repo, from)?;
    let to_snapshot = resolve_endpoint_snapshot(&repo, to)?;
    let entries = diff_entries_between_endpoints(&repo, from, to)?;

    entries
        .into_iter()
        .map(|entry| {
            let file_path = entry
                .after_path
                .clone()
                .or(entry.before_path.clone())
                .ok_or_else(|| "diff entry missing path".to_string())?;
            let old_file_path = if entry.status == FileStatus::Renamed {
                entry.before_path.clone()
            } else {
                None
            };
            let before_content = if entry.status == FileStatus::Added {
                None
            } else {
                entry
                    .before_path
                    .as_deref()
                    .and_then(|path| read_endpoint_content(&repo, &repo_root, &from_snapshot, path))
            };
            let after_content = if entry.status == FileStatus::Deleted {
                None
            } else {
                entry
                    .after_path
                    .as_deref()
                    .and_then(|path| read_endpoint_content(&repo, &repo_root, &to_snapshot, path))
            };

            Ok(FileChange {
                file_path,
                status: entry.status,
                old_file_path,
                before_content,
                after_content,
            })
        })
        .collect()
}

#[derive(Clone, Debug)]
struct DiffEntry {
    status: FileStatus,
    before_path: Option<String>,
    after_path: Option<String>,
}

enum EndpointSnapshot<'repo> {
    Commit(Tree<'repo>),
    Index(git2::Index),
    Working,
}

fn resolve_endpoint_snapshot<'repo>(
    repo: &'repo Repository,
    endpoint: &StepEndpointKind,
) -> Result<EndpointSnapshot<'repo>, String> {
    match endpoint {
        StepEndpointKind::Commit { sha } => {
            resolve_commit_tree(repo, sha).map(EndpointSnapshot::Commit)
        }
        StepEndpointKind::Index => repo
            .index()
            .map(EndpointSnapshot::Index)
            .map_err(|error| format!("reading git index: {error}")),
        StepEndpointKind::Working => Ok(EndpointSnapshot::Working),
    }
}

fn resolve_commit_tree<'repo>(repo: &'repo Repository, sha: &str) -> Result<Tree<'repo>, String> {
    let obj = repo
        .revparse_single(sha)
        .map_err(|error| format!("resolving commit {sha}: {error}"))?;
    let commit = obj
        .peel_to_commit()
        .map_err(|error| format!("loading commit {sha}: {error}"))?;
    commit
        .tree()
        .map_err(|error| format!("loading tree for {sha}: {error}"))
}

fn resolve_index_tree<'repo>(repo: &'repo Repository) -> Result<Tree<'repo>, String> {
    let mut index = repo
        .index()
        .map_err(|error| format!("reading git index: {error}"))?;
    let oid = index
        .write_tree_to(repo)
        .map_err(|error| format!("materializing index tree: {error}"))?;
    repo.find_tree(oid)
        .map_err(|error| format!("loading index tree object: {error}"))
}

fn diff_entries_between_endpoints(
    repo: &Repository,
    from: &StepEndpointKind,
    to: &StepEndpointKind,
) -> Result<Vec<DiffEntry>, String> {
    match (from, to) {
        (StepEndpointKind::Commit { sha: from_sha }, StepEndpointKind::Commit { sha: to_sha }) => {
            let from_tree = resolve_commit_tree(repo, from_sha)?;
            let to_tree = resolve_commit_tree(repo, to_sha)?;
            let diff = repo
                .diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None)
                .map_err(|error| format!("building diff {from_sha}..{to_sha}: {error}"))?;
            Ok(diff_entries_from_diff(&diff))
        }
        (StepEndpointKind::Commit { sha }, StepEndpointKind::Index) => {
            let from_tree = resolve_commit_tree(repo, sha)?;
            let index = repo
                .index()
                .map_err(|error| format!("reading git index: {error}"))?;
            let diff = repo
                .diff_tree_to_index(Some(&from_tree), Some(&index), None)
                .map_err(|error| format!("building diff commit:{sha}..index: {error}"))?;
            Ok(diff_entries_from_diff(&diff))
        }
        (StepEndpointKind::Commit { sha }, StepEndpointKind::Working) => {
            let from_tree = resolve_commit_tree(repo, sha)?;
            let mut opts = GitDiffOptions::new();
            opts.include_untracked(true).recurse_untracked_dirs(true);
            let diff = repo
                .diff_tree_to_workdir_with_index(Some(&from_tree), Some(&mut opts))
                .map_err(|error| format!("building diff commit:{sha}..working: {error}"))?;
            Ok(diff_entries_from_diff(&diff))
        }
        (StepEndpointKind::Index, StepEndpointKind::Working) => {
            let index = repo
                .index()
                .map_err(|error| format!("reading git index: {error}"))?;
            let mut opts = GitDiffOptions::new();
            opts.include_untracked(true).recurse_untracked_dirs(true);
            let diff = repo
                .diff_index_to_workdir(Some(&index), Some(&mut opts))
                .map_err(|error| format!("building diff index..working: {error}"))?;
            Ok(diff_entries_from_diff(&diff))
        }
        (StepEndpointKind::Index, StepEndpointKind::Commit { sha }) => {
            let from_tree = resolve_index_tree(repo)?;
            let to_tree = resolve_commit_tree(repo, sha)?;
            let diff = repo
                .diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None)
                .map_err(|error| format!("building diff index..commit:{sha}: {error}"))?;
            Ok(diff_entries_from_diff(&diff))
        }
        (StepEndpointKind::Working, StepEndpointKind::Index) => {
            let entries = diff_entries_between_endpoints(
                repo,
                &StepEndpointKind::Index,
                &StepEndpointKind::Working,
            )?;
            Ok(invert_diff_entries(entries))
        }
        (StepEndpointKind::Working, StepEndpointKind::Commit { sha }) => {
            let entries = diff_entries_between_endpoints(
                repo,
                &StepEndpointKind::Commit { sha: sha.clone() },
                &StepEndpointKind::Working,
            )?;
            Ok(invert_diff_entries(entries))
        }
        (StepEndpointKind::Index, StepEndpointKind::Index)
        | (StepEndpointKind::Working, StepEndpointKind::Working) => Ok(vec![]),
    }
}

fn diff_entries_from_diff(diff: &git2::Diff<'_>) -> Vec<DiffEntry> {
    let mut entries = Vec::new();
    for delta in diff.deltas() {
        let old_path = delta
            .old_file()
            .path()
            .and_then(|path| path.to_str())
            .map(str::to_string);
        let new_path = delta
            .new_file()
            .path()
            .and_then(|path| path.to_str())
            .map(str::to_string);
        let status = match delta.status() {
            Delta::Added => FileStatus::Added,
            Delta::Deleted => FileStatus::Deleted,
            Delta::Modified => FileStatus::Modified,
            Delta::Renamed => FileStatus::Renamed,
            _ => continue,
        };
        let entry = match status {
            FileStatus::Added => DiffEntry {
                status,
                before_path: None,
                after_path: new_path,
            },
            FileStatus::Deleted => DiffEntry {
                status,
                before_path: old_path,
                after_path: None,
            },
            FileStatus::Modified | FileStatus::Renamed => DiffEntry {
                status,
                before_path: old_path.clone().or(new_path.clone()),
                after_path: new_path.or(old_path),
            },
        };
        entries.push(entry);
    }
    entries
}

fn invert_diff_entries(entries: Vec<DiffEntry>) -> Vec<DiffEntry> {
    entries
        .into_iter()
        .map(|entry| {
            let status = match entry.status {
                FileStatus::Added => FileStatus::Deleted,
                FileStatus::Deleted => FileStatus::Added,
                FileStatus::Modified => FileStatus::Modified,
                FileStatus::Renamed => FileStatus::Renamed,
            };
            DiffEntry {
                status,
                before_path: entry.after_path,
                after_path: entry.before_path,
            }
        })
        .collect()
}

fn read_endpoint_content(
    repo: &Repository,
    repo_root: &Path,
    endpoint: &EndpointSnapshot<'_>,
    path: &str,
) -> Option<String> {
    match endpoint {
        EndpointSnapshot::Commit(tree) => read_blob_from_tree(repo, tree, path),
        EndpointSnapshot::Index(index) => read_blob_from_index(repo, index, path),
        EndpointSnapshot::Working => {
            let absolute = repo_root.join(path);
            std::fs::read_to_string(absolute).ok()
        }
    }
}

fn read_blob_from_tree(repo: &Repository, tree: &Tree<'_>, path: &str) -> Option<String> {
    let entry = tree.get_path(Path::new(path)).ok()?;
    let blob = repo.find_blob(entry.id()).ok()?;
    String::from_utf8(blob.content().to_vec()).ok()
}

fn read_blob_from_index(repo: &Repository, index: &git2::Index, path: &str) -> Option<String> {
    let entry = index.get_path(Path::new(path), 0)?;
    let blob = repo.find_blob(entry.id).ok()?;
    String::from_utf8(blob.content().to_vec()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn base_options() -> DiffOptions {
        DiffOptions {
            cwd: std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            format: OutputFormat::Terminal,
            tui: false,
            diff_view: DiffView::Unified,
            staged: false,
            commit: None,
            from: None,
            to: None,
            stdin: false,
            profile: false,
            file_exts: vec![],
            files: vec![],
        }
    }

    #[test]
    fn collect_diff_input_supports_stdin_mode() {
        let mut options = base_options();
        options.stdin = true;

        let input = r#"[
          {
            "filePath": "src/a.ts",
            "status": "modified",
            "beforeContent": "fn old() {}",
            "afterContent": "fn new() {}"
          }
        ]"#;

        let phase =
            collect_diff_input_with_stdin(&options, Some(input)).expect("stdin mode should parse");
        assert!(phase.from_stdin);
        assert_eq!(phase.file_changes.len(), 1);
        assert_eq!(phase.file_changes[0].file_path, "src/a.ts");
    }

    #[test]
    fn collect_diff_input_supports_two_file_mode() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        let base = std::env::temp_dir().join(format!("sem-diff-h1-{stamp}"));
        std::fs::create_dir_all(&base).expect("temp dir should be created");

        let before_path = base.join("before.rs");
        let after_path = base.join("after.rs");
        std::fs::write(&before_path, "fn old() {}\n").expect("before file should be written");
        std::fs::write(&after_path, "fn new() {}\n").expect("after file should be written");

        let mut options = base_options();
        options.files = vec![
            before_path.to_string_lossy().to_string(),
            after_path.to_string_lossy().to_string(),
        ];

        let phase =
            collect_diff_input_with_stdin(&options, None).expect("two-file mode should parse");
        assert!(!phase.from_stdin);
        assert_eq!(phase.file_changes.len(), 1);
        assert_eq!(
            phase.file_changes[0].after_content.as_deref(),
            Some("fn new() {}\n")
        );

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn collect_diff_input_rejects_single_file_mode() {
        let mut options = base_options();
        options.files = vec!["only-one.rs".to_string()];

        let error = collect_diff_input_with_stdin(&options, None)
            .err()
            .expect("must fail");
        assert_eq!(error, "provide two files to compare, or none for git diff.");
    }

    #[test]
    fn execute_output_phase_returns_no_change_message_for_tui() {
        let mut options = base_options();
        options.tui = true;

        let empty_result = DiffResult {
            changes: vec![],
            file_count: 0,
            added_count: 0,
            modified_count: 0,
            deleted_count: 0,
            moved_count: 0,
            renamed_count: 0,
        };

        let output = execute_output_phase(&options, &empty_result)
            .expect("empty result should be rendered")
            .expect("tui no-change path should return terminal text");
        assert!(output.contains("No semantic changes detected."));
    }

    #[test]
    fn is_commit_navigation_mode_requires_explicit_commit_tui_mode() {
        let mut options = base_options();
        options.tui = true;
        options.commit = Some("HEAD~1".to_string());
        assert!(is_commit_navigation_mode(&options));

        let mut without_commit = base_options();
        without_commit.tui = true;
        assert!(!is_commit_navigation_mode(&without_commit));

        let mut with_stdin = base_options();
        with_stdin.tui = true;
        with_stdin.commit = Some("HEAD~1".to_string());
        with_stdin.stdin = true;
        assert!(!is_commit_navigation_mode(&with_stdin));

        let mut with_staged = base_options();
        with_staged.tui = true;
        with_staged.commit = Some("HEAD~1".to_string());
        with_staged.staged = true;
        assert!(!is_commit_navigation_mode(&with_staged));

        let mut with_range = base_options();
        with_range.tui = true;
        with_range.commit = Some("HEAD~1".to_string());
        with_range.from = Some("HEAD~3".to_string());
        with_range.to = Some("HEAD~1".to_string());
        assert!(!is_commit_navigation_mode(&with_range));

        let mut with_files = base_options();
        with_files.tui = true;
        with_files.commit = Some("HEAD~1".to_string());
        with_files.files = vec!["a.rs".to_string(), "b.rs".to_string()];
        assert!(!is_commit_navigation_mode(&with_files));
    }

    fn init_repo_with_three_commits(base: &Path) -> Vec<String> {
        std::fs::create_dir_all(base).expect("temp repo dir should be created");

        run_git(base, &["init"]);
        run_git(base, &["config", "user.email", "sem@example.com"]);
        run_git(base, &["config", "user.name", "sem"]);

        std::fs::write(base.join("example.rs"), "fn one() {}\n").expect("first write should work");
        run_git(base, &["add", "."]);
        run_git(base, &["commit", "-m", "first"]);

        std::fs::write(base.join("example.rs"), "fn one() {}\nfn two() {}\n")
            .expect("second write should work");
        run_git(base, &["add", "."]);
        run_git(base, &["commit", "-m", "second"]);

        std::fs::write(
            base.join("example.rs"),
            "fn one() {}\nfn two() {}\nfn three() {}\n",
        )
        .expect("third write should work");
        run_git(base, &["add", "."]);
        run_git(base, &["commit", "-m", "third"]);

        let git = GitBridge::open(base).expect("repo should open");
        let head = git.get_head_sha().expect("head should resolve");
        git.get_first_parent_lineage(&head)
            .expect("lineage should resolve")
    }

    fn run_git(base: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(base)
            .status()
            .expect("git command should spawn");
        assert!(
            status.success(),
            "git {:?} must succeed, exit code: {:?}",
            args,
            status.code()
        );
    }

    #[test]
    fn process_commit_step_request_transitions_cursor_on_lineage() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        let base = std::env::temp_dir().join(format!("sem-commit-nav-{stamp}"));
        let lineage = init_repo_with_three_commits(&base);
        let endpoints: Vec<StepEndpoint> = lineage
            .iter()
            .rev()
            .map(|sha| StepEndpoint {
                endpoint_id: commit_endpoint_id(sha),
                display_ref: None,
                kind: StepEndpointKind::Commit { sha: sha.clone() },
            })
            .collect();
        let endpoint_index: HashMap<String, usize> = endpoints
            .iter()
            .enumerate()
            .map(|(index, endpoint)| (endpoint.endpoint_id.clone(), index))
            .collect();
        let context = CommitNavigationContext {
            cwd: base.to_string_lossy().to_string(),
            file_exts: vec![],
            source_mode: TuiSourceMode::Commit,
            endpoints,
            endpoint_index,
        };

        let middle_sha = lineage[1].clone();
        let middle_endpoint_id = commit_endpoint_id(&middle_sha);

        let older = process_commit_step_request(
            &context,
            &CommitStepRequest {
                request_id: 1,
                action: CommitStepAction::Older,
                current_endpoint_id: middle_endpoint_id.clone(),
                current_index: 1,
                source_mode: TuiSourceMode::Commit,
            },
        );
        assert_eq!(older.status, CommitLoadStatus::Loaded);
        let older_cursor = older
            .snapshot
            .expect("older request should return snapshot")
            .cursor;
        assert_eq!(older_cursor.sha, lineage[2]);
        assert!(!older_cursor.has_older);
        assert!(older_cursor.has_newer);

        let newer = process_commit_step_request(
            &context,
            &CommitStepRequest {
                request_id: 2,
                action: CommitStepAction::Newer,
                current_endpoint_id: middle_endpoint_id,
                current_index: 1,
                source_mode: TuiSourceMode::Commit,
            },
        );
        assert_eq!(newer.status, CommitLoadStatus::Loaded);
        let newer_cursor = newer
            .snapshot
            .expect("newer request should return snapshot")
            .cursor;
        assert_eq!(newer_cursor.sha, lineage[0]);
        assert!(newer_cursor.has_older);
        assert!(!newer_cursor.has_newer);

        let boundary = process_commit_step_request(
            &context,
            &CommitStepRequest {
                request_id: 3,
                action: CommitStepAction::Newer,
                current_endpoint_id: commit_endpoint_id(&lineage[0]),
                current_index: 2,
                source_mode: TuiSourceMode::Commit,
            },
        );
        assert_eq!(boundary.status, CommitLoadStatus::BoundaryNoop);

        let root_boundary = process_commit_step_request(
            &context,
            &CommitStepRequest {
                request_id: 4,
                action: CommitStepAction::Older,
                current_endpoint_id: commit_endpoint_id(
                    lineage.last().expect("lineage should have a root commit"),
                ),
                current_index: 0,
                source_mode: TuiSourceMode::Commit,
            },
        );
        assert_eq!(root_boundary.status, CommitLoadStatus::BoundaryNoop);

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn process_commit_step_request_returns_unsupported_for_non_commit_mode() {
        let context = CommitNavigationContext {
            cwd: std::env::current_dir()
                .expect("cwd should resolve")
                .to_string_lossy()
                .to_string(),
            file_exts: vec![],
            source_mode: TuiSourceMode::Unsupported,
            endpoints: vec![],
            endpoint_index: HashMap::new(),
        };

        let response = process_commit_step_request(
            &context,
            &CommitStepRequest {
                request_id: 7,
                action: CommitStepAction::Older,
                current_endpoint_id: "deadbeef".to_string(),
                current_index: 0,
                source_mode: TuiSourceMode::Unsupported,
            },
        );
        assert_eq!(response.status, CommitLoadStatus::UnsupportedMode);
        assert!(response.retain_previous_snapshot);
    }

    #[test]
    fn build_commit_navigation_context_builds_oldest_to_newest_endpoint_path() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        let base = std::env::temp_dir().join(format!("sem-step-path-{stamp}"));
        let lineage = init_repo_with_three_commits(&base);

        let mut options = base_options();
        options.cwd = base.to_string_lossy().to_string();
        options.tui = true;
        options.commit = Some("HEAD~1".to_string());

        let (context, cursor) = build_commit_navigation_context(&options)
            .expect("context should build")
            .expect("commit mode should enable stepping");

        assert_eq!(context.endpoints.len(), 3);
        assert_eq!(
            context.endpoints[0].endpoint_id,
            commit_endpoint_id(lineage.last().expect("root commit should exist"))
        );
        assert_eq!(
            context.endpoints[2].endpoint_id,
            commit_endpoint_id(&lineage[0])
        );
        assert_eq!(cursor.endpoint_id, commit_endpoint_id(&lineage[1]));
        assert_eq!(cursor.index, 1);

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn endpoint_id_to_kind_parses_commit_index_and_working() {
        let commit = endpoint_id_to_kind("commit:abc123").expect("commit endpoint id must parse");
        assert_eq!(
            commit,
            StepEndpointKind::Commit {
                sha: "abc123".to_string()
            }
        );
        assert_eq!(
            endpoint_id_to_kind("index").expect("index endpoint id must parse"),
            StepEndpointKind::Index
        );
        assert_eq!(
            endpoint_id_to_kind("WORKING").expect("working endpoint id must parse"),
            StepEndpointKind::Working
        );
    }

    #[test]
    fn endpoint_loader_supports_commit_to_index_and_index_to_working() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        let base = std::env::temp_dir().join(format!("sem-endpoint-loader-{stamp}"));
        std::fs::create_dir_all(&base).expect("temp repo dir should be created");

        run_git(&base, &["init"]);
        run_git(&base, &["config", "user.email", "sem@example.com"]);
        run_git(&base, &["config", "user.name", "sem"]);
        std::fs::write(base.join("example.rs"), "fn one() {}\n")
            .expect("initial file should write");
        run_git(&base, &["add", "."]);
        run_git(&base, &["commit", "-m", "first"]);

        let git = GitBridge::open(&base).expect("repo should open");
        let head_sha = git.get_head_sha().expect("head should resolve");

        std::fs::write(base.join("example.rs"), "fn one_staged() {}\n")
            .expect("staged content should write");
        run_git(&base, &["add", "example.rs"]);

        let commit_to_index = load_changed_files_between_endpoints(
            &base.to_string_lossy(),
            &StepEndpointKind::Commit {
                sha: head_sha.clone(),
            },
            &StepEndpointKind::Index,
        )
        .expect("commit->index loader should succeed");
        let staged_change = commit_to_index
            .iter()
            .find(|change| change.file_path == "example.rs")
            .expect("staged change should be present");
        assert_eq!(staged_change.status, FileStatus::Modified);
        assert_eq!(
            staged_change.before_content.as_deref(),
            Some("fn one() {}\n")
        );
        assert_eq!(
            staged_change.after_content.as_deref(),
            Some("fn one_staged() {}\n")
        );

        std::fs::write(base.join("example.rs"), "fn one_working() {}\n")
            .expect("working content should write");

        let index_to_working = load_changed_files_between_endpoints(
            &base.to_string_lossy(),
            &StepEndpointKind::Index,
            &StepEndpointKind::Working,
        )
        .expect("index->working loader should succeed");
        let working_change = index_to_working
            .iter()
            .find(|change| change.file_path == "example.rs")
            .expect("working change should be present");
        assert_eq!(working_change.status, FileStatus::Modified);
        assert_eq!(
            working_change.before_content.as_deref(),
            Some("fn one_staged() {}\n")
        );
        assert_eq!(
            working_change.after_content.as_deref(),
            Some("fn one_working() {}\n")
        );

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn load_endpoint_diff_result_returns_empty_for_self_comparison() {
        let endpoint = StepEndpoint {
            endpoint_id: "index".to_string(),
            display_ref: Some("INDEX".to_string()),
            kind: StepEndpointKind::Index,
        };
        let result = load_endpoint_diff_result("/tmp/not-used", &endpoint, &endpoint, &[])
            .expect("self comparison should return empty diff result");
        assert_eq!(result.file_count, 0);
        assert_eq!(result.changes.len(), 0);
    }

    #[test]
    fn process_step_request_reports_cursor_index_mismatch_as_load_failed() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        let base = std::env::temp_dir().join(format!("sem-step-mismatch-{stamp}"));
        std::fs::create_dir_all(&base).expect("temp repo dir should be created");
        run_git(&base, &["init"]);
        run_git(&base, &["config", "user.email", "sem@example.com"]);
        run_git(&base, &["config", "user.name", "sem"]);
        std::fs::write(base.join("example.rs"), "fn one() {}\n").expect("file should write");
        run_git(&base, &["add", "."]);
        run_git(&base, &["commit", "-m", "first"]);

        let context = CommitNavigationContext {
            cwd: base.to_string_lossy().to_string(),
            file_exts: vec![],
            source_mode: TuiSourceMode::Commit,
            endpoints: vec![
                StepEndpoint {
                    endpoint_id: "commit:a".to_string(),
                    display_ref: None,
                    kind: StepEndpointKind::Commit {
                        sha: "a".to_string(),
                    },
                },
                StepEndpoint {
                    endpoint_id: "commit:b".to_string(),
                    display_ref: None,
                    kind: StepEndpointKind::Commit {
                        sha: "b".to_string(),
                    },
                },
            ],
            endpoint_index: HashMap::from([
                ("commit:a".to_string(), 0usize),
                ("commit:b".to_string(), 1usize),
            ]),
        };

        let response = process_step_request(
            &context,
            &StepRequest {
                request_id: 99,
                action: StepAction::Older,
                current_endpoint_id: "commit:b".to_string(),
                current_index: 0,
                source_mode: TuiSourceMode::Commit,
            },
        );
        assert_eq!(response.status, StepLoadStatus::LoadFailed);
        assert!(response.retain_previous_snapshot);
        let error = response.error.unwrap_or_default();
        assert!(
            error.contains("cursor index mismatch"),
            "unexpected error: {error}"
        );

        let _ = std::fs::remove_dir_all(base);
    }
}
