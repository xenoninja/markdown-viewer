use ratatui::Frame;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;

use crate::HeadingLevel;
use crate::app::ReadingSession;
use crate::layout;

pub fn render(frame: &mut Frame<'_>, session: &ReadingSession) {
    let rendered = session.rendered(frame.area().width);
    let cursor = session.cursor();
    let color_enabled = std::env::var_os("NO_COLOR").is_none();
    let lines = rendered
        .rows()
        .iter()
        .skip(session.viewport())
        .take(usize::from(frame.area().height))
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

    frame.render_widget(Paragraph::new(Text::from(lines)), frame.area());
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
