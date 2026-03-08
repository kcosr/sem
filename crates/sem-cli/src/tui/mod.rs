mod app;
mod detail;
mod render;

use std::io;
use std::path::Path;
use std::time::Duration;

use crossterm::event::{self, DisableMouseCapture, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use sem_core::parser::differ::DiffResult;

use crate::commands::diff::DiffView;

pub fn run_tui(result: &DiffResult, initial_view: DiffView) -> io::Result<()> {
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    enable_raw_mode()?;

    let guard = TerminalGuard;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app_state = app::AppState::from_diff_result(result, initial_view);
    app_state.set_list_header_command(invoked_command_line());
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
    }

    drop(guard);
    terminal.show_cursor()?;
    Ok(())
}

fn invoked_command_line() -> String {
    let args: Vec<String> = std::env::args().collect();
    format_invoked_command(&args)
}

fn format_invoked_command(args: &[String]) -> String {
    if args.is_empty() {
        return "sem diff --tui".to_string();
    }

    let executable = Path::new(&args[0])
        .file_name()
        .and_then(|name| name.to_str())
        .map_or_else(|| args[0].clone(), ToString::to_string);

    std::iter::once(executable)
        .chain(args.iter().skip(1).cloned())
        .collect::<Vec<_>>()
        .join(" ")
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
    use super::format_invoked_command;

    #[test]
    fn format_invoked_command_uses_executable_basename() {
        let args = vec![
            "/home/kevin/worktrees/sem/crates/target/debug/sem".to_string(),
            "diff".to_string(),
            "--tui".to_string(),
            "--diff-view".to_string(),
            "side-by-side".to_string(),
        ];
        assert_eq!(
            format_invoked_command(&args),
            "sem diff --tui --diff-view side-by-side"
        );
    }
}
