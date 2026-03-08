use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::commands::diff::DiffView;

use super::app::{AppState, Mode};
use super::detail::LineKind;

pub fn draw(frame: &mut Frame<'_>, app: &AppState) {
    match app.mode() {
        Mode::List => draw_list(frame, app),
        Mode::Detail => draw_detail(frame, app),
    }

    if app.show_help() {
        draw_help_overlay(frame);
    }
}

fn draw_list(frame: &mut Frame<'_>, app: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(frame.area());

    frame.render_widget(
        Paragraph::new("sem diff --tui (List)").style(Style::default().fg(Color::Cyan)),
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
        Paragraph::new("Controls: ↑/↓ j/k move, Enter open, g/G jump, ? help, q quit"),
        chunks[2],
    );
}

fn draw_detail(frame: &mut Frame<'_>, app: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(frame.area());

    let view_label = match app.effective_view() {
        DiffView::Unified => "unified",
        DiffView::SideBySide => "side-by-side",
    };

    frame.render_widget(
        Paragraph::new(format!("Detail: {} ({view_label})", app.detail_title()))
            .style(Style::default().fg(Color::Cyan)),
        chunks[0],
    );

    match app.effective_view() {
        DiffView::Unified => {
            let lines: Vec<Line<'_>> = app
                .unified_lines()
                .iter()
                .map(|(kind, text)| {
                    let style = match kind {
                        LineKind::Header => Style::default().fg(Color::Magenta),
                        LineKind::Added => Style::default().fg(Color::Green),
                        LineKind::Removed => Style::default().fg(Color::Red),
                        LineKind::Modified => Style::default().fg(Color::Yellow),
                        LineKind::Unchanged => Style::default(),
                    };
                    Line::styled(text.clone(), style)
                })
                .collect();

            frame.render_widget(
                Paragraph::new(lines)
                    .block(Block::default().borders(Borders::ALL).title("Unified Diff"))
                    .wrap(Wrap { trim: false })
                    .scroll((saturating_scroll(app.detail_scroll()), 0)),
                chunks[1],
            );
        }
        DiffView::SideBySide => {
            let area = chunks[1];
            let line_width = area.width.saturating_sub(6) as usize;
            let half = (line_width / 2).max(20);
            let lines: Vec<Line<'_>> = app
                .side_by_side_lines()
                .iter()
                .map(|line| {
                    let left = format_column(line.left_number, &line.left_text, half);
                    let right = format_column(line.right_number, &line.right_text, half);
                    let style = match line.kind {
                        LineKind::Header => Style::default().fg(Color::Magenta),
                        LineKind::Added => Style::default().fg(Color::Green),
                        LineKind::Removed => Style::default().fg(Color::Red),
                        LineKind::Modified => Style::default().fg(Color::Yellow),
                        LineKind::Unchanged => Style::default(),
                    };
                    Line::styled(format!("{left} | {right}"), style)
                })
                .collect();

            frame.render_widget(
                Paragraph::new(lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title("Side-by-Side Diff"),
                    )
                    .scroll((saturating_scroll(app.detail_scroll()), 0)),
                chunks[1],
            );
        }
    }

    let mut footer =
        "Controls: Esc list, Tab view, n/p hunks, PgUp/PgDn scroll, g/G top-bottom, ? help, q quit"
            .to_string();
    if app.fallback_active() {
        footer.push_str(" | width too narrow for side-by-side, showing unified");
    }

    frame.render_widget(Paragraph::new(footer), chunks[2]);
}

fn draw_help_overlay(frame: &mut Frame<'_>) {
    let popup = centered_rect(80, 60, frame.area());
    frame.render_widget(Clear, popup);

    let help_lines = vec![
        Line::from("List Mode:"),
        Line::from("  ↑/↓ or j/k move selection"),
        Line::from("  Enter open detail"),
        Line::from("  g/G jump top/bottom"),
        Line::from("Detail Mode:"),
        Line::from("  Esc back to list"),
        Line::from("  Tab toggle unified/side-by-side"),
        Line::from("  n/p next/previous hunk"),
        Line::from("  PageUp/PageDown scroll by page"),
        Line::from("  g/G jump top/bottom"),
        Line::from("Global:"),
        Line::from("  ? toggle help"),
        Line::from("  q quit"),
    ];

    let paragraph = Paragraph::new(help_lines)
        .block(Block::default().title("Help").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, popup);
}

fn format_column(number: Option<usize>, text: &str, width: usize) -> String {
    let number_text = number.map_or_else(|| "    ".to_string(), |value| format!("{value:>4}"));
    let available = width.saturating_sub(number_text.len() + 1);
    let trimmed = if text.chars().count() > available {
        let keep = available.saturating_sub(1);
        let clipped: String = text.chars().take(keep).collect();
        format!("{clipped}…")
    } else {
        text.to_string()
    };
    let content = format!("{number_text} {trimmed}");
    format!("{content:width$}")
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn saturating_scroll(scroll: usize) -> u16 {
    u16::try_from(scroll).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use sem_core::model::change::{ChangeType, SemanticChange};
    use sem_core::parser::differ::DiffResult;

    use crate::commands::diff::DiffView;
    use crate::tui::app::AppState;

    fn sample_result() -> DiffResult {
        DiffResult {
            changes: vec![SemanticChange {
                id: "c1".to_string(),
                entity_id: "f::x".to_string(),
                change_type: ChangeType::Modified,
                entity_type: "function".to_string(),
                entity_name: "x".to_string(),
                file_path: "src/x.rs".to_string(),
                old_file_path: None,
                before_content: Some("line1\nline2\nline3\n".to_string()),
                after_content: Some("line1\nline2 changed\nline3\n".to_string()),
                commit_sha: None,
                author: None,
                timestamp: None,
                structural_change: Some(true),
                before_start_line: Some(1),
                before_end_line: Some(3),
                after_start_line: Some(1),
                after_end_line: Some(3),
            }],
            file_count: 1,
            added_count: 0,
            modified_count: 1,
            deleted_count: 0,
            moved_count: 0,
            renamed_count: 0,
        }
    }

    #[test]
    fn format_column_truncates_utf8_safely() {
        let output = format_column(Some(1), "abc漢字def", 8);
        assert!(output.contains('…'));
    }

    #[test]
    fn draw_handles_narrow_width_with_side_by_side_fallback() {
        let result = sample_result();

        let mut app = AppState::from_diff_result(&result, DiffView::SideBySide);
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.set_viewport(90, 20);
        assert!(app.fallback_active());

        let backend = TestBackend::new(90, 20);
        let mut terminal = Terminal::new(backend).expect("terminal should initialize");
        terminal
            .draw(|frame| draw(frame, &app))
            .expect("draw should succeed on narrow width");
    }

    #[test]
    fn draw_list_mode_with_help_overlay_succeeds() {
        let mut app = AppState::from_diff_result(&sample_result(), DiffView::Unified);
        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));

        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("terminal should initialize");
        terminal
            .draw(|frame| draw(frame, &app))
            .expect("draw should succeed with help overlay");
    }
}
