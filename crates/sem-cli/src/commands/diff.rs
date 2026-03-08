use std::io::Read;
use std::path::Path;
use std::process;
use std::time::Instant;
use std::{collections::HashMap, fmt};

use clap::ValueEnum;
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
    Commit,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitStepAction {
    Older,
    Newer,
}

impl fmt::Display for CommitStepAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommitStepAction::Older => write!(f, "stepOlder"),
            CommitStepAction::Newer => write!(f, "stepNewer"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitCursor {
    pub rev_label: Option<String>,
    pub sha: String,
    pub subject: String,
    pub has_older: bool,
    pub has_newer: bool,
}

#[derive(Clone, Debug)]
pub struct CommitNavigationContext {
    pub cwd: String,
    pub file_exts: Vec<String>,
    pub source_mode: TuiSourceMode,
    pub(crate) lineage: Vec<String>,
    pub(crate) lineage_index: HashMap<String, usize>,
}

#[derive(Clone, Debug)]
pub struct CommitSnapshot {
    pub cursor: CommitCursor,
    pub result: DiffResult,
}

#[derive(Clone, Debug)]
pub struct CommitStepRequest {
    pub request_id: u64,
    pub action: CommitStepAction,
    pub current_sha: String,
    pub source_mode: TuiSourceMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitLoadStatus {
    Loaded,
    LoadFailed,
    UnsupportedMode,
    BoundaryNoop,
    IgnoredStaleResult,
}

#[derive(Clone, Debug)]
pub struct CommitStepResponse {
    pub applied_request_id: u64,
    pub status: CommitLoadStatus,
    pub snapshot: Option<CommitSnapshot>,
    pub error: Option<String>,
    pub retain_previous_snapshot: bool,
}

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
        if result.changes.is_empty() && commit_navigation.is_none() {
            return Ok(Some(format_terminal(result)));
        }

        tui::run_tui(result, opts.diff_view, commit_navigation)
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
) -> Result<Option<(CommitNavigationContext, CommitCursor)>, String> {
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
    let lineage_index: HashMap<String, usize> = lineage
        .iter()
        .enumerate()
        .map(|(index, sha)| (sha.clone(), index))
        .collect();
    let current_sha = git
        .resolve_commit_sha(commit_ref)
        .map_err(|error| format!("resolving commit {commit_ref}: {error}"))?;

    let context = CommitNavigationContext {
        cwd: opts.cwd.clone(),
        file_exts: opts.file_exts.clone(),
        source_mode: TuiSourceMode::Commit,
        lineage,
        lineage_index,
    };
    let cursor = build_commit_cursor(&git, &context, &current_sha)?;
    Ok(Some((context, cursor)))
}

pub fn process_commit_step_request(
    context: &CommitNavigationContext,
    request: &CommitStepRequest,
) -> CommitStepResponse {
    if context.source_mode != TuiSourceMode::Commit || request.source_mode != TuiSourceMode::Commit
    {
        return CommitStepResponse {
            applied_request_id: request.request_id,
            status: CommitLoadStatus::UnsupportedMode,
            snapshot: None,
            error: None,
            retain_previous_snapshot: true,
        };
    }

    let git = match GitBridge::open(Path::new(&context.cwd)) {
        Ok(git) => git,
        Err(error) => {
            return CommitStepResponse {
                applied_request_id: request.request_id,
                status: CommitLoadStatus::LoadFailed,
                snapshot: None,
                error: Some(error.to_string()),
                retain_previous_snapshot: true,
            };
        }
    };

    let target_sha = match resolve_step_target(&git, context, &request.current_sha, request.action)
    {
        Ok(Some(target_sha)) => target_sha,
        Ok(None) => {
            return CommitStepResponse {
                applied_request_id: request.request_id,
                status: CommitLoadStatus::BoundaryNoop,
                snapshot: None,
                error: None,
                retain_previous_snapshot: true,
            };
        }
        Err(error) => {
            return CommitStepResponse {
                applied_request_id: request.request_id,
                status: CommitLoadStatus::LoadFailed,
                snapshot: None,
                error: Some(error),
                retain_previous_snapshot: true,
            };
        }
    };

    let result = match load_commit_diff_result(&context.cwd, &target_sha, &context.file_exts) {
        Ok(result) => result,
        Err(error) => {
            return CommitStepResponse {
                applied_request_id: request.request_id,
                status: CommitLoadStatus::LoadFailed,
                snapshot: None,
                error: Some(error),
                retain_previous_snapshot: true,
            };
        }
    };

    match build_commit_cursor(&git, context, &target_sha) {
        Ok(cursor) => CommitStepResponse {
            applied_request_id: request.request_id,
            status: CommitLoadStatus::Loaded,
            snapshot: Some(CommitSnapshot { cursor, result }),
            error: None,
            retain_previous_snapshot: false,
        },
        Err(error) => CommitStepResponse {
            applied_request_id: request.request_id,
            status: CommitLoadStatus::LoadFailed,
            snapshot: None,
            error: Some(error),
            retain_previous_snapshot: true,
        },
    }
}

fn resolve_step_target(
    git: &GitBridge,
    context: &CommitNavigationContext,
    current_sha: &str,
    action: CommitStepAction,
) -> Result<Option<String>, String> {
    let index = context.lineage_index.get(current_sha).copied();
    match action {
        CommitStepAction::Older => {
            if let Some(index) = index {
                if index + 1 < context.lineage.len() {
                    return Ok(Some(context.lineage[index + 1].clone()));
                }
                return Ok(None);
            }
            git.get_first_parent_sha(current_sha)
                .map_err(|error| format!("resolving first parent for {current_sha}: {error}"))
        }
        CommitStepAction::Newer => {
            if let Some(index) = index {
                if index > 0 {
                    return Ok(Some(context.lineage[index - 1].clone()));
                }
                return Ok(None);
            }
            Ok(None)
        }
    }
}

fn build_commit_cursor(
    git: &GitBridge,
    context: &CommitNavigationContext,
    sha: &str,
) -> Result<CommitCursor, String> {
    let index = context.lineage_index.get(sha).copied();
    let has_newer = index.is_some_and(|value| value > 0);
    let has_older = if let Some(index) = index {
        index + 1 < context.lineage.len()
    } else {
        git.get_first_parent_sha(sha)
            .map_err(|error| format!("resolving first parent for {sha}: {error}"))?
            .is_some()
    };
    let subject = git
        .get_commit_subject(sha)
        .map_err(|error| format!("resolving subject for {sha}: {error}"))?;

    Ok(CommitCursor {
        rev_label: index.map(|value| format!("HEAD~{value}")),
        sha: sha.to_string(),
        subject,
        has_older,
        has_newer,
    })
}

fn load_commit_diff_result(
    cwd: &str,
    sha: &str,
    file_exts: &[String],
) -> Result<DiffResult, String> {
    let git =
        GitBridge::open(Path::new(cwd)).map_err(|_| "Not inside a Git repository.".to_string())?;
    let scope = DiffScope::Commit {
        sha: sha.to_string(),
    };
    let file_changes = git
        .get_changed_files(&scope)
        .map_err(|error| format!("loading changed files for commit {sha}: {error}"))?;
    let filtered = filter_file_changes(file_changes, file_exts);
    Ok(compute_diff_result(&filtered).result)
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
        let lineage_index: HashMap<String, usize> = lineage
            .iter()
            .enumerate()
            .map(|(index, sha)| (sha.clone(), index))
            .collect();
        let context = CommitNavigationContext {
            cwd: base.to_string_lossy().to_string(),
            file_exts: vec![],
            source_mode: TuiSourceMode::Commit,
            lineage: lineage.clone(),
            lineage_index,
        };

        let middle_sha = lineage[1].clone();

        let older = process_commit_step_request(
            &context,
            &CommitStepRequest {
                request_id: 1,
                action: CommitStepAction::Older,
                current_sha: middle_sha.clone(),
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
                current_sha: middle_sha,
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
                current_sha: lineage[0].clone(),
                source_mode: TuiSourceMode::Commit,
            },
        );
        assert_eq!(boundary.status, CommitLoadStatus::BoundaryNoop);

        let root_boundary = process_commit_step_request(
            &context,
            &CommitStepRequest {
                request_id: 4,
                action: CommitStepAction::Older,
                current_sha: lineage
                    .last()
                    .expect("lineage should have a root commit")
                    .clone(),
                source_mode: TuiSourceMode::Commit,
            },
        );
        assert_eq!(root_boundary.status, CommitLoadStatus::BoundaryNoop);

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn process_commit_step_request_returns_unsupported_for_non_commit_mode() {
        let context = CommitNavigationContext {
            cwd: "/tmp/not-used".to_string(),
            file_exts: vec![],
            source_mode: TuiSourceMode::Unsupported,
            lineage: vec![],
            lineage_index: HashMap::new(),
        };

        let response = process_commit_step_request(
            &context,
            &CommitStepRequest {
                request_id: 7,
                action: CommitStepAction::Older,
                current_sha: "deadbeef".to_string(),
                source_mode: TuiSourceMode::Unsupported,
            },
        );
        assert_eq!(response.status, CommitLoadStatus::UnsupportedMode);
        assert!(response.retain_previous_snapshot);
    }
}
