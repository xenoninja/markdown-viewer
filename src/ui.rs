use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::app::{
    MIN_TERMINAL_HEIGHT, MIN_TERMINAL_WIDTH, OutlineBranchState, PaneFocus, ReadingSession,
    StatusLevel,
};
use crate::layout;
use crate::{AlertKind, HeadingLevel};

pub fn render(frame: &mut Frame<'_>, session: &ReadingSession) {
    if session.terminal_too_small(frame.area().width, frame.area().height) {
        render_terminal_too_small(frame, session.color_enabled());
        return;
    }

    let panes = session.pane_layout(frame.area().width);
    let rendered = session.rendered(frame.area().width);
    let cursor = session.cursor();
    let color_enabled = session.color_enabled();
    let content_area = Rect::new(
        panes.document_x,
        frame.area().y,
        panes.document_width,
        session.content_height(frame.area().height),
    );
    let lines = rendered
        .rows()
        .iter()
        .skip(session.viewport())
        .take(usize::from(content_area.height))
        .map(|row| {
            let mut spans = Vec::with_capacity(row.cells().len() + 1);
            let alert = row.alert_kind();
            let blank_width = row.column().saturating_sub(row.leading_width());
            if blank_width > 0 {
                spans.push(Span::raw(" ".repeat(blank_width)));
            }
            if !row.leading().is_empty() {
                if let Some(query) = session.search_leading_query(row.block()) {
                    spans.extend(highlighted_leading(row.leading(), query));
                } else {
                    spans.push(Span::styled(
                        row.leading().to_owned(),
                        leading_style(alert, color_enabled),
                    ));
                }
            }
            if row.clipped_prefix_width() > 0 {
                spans.push(Span::raw(" ".repeat(row.clipped_prefix_width())));
            }
            spans.extend(row.visible_cells().map(|cell| {
                let mut style = cell_style(cell.style(), alert, color_enabled);
                if session.is_search_match(cell.position()) {
                    style = style.add_modifier(Modifier::UNDERLINED | Modifier::BOLD);
                    if color_enabled {
                        style = style.fg(Color::Yellow);
                    }
                }
                if cell.is_navigable() && session.is_selected(cell.position(), frame.area().width) {
                    style = style.add_modifier(Modifier::REVERSED);
                    if color_enabled {
                        style = style.bg(Color::DarkGray);
                    }
                }
                if cell.is_navigable() && Some(cell.position()) == cursor {
                    style = style.add_modifier(Modifier::REVERSED | Modifier::BOLD);
                    if color_enabled {
                        style = style.fg(Color::Black).bg(Color::Gray);
                    }
                }
                Span::styled(cell.symbol().to_owned(), style)
            }));
            Line::from(spans)
        })
        .collect::<Vec<_>>();

    frame.render_widget(Paragraph::new(Text::from(lines)), content_area);
    if panes.outline_width > 0 {
        let outline_area = Rect::new(
            frame.area().x,
            frame.area().y,
            panes.outline_width.saturating_add(1),
            session.content_height(frame.area().height),
        );
        let current_section = session.current_section();
        let outline_selection = session.outline_selection();
        let visible_outline = session.visible_outline_indices();
        let outline = visible_outline
            .into_iter()
            .skip(session.outline_viewport())
            .map(|index| {
                let entry = &session.outline()[index];
                let branch_marker = match session.outline_branch_state(index) {
                    OutlineBranchState::Leaf => "  ",
                    OutlineBranchState::Collapsed => "▸ ",
                    OutlineBranchState::Expanded => "▾ ",
                };
                let prefix = format!(
                    "{}{}",
                    "  ".repeat(entry.level.depth().saturating_sub(1)),
                    branch_marker
                );
                let mut style = Style::new();
                if Some(entry.position) == current_section {
                    style = style.add_modifier(Modifier::BOLD);
                    if color_enabled {
                        style = style.fg(Color::Cyan);
                    }
                }
                if session.focus() == PaneFocus::Outline
                    && Some(entry.position) == outline_selection
                {
                    style = style.add_modifier(Modifier::REVERSED);
                    if color_enabled {
                        style = style.fg(Color::Yellow);
                    }
                }
                Line::styled(
                    ellipsize_outline_label(&prefix, &entry.label, panes.outline_width),
                    style,
                )
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(outline).block(Block::new().borders(Borders::RIGHT)),
            outline_area,
        );
    }

    if session.viewer_chrome() {
        render_scrollbar(frame, session, panes.document_x + panes.document_width);
    }

    if let Some(status) = session.status_text_for_width(frame.area().width) {
        let status_area = Rect::new(
            frame.area().x,
            frame.area().bottom().saturating_sub(1),
            frame.area().width,
            1,
        );
        frame.render_widget(
            Paragraph::new(status).style(status_style(
                session.status_level(),
                session.color_enabled(),
            )),
            status_area,
        );
    }

    if session.help_open() {
        render_help(frame, session.color_enabled());
    }
}

fn highlighted_leading(leading: &str, query: &str) -> Vec<Span<'static>> {
    let plain = Style::new().add_modifier(Modifier::DIM);
    let matched = plain.add_modifier(Modifier::UNDERLINED);
    let mut spans = Vec::new();
    let mut offset = 0;
    for range in crate::search::literal_match_ranges(leading, query) {
        if offset < range.start {
            spans.push(Span::styled(leading[offset..range.start].to_owned(), plain));
        }
        spans.push(Span::styled(leading[range.clone()].to_owned(), matched));
        offset = range.end;
    }
    if offset < leading.len() {
        spans.push(Span::styled(leading[offset..].to_owned(), plain));
    }
    spans
}

fn ellipsize_outline_label(prefix: &str, label: &str, width: u16) -> String {
    let available = usize::from(width).saturating_sub(prefix.width());
    if label.width() <= available {
        return format!("{prefix}{label}");
    }
    if available == 0 {
        return prefix.to_owned();
    }

    let label_width = available.saturating_sub(1);
    let mut visible = String::new();
    let mut width = 0_usize;
    for grapheme in label.graphemes(true) {
        let grapheme_width = grapheme.width();
        if width.saturating_add(grapheme_width) > label_width {
            break;
        }
        visible.push_str(grapheme);
        width += grapheme_width;
    }
    format!("{prefix}{visible}…")
}

fn cell_style(semantic: layout::CellStyle, alert: Option<AlertKind>, color_enabled: bool) -> Style {
    let mut modifiers = Modifier::empty();
    modifiers |= match semantic.heading_level() {
        Some(HeadingLevel::H1) => Modifier::BOLD | Modifier::UNDERLINED,
        Some(HeadingLevel::H2) => Modifier::BOLD,
        Some(HeadingLevel::H3) => Modifier::UNDERLINED,
        Some(HeadingLevel::H4) => Modifier::ITALIC,
        Some(HeadingLevel::H5) => Modifier::DIM,
        Some(HeadingLevel::H6) => Modifier::DIM | Modifier::ITALIC,
        _ => Modifier::empty(),
    };
    if semantic.is_emphasis() {
        modifiers |= Modifier::ITALIC;
    }
    if semantic.is_strong() {
        modifiers |= Modifier::BOLD;
    }
    if semantic.is_strikethrough() {
        modifiers |= Modifier::CROSSED_OUT;
    }
    if semantic.is_inline_code() {
        modifiers |= Modifier::DIM | Modifier::UNDERLINED;
    }
    if semantic.is_link() {
        modifiers |= Modifier::UNDERLINED;
    }
    if semantic.is_thematic_break() {
        modifiers |= Modifier::DIM;
    }
    if semantic.is_table_header() {
        modifiers |= Modifier::BOLD;
    }
    let mut style = Style::new().add_modifier(modifiers);
    if color_enabled {
        if let Some(level) = semantic.heading_level() {
            style = style.fg(match level {
                HeadingLevel::H1 | HeadingLevel::H2 => Color::LightCyan,
                _ => Color::Cyan,
            });
        } else if semantic.is_link() {
            style = style.fg(Color::LightBlue);
        } else if let Some(alert) = alert {
            style = style.fg(alert_color(alert));
        }
    }
    if let Some(highlight) = semantic.highlight() {
        if highlight.is_bold() {
            style = style.add_modifier(Modifier::BOLD);
        }
        if highlight.is_italic() {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if highlight.is_underlined() {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        if color_enabled && let Some((red, green, blue)) = highlight.foreground() {
            style = style.fg(Color::Rgb(red, green, blue));
        }
    }
    style
}

fn leading_style(alert: Option<AlertKind>, color_enabled: bool) -> Style {
    let mut style = Style::new().add_modifier(Modifier::DIM);
    if let Some(alert) = alert {
        style = style.add_modifier(Modifier::BOLD);
        if color_enabled {
            style = style.fg(alert_color(alert));
        }
    }
    style
}

fn alert_color(alert: AlertKind) -> Color {
    match alert {
        AlertKind::Note => Color::LightBlue,
        AlertKind::Tip => Color::LightGreen,
        AlertKind::Important => Color::LightMagenta,
        AlertKind::Warning => Color::Yellow,
        AlertKind::Caution => Color::LightRed,
    }
}

fn status_style(level: StatusLevel, color_enabled: bool) -> Style {
    let mut style = Style::new().add_modifier(match level {
        StatusLevel::Normal | StatusLevel::Success => Modifier::BOLD,
        StatusLevel::Warning => Modifier::BOLD | Modifier::UNDERLINED,
        StatusLevel::Error => Modifier::BOLD | Modifier::REVERSED,
    });
    if color_enabled {
        style = style.fg(match level {
            StatusLevel::Normal => Color::Cyan,
            StatusLevel::Success => Color::Green,
            StatusLevel::Warning => Color::Yellow,
            StatusLevel::Error => Color::LightRed,
        });
    }
    style
}

fn render_scrollbar(frame: &mut Frame<'_>, session: &ReadingSession, column: u16) {
    let height = session.content_height(frame.area().height);
    if height == 0 || column >= frame.area().width {
        return;
    }
    let thumb = session
        .scrollbar_thumb_row(frame.area().width, height)
        .unwrap_or_default();
    let color_enabled = session.color_enabled();
    for row in 0..height {
        let symbol = if row == thumb { "█" } else { "│" };
        let mut style = Style::new().add_modifier(if row == thumb {
            Modifier::BOLD
        } else {
            Modifier::DIM
        });
        if color_enabled {
            style = style.fg(if row == thumb {
                Color::Cyan
            } else {
                Color::DarkGray
            });
        }
        frame.render_widget(
            Paragraph::new(symbol).style(style),
            Rect::new(column, row, 1, 1),
        );
    }
}

fn render_terminal_too_small(frame: &mut Frame<'_>, color_enabled: bool) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    let message = Text::from(vec![
        Line::from("Terminal too small"),
        Line::from(format!(
            "Resize to at least {MIN_TERMINAL_WIDTH} × {MIN_TERMINAL_HEIGHT}"
        )),
        Line::from(format!("Current: {} × {}", area.width, area.height)),
    ]);
    let message_area = centered(area, 34.min(area.width), 3.min(area.height));
    let mut style = Style::new().add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    if color_enabled {
        style = style.fg(Color::Yellow);
    }
    frame.render_widget(
        Paragraph::new(message).centered().style(style),
        message_area,
    );
}

fn render_help(frame: &mut Frame<'_>, color_enabled: bool) {
    let frame_area = frame.area();
    let width = if frame_area.width < 60 {
        frame_area.width
    } else {
        frame_area.width.saturating_sub(4).min(78)
    };
    let height = if frame_area.height < 14 {
        frame_area.height
    } else {
        frame_area.height.saturating_sub(2).min(18)
    };
    let area = centered(frame_area, width, height);
    frame.render_widget(Clear, area);
    let title_style = if color_enabled {
        Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::new().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    };
    let group_style = if color_enabled {
        Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::new().add_modifier(Modifier::BOLD)
    };
    let lines = [
        ("NAVIGATION", "h/j/k/l w/b 0/^/$ gg/G"),
        ("", "{/} count Ctrl-u/d/f/b/e/y"),
        ("OUTLINE", "o Ctrl-w h/l j/k h/l Enter"),
        ("SEARCH", "/ n/N Esc"),
        ("SELECTION", "v/V y Esc"),
        ("LINKS", "gx Ctrl-o Ctrl-i"),
        ("RELOAD", "r local Document"),
        ("APPLICATION", "? q Ctrl-c Esc"),
    ]
    .into_iter()
    .map(|(group, keys)| {
        Line::from(vec![
            Span::styled(format!("{group:12}"), group_style),
            Span::raw(keys),
        ])
    })
    .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::new()
                .title(Span::styled(" FIXED INTERACTIONS ", title_style))
                .borders(Borders::ALL),
        ),
        area,
    );
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}
