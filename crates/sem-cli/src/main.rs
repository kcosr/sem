mod commands;
mod formatters;
mod tui;

use clap::{Parser, Subcommand, ValueEnum};
use commands::blame::{blame_command, BlameOptions};
use commands::diff::{diff_command, DiffOptions, DiffView, OutputFormat};
use commands::graph::{graph_command, GraphFormat, GraphOptions};
use commands::impact::{impact_command, ImpactOptions};

#[derive(Debug, Parser)]
#[command(name = "sem", version = "0.3.1", about = "Semantic version control")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum DiffFormatArg {
    Terminal,
    Json,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Show semantic diff of changes
    Diff {
        /// Two files to compare (e.g. sem diff old.ts new.ts)
        #[arg(num_args = 0..=2)]
        files: Vec<String>,

        /// Show only staged changes
        #[arg(long)]
        staged: bool,

        /// Show changes from a specific commit
        #[arg(long)]
        commit: Option<String>,

        /// Start of commit range
        #[arg(long)]
        from: Option<String>,

        /// End of commit range
        #[arg(long)]
        to: Option<String>,

        /// Read FileChange[] JSON from stdin instead of git
        #[arg(long)]
        stdin: bool,

        /// Output format: terminal or json (invalid with --tui)
        #[arg(long, conflicts_with = "tui")]
        format: Option<DiffFormatArg>,

        /// Launch interactive TUI mode
        #[arg(long)]
        tui: bool,

        /// Initial diff view for TUI mode
        #[arg(long, default_value = "unified", requires = "tui")]
        diff_view: DiffView,

        /// Show internal timing profile
        #[arg(long, hide = true)]
        profile: bool,

        /// Only include files with these extensions (e.g. --file-exts .py .rs)
        #[arg(long)]
        file_exts: Vec<String>,
    },
    /// Show impact of changing an entity (what else would break?)
    Impact {
        /// Name of the entity to analyze
        #[arg()]
        entity: String,

        /// Specific files to analyze (default: all supported files)
        #[arg(long)]
        files: Vec<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Only include files with these extensions (e.g. --file-exts .py .rs)
        #[arg(long)]
        file_exts: Vec<String>,
    },
    /// Show semantic blame — who last modified each entity
    Blame {
        /// File to blame
        #[arg()]
        file: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show entity dependency graph
    Graph {
        /// Specific files to analyze (default: all supported files)
        #[arg()]
        files: Vec<String>,

        /// Show dependencies/dependents for a specific entity
        #[arg(long)]
        entity: Option<String>,

        /// Output format: terminal or json
        #[arg(long, default_value = "terminal")]
        format: String,

        /// Only include files with these extensions (e.g. --file-exts .py .rs)
        #[arg(long)]
        file_exts: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Diff {
            files,
            staged,
            commit,
            from,
            to,
            stdin,
            format,
            tui,
            diff_view,
            profile,
            file_exts,
        }) => {
            let output_format = match format.unwrap_or(DiffFormatArg::Terminal) {
                DiffFormatArg::Json => OutputFormat::Json,
                DiffFormatArg::Terminal => OutputFormat::Terminal,
            };

            diff_command(DiffOptions {
                cwd: std::env::current_dir()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                format: output_format,
                tui,
                diff_view,
                staged,
                commit,
                from,
                to,
                stdin,
                profile,
                file_exts,
                files,
            });
        }
        Some(Commands::Blame { file, json }) => {
            blame_command(BlameOptions {
                cwd: std::env::current_dir()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                file_path: file,
                json,
            });
        }
        Some(Commands::Impact {
            entity,
            files,
            json,
            file_exts,
        }) => {
            impact_command(ImpactOptions {
                cwd: std::env::current_dir()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                entity_name: entity,
                file_paths: files,
                json,
                file_exts,
            });
        }
        Some(Commands::Graph {
            files,
            entity,
            format,
            file_exts,
        }) => {
            let graph_format = match format.as_str() {
                "json" => GraphFormat::Json,
                _ => GraphFormat::Terminal,
            };

            graph_command(GraphOptions {
                cwd: std::env::current_dir()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                file_paths: files,
                entity,
                format: graph_format,
                file_exts,
            });
        }
        None => {
            // Default to diff when no subcommand is given
            diff_command(DiffOptions {
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
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn diff_rejects_tui_with_format() {
        let parsed = Cli::try_parse_from(["sem", "diff", "--tui", "--format", "json"]);
        assert!(parsed.is_err());
        let error = parsed.expect_err("must error").to_string();
        assert!(error.contains("--format"));
        assert!(error.contains("--tui"));
    }

    #[test]
    fn diff_accepts_tui_with_side_by_side_view() {
        let parsed = Cli::try_parse_from(["sem", "diff", "--tui", "--diff-view", "side-by-side"])
            .expect("must parse");
        match parsed.command {
            Some(Commands::Diff {
                tui,
                diff_view,
                format,
                ..
            }) => {
                assert!(tui);
                assert_eq!(diff_view, DiffView::SideBySide);
                assert_eq!(format, None);
            }
            _ => panic!("expected diff command"),
        }
    }

    #[test]
    fn diff_tui_defaults_to_unified_view() {
        let parsed = Cli::try_parse_from(["sem", "diff", "--tui"]).expect("must parse");
        match parsed.command {
            Some(Commands::Diff { diff_view, .. }) => {
                assert_eq!(diff_view, DiffView::Unified);
            }
            _ => panic!("expected diff command"),
        }
    }
}
