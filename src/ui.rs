use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::HeadingLevel;
use crate::app::{OutlineBranchState, PaneFocus, ReadingSession};
use crate::layout;

pub fn render(frame: &mut Frame<'_>, session: &ReadingSession) {
    let panes = session.pane_layout(frame.area().width);
    let rendered = session.rendered(frame.area().width);
    let cursor = session.cursor();
    let color_enabled = std::env::var_os("NO_COLOR").is_none();
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
            let blank_width = row.column().saturating_sub(row.leading_width());
            if blank_width > 0 {
                spans.push(Span::raw(" ".repeat(blank_width)));
            }
            if !row.leading().is_empty() {
                spans.push(Span::styled(
                    row.leading().to_owned(),
                    Style::new().add_modifier(Modifier::DIM),
                ));
            }
            if row.clipped_prefix_width() > 0 {
                spans.push(Span::raw(" ".repeat(row.clipped_prefix_width())));
            }
            spans.extend(row.visible_cells().map(|cell| {
                let mut style = cell_style(cell.style(), color_enabled);
                if cell.is_navigable() && Some(cell.position()) == cursor {
                    style = style.add_modifier(Modifier::REVERSED);
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
                }
                if session.focus() == PaneFocus::Outline
                    && Some(entry.position) == outline_selection
                {
                    style = style.add_modifier(Modifier::REVERSED);
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
    if let Some(status) = session.status_text() {
        let status_area = Rect::new(
            frame.area().x,
            frame.area().bottom().saturating_sub(1),
            frame.area().width,
            1,
        );
        frame.render_widget(
            Paragraph::new(status).style(Style::new().add_modifier(Modifier::BOLD)),
            status_area,
        );
    }
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

fn cell_style(semantic: layout::CellStyle, color_enabled: bool) -> Style {
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
