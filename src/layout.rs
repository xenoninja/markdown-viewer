use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{Block, BlockKind, Document, HeadingLevel, InlineStyle, ListMarker};

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

/// Semantic presentation carried by a rendered cell.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CellStyle {
    heading_level: Option<HeadingLevel>,
    inline: InlineStyle,
    link: bool,
    thematic_break: bool,
}

impl CellStyle {
    #[must_use]
    pub fn heading_level(self) -> Option<HeadingLevel> {
        self.heading_level
    }

    #[must_use]
    pub fn is_emphasis(self) -> bool {
        self.inline.is_emphasis()
    }

    #[must_use]
    pub fn is_strong(self) -> bool {
        self.inline.is_strong()
    }

    #[must_use]
    pub fn is_strikethrough(self) -> bool {
        self.inline.is_strikethrough()
    }

    #[must_use]
    pub fn is_inline_code(self) -> bool {
        self.inline.is_inline_code()
    }

    #[must_use]
    pub fn is_link(self) -> bool {
        self.link
    }

    #[must_use]
    pub fn is_thematic_break(self) -> bool {
        self.thematic_break
    }

    fn from_semantics(kind: BlockKind, inline: InlineStyle, link: bool) -> Self {
        Self {
            heading_level: match kind {
                BlockKind::Heading(level) => Some(level),
                _ => None,
            },
            inline,
            link,
            thematic_break: kind == BlockKind::ThematicBreak,
        }
    }
}

/// One piece of semantic content placed into a rendered row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedCell {
    symbol: String,
    position: SemanticPosition,
    width: usize,
    style: CellStyle,
    link_target: Option<String>,
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
    pub fn style(&self) -> CellStyle {
        self.style
    }

    #[must_use]
    pub fn link_target(&self) -> Option<&str> {
        self.link_target.as_deref()
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
    leading: String,
}

impl RenderedRow {
    #[must_use]
    pub fn cells(&self) -> &[RenderedCell] {
        &self.cells
    }

    #[must_use]
    pub fn text(&self) -> String {
        let mut text = self.leading.clone();
        text.extend(self.cells().iter().map(RenderedCell::symbol));
        text
    }

    #[must_use]
    pub fn column(&self) -> usize {
        self.column
    }

    #[must_use]
    pub fn leading(&self) -> &str {
        &self.leading
    }

    #[must_use]
    pub fn leading_width(&self) -> usize {
        UnicodeWidthStr::width(self.leading.as_str())
    }

    #[must_use]
    pub fn display_width(&self) -> usize {
        self.leading_width() + cells_width(&self.cells)
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
    let base_column = pane_width.saturating_sub(content_width) / 2;
    let mut rows = Vec::new();

    for (block_index, block) in document.blocks().iter().enumerate() {
        let leading = block_leading(block);
        let leading_width = UnicodeWidthStr::width(leading.as_str());
        let block_width = content_width.saturating_sub(leading_width).max(1);
        if block.kind() == BlockKind::ThematicBreak {
            layout_thematic_break(
                block.kind(),
                block_index,
                block_width,
                base_column,
                leading,
                &mut rows,
            );
            continue;
        }
        if block.kind() == BlockKind::Empty {
            layout_empty_list_item(block, block_index, base_column, &mut rows);
            continue;
        }
        let column = base_column + leading_width;
        wrap_block(block, block_index, block_width, column, &leading, &mut rows);
    }

    RenderedDocument {
        rows,
        content_width,
    }
}

fn block_leading(block: &Block) -> String {
    let mut leading = "│ ".repeat(block.quote_depth());
    if let Some(crate::ListItem {
        depth,
        marker,
        continuation,
    }) = block.list_item()
    {
        leading.push_str(&"  ".repeat(depth.saturating_sub(1)));
        let marker = list_marker(marker);
        if continuation {
            leading.push_str(&" ".repeat(UnicodeWidthStr::width(marker.as_str())));
        } else {
            leading.push_str(&marker);
        }
    }
    leading
}

fn list_marker(marker: ListMarker) -> String {
    match marker {
        ListMarker::Unordered => "• ".to_owned(),
        ListMarker::Ordered(number) => format!("{number}. "),
        ListMarker::Task {
            checked,
            number: None,
        } => format!("{} ", if checked { '☑' } else { '☐' }),
        ListMarker::Task {
            checked,
            number: Some(number),
        } => format!("{number}. {} ", if checked { '☑' } else { '☐' }),
    }
}

fn layout_thematic_break(
    kind: BlockKind,
    block: usize,
    width: usize,
    base_column: usize,
    mut leading: String,
    rows: &mut Vec<RenderedRow>,
) {
    let rule_width = width.clamp(1, 8);
    leading.push_str(&"─".repeat(rule_width.saturating_sub(1)));
    let column = base_column + UnicodeWidthStr::width(leading.as_str());
    rows.push(RenderedRow {
        cells: vec![RenderedCell {
            symbol: "─".to_owned(),
            position: SemanticPosition { block, grapheme: 0 },
            width: 1,
            style: CellStyle::from_semantics(kind, InlineStyle::default(), false),
            link_target: None,
        }],
        column,
        leading,
    });
}

fn layout_empty_list_item(
    block: &Block,
    block_index: usize,
    base_column: usize,
    rows: &mut Vec<RenderedRow>,
) {
    let item = block
        .list_item()
        .expect("empty list content belongs to a list item");
    let mut leading = "│ ".repeat(block.quote_depth());
    leading.push_str(&"  ".repeat(item.depth.saturating_sub(1)));
    let symbol = list_marker(item.marker).trim_end().to_owned();
    let column = base_column + UnicodeWidthStr::width(leading.as_str());
    rows.push(RenderedRow {
        cells: vec![RenderedCell {
            width: UnicodeWidthStr::width(symbol.as_str()),
            symbol,
            position: SemanticPosition {
                block: block_index,
                grapheme: 0,
            },
            style: CellStyle::from_semantics(block.kind(), InlineStyle::default(), false),
            link_target: None,
        }],
        column,
        leading,
    });
}

fn wrap_block(
    block: &Block,
    block_index: usize,
    width: usize,
    column: usize,
    leading: &str,
    rows: &mut Vec<RenderedRow>,
) {
    let mut row = Vec::new();
    let mut word = Vec::new();
    let mut separator = None;
    let mut grapheme = 0;

    for span in block.spans() {
        let style =
            CellStyle::from_semantics(block.kind(), span.style(), span.link_target().is_some());
        for symbol in span.text().graphemes(true) {
            let position = SemanticPosition {
                block: block_index,
                grapheme,
            };
            grapheme += 1;

            if symbol == "\n" {
                flush_word(
                    &mut word,
                    &mut separator,
                    &mut row,
                    width,
                    column,
                    leading,
                    rows,
                );
                push_row_if_populated(&mut row, column, leading, rows);
                separator = None;
                continue;
            }

            let display_symbol = if UnicodeWidthStr::width(symbol) == 0
                && !symbol.chars().all(char::is_whitespace)
            {
                format!("◌{symbol}")
            } else {
                symbol.to_owned()
            };
            let cell = RenderedCell {
                width: UnicodeWidthStr::width(display_symbol.as_str()),
                symbol: display_symbol,
                position,
                style,
                link_target: span.link_target().map(str::to_owned),
            };
            if symbol.chars().all(char::is_whitespace) {
                flush_word(
                    &mut word,
                    &mut separator,
                    &mut row,
                    width,
                    column,
                    leading,
                    rows,
                );
                separator.get_or_insert(cell);
            } else {
                word.push(cell);
            }
        }
    }

    flush_word(
        &mut word,
        &mut separator,
        &mut row,
        width,
        column,
        leading,
        rows,
    );
    push_row_if_populated(&mut row, column, leading, rows);
}

fn flush_word(
    word: &mut Vec<RenderedCell>,
    separator: &mut Option<RenderedCell>,
    row: &mut Vec<RenderedCell>,
    width: usize,
    column: usize,
    leading: &str,
    rows: &mut Vec<RenderedRow>,
) {
    if word.is_empty() {
        return;
    }

    let word_width = cells_width(word);
    let separator_width = usize::from(!row.is_empty());
    if !row.is_empty() && cells_width(row) + separator_width + word_width > width {
        push_row_if_populated(row, column, leading, rows);
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
        push_row_if_populated(row, column, leading, rows);
        for cell in word.drain(..) {
            if !row.is_empty() && cells_width(row) + cell.width > width {
                push_row_if_populated(row, column, leading, rows);
            }
            row.push(cell);
        }
    }

    *separator = None;
}

fn push_row_if_populated(
    row: &mut Vec<RenderedCell>,
    column: usize,
    leading: &str,
    rows: &mut Vec<RenderedRow>,
) {
    if !row.is_empty() {
        rows.push(RenderedRow {
            cells: std::mem::take(row),
            column,
            leading: leading.to_owned(),
        });
    }
}

fn cells_width(cells: &[RenderedCell]) -> usize {
    cells.iter().map(|cell| cell.width).sum()
}
