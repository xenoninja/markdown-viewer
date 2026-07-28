use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use unicode_segmentation::UnicodeSegmentation;

use crate::Document;
use crate::layout::{CellLocation, RenderedDocument, SemanticPosition, layout};
use crate::source::{SourceError, load_document};
use crate::ui;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    Quit,
}

#[derive(Debug)]
pub struct ReadingSession {
    document: Document,
    cursor: Option<SemanticPosition>,
    viewport: usize,
    preferred_column: Option<usize>,
    pending_count: Option<usize>,
    pending_g: bool,
    quit: bool,
}

impl ReadingSession {
    #[must_use]
    pub fn new(document: Document) -> Self {
        let cursor = layout(&document, 100).first_position();
        Self {
            document,
            cursor,
            viewport: 0,
            preferred_column: None,
            pending_count: None,
            pending_g: false,
            quit: false,
        }
    }

    pub fn command(&mut self, command: Command) {
        match command {
            Command::Quit => self.quit = true,
        }
    }

    pub fn key(&mut self, key: KeyEvent, width: u16, height: u16) {
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && let KeyCode::Char(character) = key.code
        {
            self.control(character, width, height);
            return;
        }

        let KeyCode::Char(character) = key.code else {
            self.clear_pending();
            return;
        };

        if character.is_ascii_digit() {
            if character == '0' && self.pending_count.is_none() {
                self.motion(width, height, Motion::RowStart, 1);
            } else {
                let digit = character.to_digit(10).expect("ASCII digit") as usize;
                self.pending_count = Some(
                    self.pending_count
                        .unwrap_or_default()
                        .saturating_mul(10)
                        .saturating_add(digit),
                );
            }
            return;
        }

        if self.pending_g {
            self.pending_g = false;
            if character == 'g' {
                let count = self.pending_count.take();
                self.document_row(width, height, count, false);
                return;
            }
        }

        if character == 'g' {
            self.pending_g = true;
            return;
        }

        let supplied_count = self.pending_count.take();
        let count = supplied_count.unwrap_or(1).max(1);
        match character {
            'q' => self.command(Command::Quit),
            'h' => self.motion(width, height, Motion::Left, count),
            'j' => self.motion(width, height, Motion::Down, count),
            'k' => self.motion(width, height, Motion::Up, count),
            'l' => self.motion(width, height, Motion::Right, count),
            'w' => self.motion(width, height, Motion::WordForward, count),
            'b' => self.motion(width, height, Motion::WordBackward, count),
            '^' => self.motion(width, height, Motion::FirstNonBlank, count),
            '$' => self.motion(width, height, Motion::RowEnd, count),
            'G' => self.document_row(width, height, supplied_count, true),
            '{' => self.motion(width, height, Motion::ParagraphBackward, count),
            '}' => self.motion(width, height, Motion::ParagraphForward, count),
            _ => self.clear_pending(),
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

    #[must_use]
    pub fn cursor(&self) -> Option<SemanticPosition> {
        self.cursor
    }

    #[must_use]
    pub fn viewport(&self) -> usize {
        self.viewport
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.ensure_cursor_visible(&layout(&self.document, width), height);
    }

    fn control(&mut self, character: char, width: u16, height: u16) {
        let count = self.pending_count.take().unwrap_or(1).max(1);
        self.pending_g = false;
        let page = usize::from(height.max(1));
        let half_page_distance = count.saturating_mul((page / 2).max(1));
        let page_distance = count.saturating_mul(page);
        let scroll_distance = isize::try_from(count).unwrap_or(isize::MAX);
        match character {
            'u' => self.motion(width, height, Motion::Up, half_page_distance),
            'd' => self.motion(width, height, Motion::Down, half_page_distance),
            'b' => self.motion(width, height, Motion::Up, page_distance),
            'f' => self.motion(width, height, Motion::Down, page_distance),
            'e' => self.scroll(&layout(&self.document, width), height, scroll_distance),
            'y' => self.scroll(&layout(&self.document, width), height, -scroll_distance),
            _ => {}
        }
    }

    fn motion(&mut self, width: u16, height: u16, motion: Motion, count: usize) {
        let rendered = layout(&self.document, width);
        let Some(cursor) = self.cursor else {
            return;
        };
        let Some(location) = rendered.cell_for_position(cursor) else {
            self.cursor = rendered.first_position();
            self.ensure_cursor_visible(&rendered, height);
            return;
        };

        let target = match motion {
            Motion::Left | Motion::Right => {
                self.preferred_column = None;
                horizontal_target(&rendered, location, motion, count)
            }
            Motion::Up | Motion::Down => {
                let column = *self.preferred_column.get_or_insert(location.column);
                let row = match motion {
                    Motion::Up => location.row.saturating_sub(count),
                    Motion::Down => location
                        .row
                        .saturating_add(count)
                        .min(rendered.rows().len().saturating_sub(1)),
                    _ => unreachable!(),
                };
                rendered.nearest_position(row, column)
            }
            Motion::WordForward | Motion::WordBackward => {
                self.preferred_column = None;
                word_target(&self.document, cursor, motion, count)
            }
            Motion::RowStart | Motion::FirstNonBlank => {
                self.preferred_column = None;
                let row = location
                    .row
                    .saturating_add(count.saturating_sub(1))
                    .min(rendered.rows().len().saturating_sub(1));
                rendered.rows()[row]
                    .cells()
                    .iter()
                    .find(|cell| cell.is_navigable())
                    .map(|cell| cell.position())
            }
            Motion::RowEnd => {
                self.preferred_column = None;
                let row = location
                    .row
                    .saturating_add(count.saturating_sub(1))
                    .min(rendered.rows().len().saturating_sub(1));
                rendered.rows()[row]
                    .cells()
                    .iter()
                    .rev()
                    .find(|cell| cell.is_navigable())
                    .map(|cell| cell.position())
            }
            Motion::ParagraphBackward | Motion::ParagraphForward => {
                self.preferred_column = None;
                paragraph_target(&rendered, cursor, motion, count)
            }
        };

        if let Some(target) = target {
            self.cursor = Some(target);
        }
        self.ensure_cursor_visible(&rendered, height);
    }

    fn document_row(&mut self, width: u16, height: u16, count: Option<usize>, end: bool) {
        let rendered = layout(&self.document, width);
        self.preferred_column = None;
        self.cursor = match count {
            Some(count) if count > 0 && (end || count > 1) => rendered
                .rows()
                .get(count.saturating_sub(1))
                .and_then(|row| row.cells().first())
                .filter(|cell| cell.is_navigable())
                .map(|cell| cell.position())
                .or_else(|| rendered.last_position()),
            _ if end => rendered.last_position(),
            _ => rendered.first_position(),
        };
        self.ensure_cursor_visible(&rendered, height);
    }

    fn scroll(&mut self, rendered: &RenderedDocument, height: u16, amount: isize) {
        let maximum = rendered
            .rows()
            .len()
            .saturating_sub(usize::from(height.max(1)));
        self.viewport = self.viewport.saturating_add_signed(amount).min(maximum);
    }

    fn ensure_cursor_visible(&mut self, rendered: &RenderedDocument, height: u16) {
        let height = usize::from(height.max(1));
        let maximum = rendered.rows().len().saturating_sub(height);
        self.viewport = self.viewport.min(maximum);
        let Some(row) = self
            .cursor
            .and_then(|cursor| rendered.row_for_position(cursor))
        else {
            return;
        };
        if row < self.viewport {
            self.viewport = row;
        } else if row >= self.viewport + height {
            self.viewport = row + 1 - height;
        }
    }

    fn clear_pending(&mut self) {
        self.pending_count = None;
        self.pending_g = false;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Motion {
    Left,
    Down,
    Up,
    Right,
    WordForward,
    WordBackward,
    RowStart,
    FirstNonBlank,
    RowEnd,
    ParagraphBackward,
    ParagraphForward,
}

fn horizontal_target(
    rendered: &RenderedDocument,
    location: CellLocation,
    motion: Motion,
    count: usize,
) -> Option<SemanticPosition> {
    let cells = rendered
        .rows()
        .iter()
        .flat_map(|row| row.cells())
        .filter(|cell| cell.is_navigable())
        .collect::<Vec<_>>();
    let current = cells
        .iter()
        .position(|cell| cell.position() == location.position)?;
    let target = match motion {
        Motion::Left => current.saturating_sub(count),
        Motion::Right => current
            .saturating_add(count)
            .min(cells.len().saturating_sub(1)),
        _ => unreachable!(),
    };
    cells.get(target).map(|cell| cell.position())
}

fn word_target(
    document: &Document,
    cursor: SemanticPosition,
    motion: Motion,
    count: usize,
) -> Option<SemanticPosition> {
    let graphemes = semantic_graphemes(document);
    let mut index = graphemes
        .iter()
        .position(|(position, _)| *position == cursor)?;

    for _ in 0..count {
        index = match motion {
            Motion::WordForward => next_word(&graphemes, index),
            Motion::WordBackward => previous_word(&graphemes, index),
            _ => unreachable!(),
        };
    }
    Some(graphemes[index].0)
}

fn semantic_graphemes(document: &Document) -> Vec<(SemanticPosition, &str)> {
    document
        .blocks()
        .iter()
        .enumerate()
        .flat_map(|(block, content)| {
            content
                .text()
                .graphemes(true)
                .enumerate()
                .map(move |(grapheme, text)| (SemanticPosition { block, grapheme }, text))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WordClass {
    Whitespace,
    Keyword,
    Punctuation,
}

fn word_class(grapheme: &str) -> WordClass {
    if grapheme.chars().all(char::is_whitespace) {
        WordClass::Whitespace
    } else if grapheme
        .chars()
        .any(|character| character.is_alphanumeric() || character == '_')
    {
        WordClass::Keyword
    } else {
        WordClass::Punctuation
    }
}

fn next_word(graphemes: &[(SemanticPosition, &str)], current: usize) -> usize {
    let mut index = current;
    let class = word_class(graphemes[index].1);

    while class != WordClass::Whitespace
        && index + 1 < graphemes.len()
        && graphemes[index].0.block == graphemes[index + 1].0.block
        && word_class(graphemes[index + 1].1) == class
    {
        index += 1;
    }
    if index + 1 < graphemes.len() {
        index += 1;
    }
    while index + 1 < graphemes.len() && word_class(graphemes[index].1) == WordClass::Whitespace {
        index += 1;
    }
    while index > 0 && word_class(graphemes[index].1) == WordClass::Whitespace {
        index -= 1;
    }
    index
}

fn previous_word(graphemes: &[(SemanticPosition, &str)], current: usize) -> usize {
    let mut index = current.saturating_sub(1);
    while index > 0 && word_class(graphemes[index].1) == WordClass::Whitespace {
        index -= 1;
    }
    let class = word_class(graphemes[index].1);
    while index > 0
        && graphemes[index].0.block == graphemes[index - 1].0.block
        && word_class(graphemes[index - 1].1) == class
    {
        index -= 1;
    }
    index
}

fn paragraph_target(
    rendered: &RenderedDocument,
    cursor: SemanticPosition,
    motion: Motion,
    count: usize,
) -> Option<SemanticPosition> {
    let starts = rendered
        .rows()
        .iter()
        .flat_map(|row| row.cells())
        .filter(|cell| cell.is_navigable())
        .map(|cell| cell.position())
        .fold(Vec::new(), |mut starts, position| {
            if starts
                .last()
                .is_none_or(|last: &SemanticPosition| last.block != position.block)
            {
                starts.push(position);
            }
            starts
        });
    let current = starts
        .iter()
        .position(|position| position.block == cursor.block)?;
    let target = match motion {
        Motion::ParagraphBackward => current.saturating_sub(count),
        Motion::ParagraphForward => current
            .saturating_add(count)
            .min(starts.len().saturating_sub(1)),
        _ => unreachable!(),
    };
    starts.get(target).copied()
}

pub struct Harness {
    session: ReadingSession,
    terminal: Terminal<TestBackend>,
}

impl Harness {
    #[must_use]
    pub fn new(document: Document, width: u16, height: u16) -> Self {
        let backend = TestBackend::new(width, height);
        let terminal = Terminal::new(backend).expect("TestBackend is infallible");
        let mut harness = Self {
            session: ReadingSession::new(document),
            terminal,
        };
        harness.draw();
        harness
    }

    pub fn open(path: impl AsRef<Path>, width: u16, height: u16) -> Result<Self, SourceError> {
        Ok(Self::new(load_document(path)?, width, height))
    }

    pub fn command(&mut self, command: Command) {
        self.session.command(command);
        self.draw();
    }

    pub fn keys(&mut self, keys: &str) {
        for character in keys.chars() {
            self.key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
        }
    }

    pub fn control(&mut self, character: char) {
        self.key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::CONTROL,
        ));
    }

    pub fn key(&mut self, key: KeyEvent) {
        let area = self.terminal.backend().buffer().area;
        self.session.key(key, area.width, area.height);
        self.draw();
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.terminal.backend_mut().resize(width, height);
        self.terminal
            .resize(Rect::new(0, 0, width, height))
            .expect("TestBackend is infallible");
        self.session.resize(width, height);
        self.draw();
    }

    #[must_use]
    pub fn has_quit(&self) -> bool {
        self.session.has_quit()
    }

    #[must_use]
    pub fn cursor(&self) -> Option<SemanticPosition> {
        self.session.cursor()
    }

    #[must_use]
    pub fn viewport(&self) -> usize {
        self.session.viewport()
    }

    #[must_use]
    pub fn cursor_cell(&self) -> Option<CellLocation> {
        self.screen_cell(self.session.cursor()?)
    }

    #[must_use]
    pub fn cursor_is_highlighted(&self) -> bool {
        self.cursor()
            .and_then(|cursor| self.modifier_at(cursor))
            .is_some_and(|modifier| modifier.contains(Modifier::REVERSED))
    }

    #[must_use]
    pub fn modifier_at(&self, position: SemanticPosition) -> Option<Modifier> {
        let location = self.screen_cell(position)?;
        let column = u16::try_from(location.column).ok()?;
        let row = u16::try_from(location.row).ok()?;
        Some(self.terminal.backend().buffer()[(column, row)].modifier)
    }

    fn screen_cell(&self, position: SemanticPosition) -> Option<CellLocation> {
        let area = self.terminal.backend().buffer().area;
        let mut location =
            layout(self.session.document(), area.width).cell_for_position(position)?;
        let screen_row = location.row.checked_sub(self.session.viewport())?;
        if screen_row >= usize::from(area.height) {
            return None;
        }
        location.row = screen_row;
        Some(location)
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
