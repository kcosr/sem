use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::commands::diff::DiffView;

use super::app::AppState;

pub fn draw(frame: &mut Frame<'_>, app: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1), Constraint::Length(2)])
        .split(frame.area());

    let mode = match app.initial_view() {
        DiffView::Unified => "unified",
        DiffView::SideBySide => "side-by-side",
    };

    frame.render_widget(
        Paragraph::new(format!(
            "sem diff --tui  (initial diff view: {mode})"
        ))
        .style(Style::default().fg(Color::Cyan)),
        chunks[0],
    );

    let items: Vec<ListItem<'_>> = app
        .rows()
        .iter()
        .map(|row| {
            let mut line = format!(
                "{}  {} {} [{}]",
                row.file_path, row.entity_type, row.entity_name, row.change_type
            );
            if let Some(range_label) = &row.range_label {
                line.push(' ');
                line.push_str(range_label);
            }
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().title("Entities").borders(Borders::ALL))
        .highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
        .highlight_symbol("▶ ");

    let mut state = ListState::default();
    state.select(Some(app.selected()));
    frame.render_stateful_widget(list, chunks[1], &mut state);

    frame.render_widget(
        Paragraph::new("Controls: ↑/↓ or j/k move, q quit"),
        chunks[2],
    );
}
