use std::io::Read;
use std::path::Path;
use std::process;
use std::time::Instant;

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
        if result.changes.is_empty() {
            return Ok(Some(format_terminal(result)));
        }

        tui::run_tui(result, opts.diff_view)
            .map_err(|error| format!("failed to start TUI: {error}"))?;
        return Ok(None);
    }

    let output = match opts.format {
        OutputFormat::Json => format_json(result),
        OutputFormat::Terminal => format_terminal(result),
    };

    Ok(Some(output))
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
