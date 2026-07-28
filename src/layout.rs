use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{Block, Document};

const MAX_PROSE_WIDTH: usize = 100;

/// Width-independent location in the semantic Document.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SemanticPosition {
    pub block: usize,
    pub grapheme: usize,
}

/// The terminal location occupied by one semantic grapheme.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CellLocation {
    pub row: usize,
    pub column: usize,
    pub width: usize,
    pub position: SemanticPosition,
}

/// One piece of semantic content placed into a rendered row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedCell {
    symbol: String,
    position: SemanticPosition,
    width: usize,
}

impl RenderedCell {
    #[must_use]
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    #[must_use]
    pub fn position(&self) -> SemanticPosition {
        self.position
    }

    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    #[must_use]
    pub fn is_navigable(&self) -> bool {
        !self.symbol.chars().all(char::is_whitespace)
    }
}

/// A rendered row whose cells retain their semantic cursor mappings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedRow {
    cells: Vec<RenderedCell>,
    column: usize,
}

impl RenderedRow {
    #[must_use]
    pub fn cells(&self) -> &[RenderedCell] {
        &self.cells
    }

    #[must_use]
    pub fn text(&self) -> String {
        self.cells()
            .iter()
            .map(RenderedCell::symbol)
            .collect::<String>()
    }

    #[must_use]
    pub fn column(&self) -> usize {
        self.column
    }

    #[must_use]
    pub fn display_width(&self) -> usize {
        cells_width(&self.cells)
    }
}

/// Terminal rows produced by mdview's custom layout boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedDocument {
    rows: Vec<RenderedRow>,
    content_width: usize,
}

impl RenderedDocument {
    #[must_use]
    pub fn rows(&self) -> &[RenderedRow] {
        &self.rows
    }

    #[must_use]
    pub fn content_width(&self) -> usize {
        self.content_width
    }

    #[must_use]
    pub fn cell_for_position(&self, position: SemanticPosition) -> Option<CellLocation> {
        self.rows.iter().enumerate().find_map(|(row_index, row)| {
            let mut column = row.column;
            for cell in &row.cells {
                if cell.position == position {
                    return Some(CellLocation {
                        row: row_index,
                        column,
                        width: cell.width,
                        position,
                    });
                }
                column += cell.width;
            }
            None
        })
    }

    #[must_use]
    pub fn position_at(&self, row: usize, column: usize) -> Option<SemanticPosition> {
        let row = self.rows.get(row)?;
        let mut cell_column = row.column;
        for cell in &row.cells {
            let width = cell.width.max(1);
            if (cell_column..cell_column + width).contains(&column) {
                return Some(cell.position);
            }
            cell_column += cell.width;
        }
        None
    }

    #[must_use]
    pub fn first_position(&self) -> Option<SemanticPosition> {
        self.rows
            .iter()
            .flat_map(|row| &row.cells)
            .find(|cell| cell.is_navigable())
            .map(RenderedCell::position)
    }

    #[must_use]
    pub fn last_position(&self) -> Option<SemanticPosition> {
        self.rows
            .iter()
            .rev()
            .flat_map(|row| row.cells.iter().rev())
            .find(|cell| cell.is_navigable())
            .map(RenderedCell::position)
    }

    #[must_use]
    pub fn row_for_position(&self, position: SemanticPosition) -> Option<usize> {
        self.cell_for_position(position).map(|cell| cell.row)
    }

    #[must_use]
    pub fn nearest_position(&self, row: usize, column: usize) -> Option<SemanticPosition> {
        let row = self.rows.get(row)?;
        let mut cell_column = row.column;
        let mut nearest = None;
        for cell in &row.cells {
            if cell.is_navigable() {
                let candidate = (cell_column.abs_diff(column), cell.position);
                if nearest.is_none_or(|current| candidate < current) {
                    nearest = Some(candidate);
                }
            }
            cell_column += cell.width;
        }
        nearest.map(|(_, position)| position)
    }
}

#[must_use]
pub fn layout(document: &Document, width: u16) -> RenderedDocument {
    let pane_width = usize::from(width.max(1));
    let content_width = pane_width.min(MAX_PROSE_WIDTH);
    let column = pane_width.saturating_sub(content_width) / 2;
    let mut rows = Vec::new();

    for (block_index, block) in document.blocks().iter().enumerate() {
        match block {
            Block::Paragraph(text) | Block::RawHtml(text) => {
                wrap_block(text, block_index, content_width, column, &mut rows);
            }
        }
    }

    RenderedDocument {
        rows,
        content_width,
    }
}

fn wrap_block(text: &str, block: usize, width: usize, column: usize, rows: &mut Vec<RenderedRow>) {
    let mut row = Vec::new();
    let mut word = Vec::new();
    let mut separator = None;

    for (grapheme, symbol) in text.graphemes(true).enumerate() {
        let display_symbol =
            if UnicodeWidthStr::width(symbol) == 0 && !symbol.chars().all(char::is_whitespace) {
                format!("◌{symbol}")
            } else {
                symbol.to_owned()
            };
        let cell = RenderedCell {
            width: UnicodeWidthStr::width(display_symbol.as_str()),
            symbol: display_symbol,
            position: SemanticPosition { block, grapheme },
        };
        if symbol.chars().all(char::is_whitespace) {
            flush_word(&mut word, &mut separator, &mut row, width, column, rows);
            separator.get_or_insert(cell);
        } else {
            word.push(cell);
        }
    }

    flush_word(&mut word, &mut separator, &mut row, width, column, rows);
    if !row.is_empty() {
        rows.push(RenderedRow { cells: row, column });
    }
}

fn flush_word(
    word: &mut Vec<RenderedCell>,
    separator: &mut Option<RenderedCell>,
    row: &mut Vec<RenderedCell>,
    width: usize,
    column: usize,
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
            column,
        });
    }

    if word_width <= width {
        if !row.is_empty() {
            let mut separator = separator.take().expect("words have separating whitespace");
            separator.symbol = " ".to_owned();
            separator.width = 1;
            row.push(separator);
        }
        row.append(word);
    } else {
        if !row.is_empty() {
            rows.push(RenderedRow {
                cells: std::mem::take(row),
                column,
            });
        }
        for cell in word.drain(..) {
            if !row.is_empty() && cells_width(row) + cell.width > width {
                rows.push(RenderedRow {
                    cells: std::mem::take(row),
                    column,
                });
            }
            row.push(cell);
        }
    }

    *separator = None;
}

fn cells_width(cells: &[RenderedCell]) -> usize {
    cells.iter().map(|cell| cell.width).sum()
}
