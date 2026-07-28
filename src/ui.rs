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
            if row.column() > 0 {
                spans.push(Span::raw(" ".repeat(row.column())));
            }
            spans.extend(row.cells().iter().map(|cell| {
                if Some(cell.position()) == cursor {
                    Span::styled(
                        cell.symbol().to_owned(),
                        Style::new().add_modifier(Modifier::REVERSED),
                    )
                } else {
                    Span::raw(cell.symbol().to_owned())
                }
            }));
            Line::from(spans)
        })
        .collect::<Vec<_>>();

    frame.render_widget(Paragraph::new(Text::from(lines)), frame.area());
}
