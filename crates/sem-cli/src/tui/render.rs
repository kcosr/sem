use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use sem_core::model::change::ChangeType;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::sync::{Mutex, MutexGuard};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

use crate::commands::diff::DiffView;

use super::app::{AppState, Mode};
use super::detail::LineKind;

const ICON_COL_WIDTH: usize = 2;
const INTER_COL_SPACES: usize = 3;
const TYPE_MIN_WIDTH: usize = 8;
const ENTITY_MIN_WIDTH: usize = 16;
const CHANGE_MIN_WIDTH: usize = 10;
const DELTA_MIN_WIDTH: usize = 11;
const TYPE_EXTRA_MAX: usize = 4;
const CHANGE_EXTRA_MAX: usize = 2;
const DELTA_EXTRA_MAX: usize = 2;
const DIFF_ADD_BG: Color = Color::Rgb(33, 58, 43);
const DIFF_REMOVE_BG: Color = Color::Rgb(74, 34, 29);
const DIFF_MODIFIED_BG: Color = Color::Rgb(58, 51, 25);
const DIFF_GUTTER_FG: Color = Color::Rgb(95, 95, 95);
const DIFF_HUNK_FG: Color = Color::Gray;

#[derive(Clone, Copy, Debug)]
struct ListColumnWidths {
    type_col: usize,
    entity_col: usize,
    change_col: usize,
    delta_col: usize,
}

#[derive(Clone, Debug)]
struct UnifiedRenderRow {
    kind: LineKind,
    old_number: Option<usize>,
    new_number: Option<usize>,
    sign: char,
    text: String,
}

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
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(frame.area());
    let widths = compute_list_column_widths(chunks[1].width);

    let columns = format!(
        "  {} {} {} {}",
        fit_cell("Type", widths.type_col),
        fit_cell("Entity", widths.entity_col),
        fit_cell("Change", widths.change_col),
        fit_cell("+/-", widths.delta_col),
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(app.list_header_command(), Style::default().fg(Color::Cyan)),
            Line::raw(""),
            Line::styled(columns, Style::default().fg(Color::DarkGray)),
        ]),
        chunks[0],
    );

    let mut items: Vec<ListItem<'_>> = Vec::new();
    let mut selectable_indices: Vec<usize> = Vec::new();
    let mut current_file: Option<&str> = None;

    for row in app.rows() {
        if current_file != Some(row.file_path.as_str()) {
            if !items.is_empty() {
                items.push(ListItem::new(Line::raw("")));
            }
            current_file = Some(row.file_path.as_str());
            items.push(ListItem::new(Line::styled(
                row.file_path.clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )));
        }

        let entity_index = selectable_indices.len();
        let marker = if entity_index == app.selected() {
            "▶"
        } else {
            " "
        };
        let (icon, tag, style) = match row.change.change_type {
            ChangeType::Added => ("⊕", "[added]", Style::default().fg(Color::Green)),
            ChangeType::Modified => {
                if row.change.structural_change == Some(false) {
                    ("~", "[cosmetic]", Style::default().fg(Color::DarkGray))
                } else {
                    ("∆", "[modified]", Style::default().fg(Color::Yellow))
                }
            }
            ChangeType::Deleted => ("⊖", "[deleted]", Style::default().fg(Color::Red)),
            ChangeType::Moved => ("→", "[moved]", Style::default().fg(Color::Blue)),
            ChangeType::Renamed => ("↻", "[renamed]", Style::default().fg(Color::Cyan)),
        };

        let spans = vec![
            Span::styled(format!("{marker}{icon}"), style),
            Span::styled(
                fit_cell(&row.entity_type, widths.type_col),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw(" "),
            Span::styled(
                fit_cell(&row.entity_name, widths.entity_col),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(fit_cell(tag, widths.change_col), style),
            Span::raw(" "),
        ];
        let mut spans = spans;
        append_delta_spans(
            &mut spans,
            row.added_lines,
            row.removed_lines,
            widths.delta_col,
        );

        selectable_indices.push(items.len());
        items.push(ListItem::new(Line::from(spans)));
    }

    let list = List::new(items)
        .block(Block::default().title("Entities").borders(Borders::ALL))
        .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White));

    let mut state = ListState::default();
    let selected = app
        .selected()
        .min(selectable_indices.len().saturating_sub(1));
    state.select(selectable_indices.get(selected).copied());
    frame.render_stateful_widget(list, chunks[1], &mut state);

    frame.render_widget(
        Paragraph::new("Controls: ↑/↓ j/k move, Enter open, g/G jump, ? help, q/Ctrl+c quit"),
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

    frame.render_widget(
        Paragraph::new(app.detail_title()).style(Style::default().fg(Color::Cyan)),
        chunks[0],
    );
    let selected_file_path = app
        .rows()
        .get(app.selected())
        .map(|row| row.file_path.as_str());
    let content_height = usize::from(chunks[1].height.saturating_sub(2)).max(1);
    let start = app.detail_scroll();

    match app.effective_view() {
        DiffView::Unified => {
            let rows = build_unified_render_rows(app.unified_lines());
            let number_width = line_number_width(
                rows.iter()
                    .flat_map(|row| [row.old_number, row.new_number])
                    .flatten()
                    .max(),
            );
            let end = (start + content_height).min(rows.len());
            let lines: Vec<Line<'_>> = rows[start..end]
                .iter()
                .map(|row| render_unified_row(row, number_width, selected_file_path))
                .collect();

            frame.render_widget(
                Paragraph::new(lines)
                    .block(Block::default().borders(Borders::ALL).title("Diff"))
                    .wrap(Wrap { trim: false }),
                chunks[1],
            );
        }
        DiffView::SideBySide => {
            let area = chunks[1];
            let line_width = area.width.saturating_sub(6) as usize;
            let half = (line_width / 2).max(20);
            let side_lines = app.side_by_side_lines();
            let end = (start + content_height).min(side_lines.len());
            let lines: Vec<Line<'_>> = side_lines[start..end]
                .iter()
                .map(|line| render_side_by_side_row(line, half, selected_file_path))
                .collect();

            frame.render_widget(
                Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Diff")),
                chunks[1],
            );
        }
    }

    let mut footer =
        "Controls: Esc list, ←/→ entity, Tab view, n/p hunks, PgUp/PgDn scroll, g/G top-bottom, ? help, q/Ctrl+c quit"
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
        Line::from("  Left/Right previous/next entity"),
        Line::from("  Tab toggle unified/side-by-side"),
        Line::from("  n/p next/previous hunk"),
        Line::from("  PageUp/PageDown scroll by page"),
        Line::from("  g/G jump top/bottom"),
        Line::from("Global:"),
        Line::from("  ? toggle help"),
        Line::from("  q or Ctrl+c quit"),
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

fn build_unified_render_rows(lines: &[(LineKind, String)]) -> Vec<UnifiedRenderRow> {
    let mut rows = Vec::with_capacity(lines.len());
    let mut old_line = 1usize;
    let mut new_line = 1usize;
    let mut has_hunk = false;

    for (kind, text) in lines {
        match kind {
            LineKind::Header => {
                if let Some((next_old, next_new)) = parse_hunk_header_starts(text) {
                    old_line = next_old;
                    new_line = next_new;
                    has_hunk = true;
                }

                rows.push(UnifiedRenderRow {
                    kind: *kind,
                    old_number: None,
                    new_number: None,
                    sign: ' ',
                    text: text.clone(),
                });
            }
            LineKind::Added => {
                let number = if has_hunk {
                    let value = Some(new_line);
                    new_line = new_line.saturating_add(1);
                    value
                } else {
                    None
                };

                rows.push(UnifiedRenderRow {
                    kind: *kind,
                    old_number: None,
                    new_number: number,
                    sign: '+',
                    text: strip_line_prefix(text, "+ "),
                });
            }
            LineKind::Removed => {
                let number = if has_hunk {
                    let value = Some(old_line);
                    old_line = old_line.saturating_add(1);
                    value
                } else {
                    None
                };

                rows.push(UnifiedRenderRow {
                    kind: *kind,
                    old_number: number,
                    new_number: None,
                    sign: '-',
                    text: strip_line_prefix(text, "- "),
                });
            }
            LineKind::Unchanged => {
                let old_number = if has_hunk { Some(old_line) } else { None };
                let new_number = if has_hunk { Some(new_line) } else { None };
                if has_hunk {
                    old_line = old_line.saturating_add(1);
                    new_line = new_line.saturating_add(1);
                }

                rows.push(UnifiedRenderRow {
                    kind: *kind,
                    old_number,
                    new_number,
                    sign: ' ',
                    text: strip_line_prefix(text, "  "),
                });
            }
            LineKind::Modified => {
                let old_number = if has_hunk { Some(old_line) } else { None };
                let new_number = if has_hunk { Some(new_line) } else { None };
                if has_hunk {
                    old_line = old_line.saturating_add(1);
                    new_line = new_line.saturating_add(1);
                }

                rows.push(UnifiedRenderRow {
                    kind: *kind,
                    old_number,
                    new_number,
                    sign: '~',
                    text: text.clone(),
                });
            }
        }
    }

    rows
}

fn render_unified_row(
    row: &UnifiedRenderRow,
    number_width: usize,
    file_path: Option<&str>,
) -> Line<'static> {
    if row.kind == LineKind::Header {
        return Line::styled(
            row.text.clone(),
            Style::default()
                .fg(DIFF_HUNK_FG)
                .add_modifier(Modifier::BOLD),
        );
    }

    let (sign_style, content_style, line_style) = match row.kind {
        LineKind::Added => (
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(Color::Green),
            Style::default().bg(DIFF_ADD_BG),
        ),
        LineKind::Removed => (
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            Style::default().fg(Color::Red),
            Style::default().bg(DIFF_REMOVE_BG),
        ),
        LineKind::Modified => (
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(Color::Yellow),
            Style::default().bg(DIFF_MODIFIED_BG),
        ),
        LineKind::Unchanged => (
            Style::default().fg(Color::DarkGray),
            Style::default(),
            Style::default(),
        ),
        LineKind::Header => unreachable!("header rows are handled above"),
    };

    let line_number = if row.kind == LineKind::Removed {
        row.old_number
    } else {
        row.new_number.or(row.old_number)
    };
    let number = line_number.map_or_else(
        || " ".repeat(number_width),
        |value| format!("{value:>number_width$}"),
    );
    let content_spans = highlight_text_spans(file_path, &row.text, row.kind)
        .unwrap_or_else(|| vec![Span::styled(row.text.clone(), content_style)]);

    let mut spans = vec![
        Span::styled(number, Style::default().fg(DIFF_GUTTER_FG)),
        Span::raw(" "),
        Span::styled(format!("{} ", row.sign), sign_style),
    ];
    spans.extend(content_spans);
    Line::from(spans).style(line_style)
}

fn parse_hunk_header_starts(line: &str) -> Option<(usize, usize)> {
    let mut parts = line.split_whitespace();
    if parts.next()? != "@@" {
        return None;
    }

    let old = parts.next()?;
    let new = parts.next()?;
    Some((parse_hunk_start(old, '-')?, parse_hunk_start(new, '+')?))
}

fn parse_hunk_start(token: &str, prefix: char) -> Option<usize> {
    let value = token.strip_prefix(prefix)?;
    let start = value.split(',').next()?;
    start.parse::<usize>().ok()
}

fn strip_line_prefix(line: &str, prefix: &str) -> String {
    line.strip_prefix(prefix).unwrap_or(line).to_string()
}

fn line_number_width(max_value: Option<usize>) -> usize {
    max_value.unwrap_or(1).to_string().len()
}

fn diff_styles_for_kind(kind: LineKind) -> (char, Style, Style, Style) {
    match kind {
        LineKind::Header => (
            ' ',
            Style::default().fg(DIFF_HUNK_FG),
            Style::default()
                .fg(DIFF_HUNK_FG)
                .add_modifier(Modifier::BOLD),
            Style::default(),
        ),
        LineKind::Added => (
            '+',
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(Color::Green),
            Style::default().bg(DIFF_ADD_BG),
        ),
        LineKind::Removed => (
            '-',
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            Style::default().fg(Color::Red),
            Style::default().bg(DIFF_REMOVE_BG),
        ),
        LineKind::Modified => (
            '~',
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(Color::Yellow),
            Style::default().bg(DIFF_MODIFIED_BG),
        ),
        LineKind::Unchanged => (
            ' ',
            Style::default().fg(Color::DarkGray),
            Style::default(),
            Style::default(),
        ),
    }
}

fn render_side_by_side_row(
    line: &super::detail::SideBySideLine,
    half_width: usize,
    file_path: Option<&str>,
) -> Line<'static> {
    let (sign, sign_style, content_style, line_style) = diff_styles_for_kind(line.kind);
    if line.kind == LineKind::Header {
        return Line::from(vec![
            Span::styled(format!("{sign} "), sign_style),
            Span::styled(
                format_column(line.left_number, &line.left_text, half_width),
                content_style,
            ),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_column(line.right_number, &line.right_text, half_width),
                content_style,
            ),
        ])
        .style(line_style);
    }

    let mut spans = vec![Span::styled(format!("{sign} "), sign_style)];
    spans.extend(render_side_column(
        line.left_number,
        &line.left_text,
        half_width,
        file_path,
        line.kind,
        content_style,
    ));
    spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
    spans.extend(render_side_column(
        line.right_number,
        &line.right_text,
        half_width,
        file_path,
        line.kind,
        content_style,
    ));
    Line::from(spans).style(line_style)
}

fn render_side_column(
    number: Option<usize>,
    text: &str,
    width: usize,
    file_path: Option<&str>,
    kind: LineKind,
    fallback_style: Style,
) -> Vec<Span<'static>> {
    let number_text = number.map_or_else(|| "    ".to_string(), |value| format!("{value:>4}"));
    let available = width.saturating_sub(number_text.len() + 1);
    let truncated = if text.chars().count() > available {
        let keep = available.saturating_sub(1);
        let clipped: String = text.chars().take(keep).collect();
        format!("{clipped}…")
    } else {
        text.to_string()
    };

    let mut spans = vec![Span::styled(
        format!("{number_text} "),
        Style::default().fg(DIFF_GUTTER_FG),
    )];
    spans.extend(
        highlight_text_spans(file_path, &truncated, kind)
            .unwrap_or_else(|| vec![Span::styled(truncated.clone(), fallback_style)]),
    );
    let pad = available.saturating_sub(truncated.chars().count());
    if pad > 0 {
        spans.push(Span::raw(" ".repeat(pad)));
    }
    spans
}

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static SYNTAX_THEME: OnceLock<Theme> = OnceLock::new();
static HIGHLIGHT_CACHE: OnceLock<Mutex<HashMap<HighlightCacheKey, Vec<Span<'static>>>>> =
    OnceLock::new();
static SYNTAX_PREWARM_STARTED: OnceLock<()> = OnceLock::new();
static SYNTAX_READY: AtomicBool = AtomicBool::new(false);
const HIGHLIGHT_CACHE_LIMIT: usize = 4096;

#[derive(Clone, Eq)]
struct HighlightCacheKey {
    extension: Option<String>,
    text: String,
}

impl PartialEq for HighlightCacheKey {
    fn eq(&self, other: &Self) -> bool {
        self.extension == other.extension && self.text == other.text
    }
}

impl Hash for HighlightCacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.extension.hash(state);
        self.text.hash(state);
    }
}

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn syntax_theme() -> &'static Theme {
    SYNTAX_THEME.get_or_init(|| {
        let themes = ThemeSet::load_defaults();
        themes
            .themes
            .get("base16-ocean.dark")
            .cloned()
            .or_else(|| themes.themes.values().next().cloned())
            .expect("syntect should load at least one default theme")
    })
}

fn highlight_cache() -> &'static Mutex<HashMap<HighlightCacheKey, Vec<Span<'static>>>> {
    HIGHLIGHT_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_highlight_cache() -> MutexGuard<'static, HashMap<HighlightCacheKey, Vec<Span<'static>>>> {
    match highlight_cache().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn syntax_extension(file_path: Option<&str>) -> Option<String> {
    file_path.and_then(|path| {
        Path::new(path)
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_string)
    })
}

fn syntax_for_extension<'a>(set: &'a SyntaxSet, extension: Option<&str>) -> &'a SyntaxReference {
    if let Some(ext) = extension {
        // Markdown grammars can be noticeably slower on first highlight;
        // keep TUI responsive by treating them as plain text.
        if matches!(ext, "md" | "markdown" | "mdx") {
            return set.find_syntax_plain_text();
        }
        if let Some(syntax) = set.find_syntax_by_extension(ext) {
            return syntax;
        }
    }

    set.find_syntax_plain_text()
}

fn highlight_text_spans(
    file_path: Option<&str>,
    text: &str,
    kind: LineKind,
) -> Option<Vec<Span<'static>>> {
    if !SYNTAX_READY.load(Ordering::Acquire) {
        return None;
    }

    if matches!(kind, LineKind::Header | LineKind::Unchanged) {
        return None;
    }

    if text.is_empty() {
        return Some(vec![Span::raw(String::new())]);
    }

    let extension = syntax_extension(file_path);
    let cache_key = HighlightCacheKey {
        extension: extension.clone(),
        text: text.to_string(),
    };
    if let Some(cached) = lock_highlight_cache().get(&cache_key).cloned() {
        return Some(apply_kind_overlay(cached, kind));
    }

    let set = syntax_set();
    let syntax = syntax_for_extension(set, extension.as_deref());
    let mut highlighter = HighlightLines::new(syntax, syntax_theme());
    let highlighted = highlighter.highlight_line(text, set).ok()?;
    let spans: Vec<Span<'static>> = highlighted
        .into_iter()
        .map(|(style, segment)| Span::styled(segment.to_string(), syntect_style_to_ratatui(style)))
        .collect();

    {
        let mut cache = lock_highlight_cache();
        if cache.len() >= HIGHLIGHT_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(cache_key, spans.clone());
    }

    Some(apply_kind_overlay(spans, kind))
}

fn apply_kind_overlay(spans: Vec<Span<'static>>, kind: LineKind) -> Vec<Span<'static>> {
    if kind != LineKind::Removed {
        return spans;
    }

    spans
        .into_iter()
        .map(|span| {
            Span::styled(
                span.content.into_owned(),
                span.style.add_modifier(Modifier::DIM),
            )
        })
        .collect()
}

fn syntect_style_to_ratatui(style: syntect::highlighting::Style) -> Style {
    let mut rt_style = Style::default().fg(Color::Rgb(
        style.foreground.r,
        style.foreground.g,
        style.foreground.b,
    ));
    if style.font_style.contains(FontStyle::BOLD) {
        rt_style = rt_style.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        rt_style = rt_style.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        rt_style = rt_style.add_modifier(Modifier::UNDERLINED);
    }
    rt_style
}

pub(crate) fn prewarm_syntax_highlighting_async() {
    if SYNTAX_PREWARM_STARTED.set(()).is_err() {
        return;
    }

    std::thread::spawn(|| {
        let _ = syntax_set();
        let _ = syntax_theme();
        SYNTAX_READY.store(true, Ordering::Release);
        let _ = highlight_text_spans(Some("prewarm.rs"), "fn warm() {}", LineKind::Added);
    });
}

fn fit_cell(text: &str, width: usize) -> String {
    let clipped = truncate_cell(text, width);
    format!("{clipped:width$}")
}

fn fit_cell_right(text: &str, width: usize) -> String {
    let clipped = truncate_cell(text, width);
    format!("{clipped:>width$}")
}

fn truncate_cell(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    if text.chars().count() <= width {
        return text.to_string();
    }

    if width == 1 {
        return "…".to_string();
    }

    let keep = width.saturating_sub(1);
    let clipped: String = text.chars().take(keep).collect();
    format!("{clipped}…")
}

fn append_delta_spans(spans: &mut Vec<Span<'_>>, added: usize, removed: usize, width: usize) {
    if width == 0 {
        return;
    }

    if width == 1 {
        spans.push(Span::styled(
            fit_cell_right(&format!("+{added}"), 1),
            Style::default().fg(Color::Green),
        ));
        return;
    }

    if width == 2 {
        spans.push(Span::styled(
            fit_cell_right(&format!("+{added}"), 1),
            Style::default().fg(Color::Green),
        ));
        spans.push(Span::styled(
            fit_cell(&format!("-{removed}"), 1),
            Style::default().fg(Color::Red),
        ));
        return;
    }

    let remaining = width.saturating_sub(1);
    let plus_width = remaining / 2;
    let minus_width = remaining.saturating_sub(plus_width);
    spans.push(Span::styled(
        fit_cell_right(&format!("+{added}"), plus_width),
        Style::default().fg(Color::Green),
    ));
    spans.push(Span::raw("/"));
    spans.push(Span::styled(
        fit_cell(&format!("-{removed}"), minus_width),
        Style::default().fg(Color::Red),
    ));
}

fn compute_list_column_widths(list_area_width: u16) -> ListColumnWidths {
    let content_width = usize::from(list_area_width.saturating_sub(4));
    let available = content_width.saturating_sub(ICON_COL_WIDTH + INTER_COL_SPACES);
    let minimum_total = TYPE_MIN_WIDTH + ENTITY_MIN_WIDTH + CHANGE_MIN_WIDTH + DELTA_MIN_WIDTH;

    let mut type_col = TYPE_MIN_WIDTH;
    let mut entity_col = ENTITY_MIN_WIDTH;
    let mut change_col = CHANGE_MIN_WIDTH;
    let mut delta_col = DELTA_MIN_WIDTH;

    if available >= minimum_total {
        let mut extra = available - minimum_total;

        let type_extra = extra.min(TYPE_EXTRA_MAX);
        type_col += type_extra;
        extra -= type_extra;

        let change_extra = extra.min(CHANGE_EXTRA_MAX);
        change_col += change_extra;
        extra -= change_extra;

        let delta_extra = extra.min(DELTA_EXTRA_MAX);
        delta_col += delta_extra;
        extra -= delta_extra;

        entity_col += extra;
    } else {
        let mut overflow = minimum_total - available;
        while overflow > 0 {
            let before = overflow;

            if entity_col > 0 {
                entity_col -= 1;
                overflow -= 1;
            }
            if overflow == 0 {
                break;
            }

            if type_col > 0 {
                type_col -= 1;
                overflow -= 1;
            }
            if overflow == 0 {
                break;
            }

            if change_col > 0 {
                change_col -= 1;
                overflow -= 1;
            }
            if overflow == 0 {
                break;
            }

            if delta_col > 0 {
                delta_col -= 1;
                overflow -= 1;
            }
            if overflow == before {
                break;
            }
        }
    }

    ListColumnWidths {
        type_col,
        entity_col,
        change_col,
        delta_col,
    }
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

    #[test]
    fn list_column_widths_use_full_available_content_width() {
        let widths = compute_list_column_widths(120);
        let content_width = usize::from(120_u16.saturating_sub(4));
        let used = ICON_COL_WIDTH
            + INTER_COL_SPACES
            + widths.type_col
            + widths.entity_col
            + widths.change_col
            + widths.delta_col;
        assert_eq!(used, content_width);
        assert!(widths.entity_col > ENTITY_MIN_WIDTH);
    }

    #[test]
    fn list_column_widths_shrink_safely_for_narrow_layouts() {
        let widths = compute_list_column_widths(40);
        let content_width = usize::from(40_u16.saturating_sub(4));
        let used = ICON_COL_WIDTH
            + INTER_COL_SPACES
            + widths.type_col
            + widths.entity_col
            + widths.change_col
            + widths.delta_col;
        assert_eq!(used, content_width);
    }

    #[test]
    fn parse_hunk_header_starts_extracts_old_and_new_starts() {
        assert_eq!(
            parse_hunk_header_starts("@@ -12,3 +40,8 @@"),
            Some((12, 40))
        );
        assert_eq!(parse_hunk_header_starts("@@ -1 +2 @@"), Some((1, 2)));
        assert_eq!(parse_hunk_header_starts("not a hunk"), None);
        assert_eq!(parse_hunk_header_starts("@@ -x,3 +2,1 @@"), None);
    }

    #[test]
    fn build_unified_render_rows_assigns_expected_line_numbers() {
        let lines = vec![
            (LineKind::Header, "@@ -10,2 +20,3 @@".to_string()),
            (LineKind::Unchanged, "  keep".to_string()),
            (LineKind::Removed, "- old".to_string()),
            (LineKind::Added, "+ new".to_string()),
            (LineKind::Added, "+ extra".to_string()),
        ];
        let rows = build_unified_render_rows(&lines);

        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].old_number, None);
        assert_eq!(rows[0].new_number, None);
        assert_eq!(rows[1].old_number, Some(10));
        assert_eq!(rows[1].new_number, Some(20));
        assert_eq!(rows[2].old_number, Some(11));
        assert_eq!(rows[2].new_number, None);
        assert_eq!(rows[3].old_number, None);
        assert_eq!(rows[3].new_number, Some(21));
        assert_eq!(rows[4].old_number, None);
        assert_eq!(rows[4].new_number, Some(22));
    }

    #[test]
    fn apply_kind_overlay_dims_removed_only() {
        let base = vec![Span::styled(
            "line".to_string(),
            Style::default().fg(Color::Red),
        )];

        let removed = apply_kind_overlay(base.clone(), LineKind::Removed);
        assert!(removed[0].style.add_modifier.contains(Modifier::DIM));

        let unchanged = apply_kind_overlay(base, LineKind::Unchanged);
        assert!(!unchanged[0].style.add_modifier.contains(Modifier::DIM));
    }
}
