use std::path::Path;

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::Document;
use crate::source::{SourceError, load_document};
use crate::ui;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    Quit,
}

#[derive(Debug)]
pub struct ReadingSession {
    document: Document,
    quit: bool,
}

impl ReadingSession {
    #[must_use]
    pub fn new(document: Document) -> Self {
        Self {
            document,
            quit: false,
        }
    }

    pub fn command(&mut self, command: Command) {
        match command {
            Command::Quit => self.quit = true,
        }
    }

    #[must_use]
    pub fn has_quit(&self) -> bool {
        self.quit
    }

    #[must_use]
    pub fn document(&self) -> &Document {
        &self.document
    }
}

pub struct Harness {
    session: ReadingSession,
    terminal: Terminal<TestBackend>,
}

impl Harness {
    pub fn open(path: impl AsRef<Path>, width: u16, height: u16) -> Result<Self, SourceError> {
        let document = load_document(path)?;
        let backend = TestBackend::new(width, height);
        let terminal = Terminal::new(backend).expect("TestBackend is infallible");
        let mut harness = Self {
            session: ReadingSession::new(document),
            terminal,
        };
        harness.draw();
        Ok(harness)
    }

    pub fn command(&mut self, command: Command) {
        self.session.command(command);
    }

    #[must_use]
    pub fn has_quit(&self) -> bool {
        self.session.has_quit()
    }

    #[must_use]
    pub fn frame(&self) -> String {
        let buffer = self.terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn draw(&mut self) {
        self.terminal
            .draw(|frame| ui::render(frame, &self.session))
            .expect("TestBackend is infallible");
    }
}
