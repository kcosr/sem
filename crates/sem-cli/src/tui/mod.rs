mod app;
mod render;

use std::io;
use std::time::Duration;

use crossterm::event::{self, DisableMouseCapture, Event};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
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

    while !app_state.should_quit() {
        terminal.draw(|frame| render::draw(frame, &app_state))?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                app_state.handle_key(key);
            }
        }
    }

    drop(guard);
    terminal.show_cursor()?;
    Ok(())
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
    }
}
