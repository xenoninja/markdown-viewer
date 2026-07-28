use unicode_width::UnicodeWidthChar;

use crate::{Block, Document};

/// Width-independent location for a cell in the semantic Document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SemanticPosition {
    pub block: usize,
    pub character: usize,
}

/// One piece of semantic content placed into a terminal row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedCell {
    symbol: char,
    position: SemanticPosition,
}

impl RenderedCell {
    fn width(&self) -> usize {
        self.symbol.width().unwrap_or_default()
    }
}

/// A rendered row whose cells retain their semantic cursor mappings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedRow {
    cells: Vec<RenderedCell>,
}

impl RenderedRow {
    #[must_use]
    pub fn cells(&self) -> &[RenderedCell] {
        &self.cells
    }

    #[must_use]
    pub fn text(&self) -> String {
        self.cells().iter().map(|cell| cell.symbol).collect()
    }
}

/// Terminal rows produced by mdview's custom layout boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedDocument {
    rows: Vec<RenderedRow>,
}

impl RenderedDocument {
    #[must_use]
    pub fn rows(&self) -> &[RenderedRow] {
        &self.rows
    }
}

#[must_use]
pub fn layout(document: &Document, width: u16) -> RenderedDocument {
    let width = usize::from(width.max(1));
    let mut rows = Vec::new();

    for (block_index, block) in document.blocks().iter().enumerate() {
        match block {
            Block::Paragraph(text) | Block::RawHtml(text) => {
                wrap_block(text, block_index, width, &mut rows);
            }
        }
    }

    RenderedDocument { rows }
}

fn wrap_block(text: &str, block: usize, width: usize, rows: &mut Vec<RenderedRow>) {
    let mut row = Vec::new();
    let mut word = Vec::new();
    let mut separator = None;

    for (character, symbol) in text.chars().enumerate() {
        let cell = RenderedCell {
            symbol,
            position: SemanticPosition { block, character },
        };
        if symbol.is_whitespace() {
            flush_word(&mut word, &mut separator, &mut row, width, rows);
            separator.get_or_insert(cell);
        } else {
            word.push(cell);
        }
    }

    flush_word(&mut word, &mut separator, &mut row, width, rows);
    if !row.is_empty() {
        rows.push(RenderedRow { cells: row });
    }
}

fn flush_word(
    word: &mut Vec<RenderedCell>,
    separator: &mut Option<RenderedCell>,
    row: &mut Vec<RenderedCell>,
    width: usize,
    rows: &mut Vec<RenderedRow>,
) {
    if word.is_empty() {
        return;
    }

    let word_width = cells_width(word);
    let separator_width = usize::from(!row.is_empty());
    if !row.is_empty() && cells_width(row) + separator_width + word_width > width {
        rows.push(RenderedRow {
            cells: std::mem::take(row),
        });
    }

    if word_width <= width {
        if !row.is_empty() {
            let mut separator = separator.take().expect("words have separating whitespace");
            separator.symbol = ' ';
            row.push(separator);
        }
        row.append(word);
    } else {
        if !row.is_empty() {
            rows.push(RenderedRow {
                cells: std::mem::take(row),
            });
        }
        for cell in word.drain(..) {
            if !row.is_empty() && cells_width(row) + cell.width() > width {
                rows.push(RenderedRow {
                    cells: std::mem::take(row),
                });
            }
            row.push(cell);
        }
    }

    *separator = None;
}

fn cells_width(cells: &[RenderedCell]) -> usize {
    cells.iter().map(RenderedCell::width).sum()
}
