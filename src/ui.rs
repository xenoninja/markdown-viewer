use ratatui::Frame;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;

use crate::app::ReadingSession;
use crate::layout;

pub fn render(frame: &mut Frame<'_>, session: &ReadingSession) {
    let rendered = layout::layout(session.document(), frame.area().width);
    let cursor = session.cursor();
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
            spans.extend(row.cells().iter().map(|cell| {
                let mut style = cell_style(cell.style());
                if Some(cell.position()) == cursor {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                Span::styled(cell.symbol().to_owned(), style)
            }));
            Line::from(spans)
        })
        .collect::<Vec<_>>();

    frame.render_widget(Paragraph::new(Text::from(lines)), frame.area());
}

fn cell_style(semantic: layout::CellStyle) -> Style {
    let mut modifiers = Modifier::empty();
    modifiers |= match semantic.heading_level() {
        Some(1) => Modifier::BOLD | Modifier::UNDERLINED,
        Some(2) => Modifier::BOLD,
        Some(3) => Modifier::UNDERLINED,
        Some(4) => Modifier::ITALIC,
        Some(5) => Modifier::DIM,
        Some(6) => Modifier::DIM | Modifier::ITALIC,
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
        modifiers |= Modifier::REVERSED;
    }
    if semantic.is_link() {
        modifiers |= Modifier::UNDERLINED;
    }
    if semantic.is_thematic_break() {
        modifiers |= Modifier::DIM;
    }
    Style::new().add_modifier(modifiers)
}
