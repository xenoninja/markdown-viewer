use ratatui::Frame;
use ratatui::text::{Line, Text};
use ratatui::widgets::Paragraph;

use crate::app::ReadingSession;
use crate::layout;

pub fn render(frame: &mut Frame<'_>, session: &ReadingSession) {
    let rendered = layout::layout(session.document(), frame.area().width);
    let lines = rendered
        .rows()
        .iter()
        .map(|row| Line::raw(row.text()))
        .collect::<Vec<_>>();

    frame.render_widget(Paragraph::new(Text::from(lines)), frame.area());
}
