use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    AlertKind, Block, BlockKind, Document, HeadingLevel, Image, InlineStyle, ListMarker,
    TableAlignment, TableCell,
};

const MAX_PROSE_WIDTH: usize = 100;

/// Observable layout work used by performance-focused application tests.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LayoutMetrics {
    blocks_laid_out: usize,
    documents_assembled: usize,
}

impl LayoutMetrics {
    #[must_use]
    pub fn blocks_laid_out(self) -> usize {
        self.blocks_laid_out
    }

    #[must_use]
    pub fn documents_assembled(self) -> usize {
        self.documents_assembled
    }
}

#[derive(Clone, Debug)]
struct CachedBlockLayout {
    width: u16,
    horizontal_offset: usize,
    rows: Vec<RenderedRow>,
}

#[derive(Clone, Debug)]
struct CachedDocumentLayout {
    width: u16,
    rendered: RenderedDocument,
}

#[derive(Debug, Default)]
pub(crate) struct LayoutCache {
    blocks: Vec<Option<CachedBlockLayout>>,
    document: Option<CachedDocumentLayout>,
    dirty_blocks: Vec<usize>,
    metrics: LayoutMetrics,
}

impl LayoutCache {
    pub(crate) fn prepare(
        &mut self,
        document: &Document,
        width: u16,
        horizontal_offsets: &[usize],
    ) {
        if self
            .document
            .as_ref()
            .is_some_and(|cached| cached.width == width)
        {
            self.patch_dirty_blocks(document, horizontal_offsets);
            return;
        }

        self.dirty_blocks.clear();
        self.blocks.resize_with(document.blocks().len(), || None);
        self.blocks.truncate(document.blocks().len());
        let pane_width = usize::from(width.max(1));
        let mut rows = Vec::new();
        for (block_index, block, horizontal_offset) in
            blocks_with_offsets(document, horizontal_offsets)
        {
            let reusable = self.blocks[block_index].as_ref().is_some_and(|cached| {
                cached.width == width && cached.horizontal_offset == horizontal_offset
            });
            if !reusable {
                self.layout_and_cache_block(block, block_index, width, horizontal_offset);
            }
            rows.extend(
                self.blocks[block_index]
                    .as_ref()
                    .expect("block layout is cached")
                    .rows
                    .iter()
                    .cloned(),
            );
        }

        let rendered = RenderedDocument {
            rows,
            content_width: pane_width.min(MAX_PROSE_WIDTH),
        };
        self.metrics.documents_assembled += 1;
        self.document = Some(CachedDocumentLayout { width, rendered });
    }

    fn patch_dirty_blocks(&mut self, document: &Document, horizontal_offsets: &[usize]) {
        let dirty_blocks = std::mem::take(&mut self.dirty_blocks);
        for block_index in dirty_blocks {
            let Some(block) = document.blocks().get(block_index) else {
                continue;
            };
            let horizontal_offset = horizontal_offsets
                .get(block_index)
                .copied()
                .unwrap_or_default();
            let width = self
                .document
                .as_ref()
                .expect("dirty blocks require a cached document")
                .width;
            let rows = self.layout_and_cache_block(block, block_index, width, horizontal_offset);
            let rendered = &mut self
                .document
                .as_mut()
                .expect("dirty blocks require a cached document")
                .rendered;
            let first = rendered
                .rows
                .partition_point(|row| row.block() < block_index);
            let last = rendered
                .rows
                .partition_point(|row| row.block() <= block_index);
            rendered.rows.splice(first..last, rows);
        }
    }

    fn layout_and_cache_block(
        &mut self,
        block: &Block,
        block_index: usize,
        width: u16,
        horizontal_offset: usize,
    ) -> Vec<RenderedRow> {
        let rows = layout_block(
            block,
            block_index,
            usize::from(width.max(1)),
            horizontal_offset,
        );
        self.blocks[block_index] = Some(CachedBlockLayout {
            width,
            horizontal_offset,
            rows: rows.clone(),
        });
        self.metrics.blocks_laid_out += 1;
        rows
    }

    pub(crate) fn invalidate_block(&mut self, block: usize) {
        if !self.dirty_blocks.contains(&block) {
            self.dirty_blocks.push(block);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.blocks.clear();
        self.document = None;
        self.dirty_blocks.clear();
    }

    pub(crate) fn metrics(&self) -> LayoutMetrics {
        self.metrics
    }

    pub(crate) fn is_prepared(&self, width: u16) -> bool {
        self.document
            .as_ref()
            .is_some_and(|cached| cached.width == width && self.dirty_blocks.is_empty())
    }

    pub(crate) fn document(&self) -> &RenderedDocument {
        &self
            .document
            .as_ref()
            .expect("layout is prepared before rendering")
            .rendered
    }

    pub(crate) fn reset_metrics(&mut self) {
        self.metrics = LayoutMetrics::default();
    }
}

#[derive(Clone, Copy)]
struct TableColumnWidth {
    preferred: usize,
    minimum: usize,
}

#[derive(Clone, Copy)]
struct TableBorder {
    left: char,
    junction: char,
    right: char,
}

#[derive(Clone, Copy)]
struct RowContext<'a> {
    column: usize,
    leading: &'a str,
    alert_kind: Option<AlertKind>,
}

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
    code: bool,
    thematic_break: bool,
    table_header: bool,
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
    pub fn is_code(self) -> bool {
        self.code
    }

    #[must_use]
    pub fn is_thematic_break(self) -> bool {
        self.thematic_break
    }

    #[must_use]
    pub fn is_table_header(self) -> bool {
        self.table_header
    }

    fn from_semantics(kind: BlockKind, inline: InlineStyle, link: bool) -> Self {
        Self {
            heading_level: match kind {
                BlockKind::Heading(level) => Some(level),
                _ => None,
            },
            inline,
            link,
            code: kind == BlockKind::Code,
            thematic_break: kind == BlockKind::ThematicBreak,
            table_header: false,
        }
    }

    fn with_table_header(mut self, table_header: bool) -> Self {
        self.table_header = table_header;
        self
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
    decorative: bool,
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
        !self.decorative && !self.symbol.chars().all(char::is_whitespace)
    }

    #[must_use]
    pub fn is_decorative(&self) -> bool {
        self.decorative
    }
}

/// A rendered row whose cells retain their semantic cursor mappings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedRow {
    cells: Vec<RenderedCell>,
    column: usize,
    leading: String,
    horizontal_offset: usize,
    block: usize,
    alert_kind: Option<AlertKind>,
}

struct CellProjection<'a> {
    cell: &'a RenderedCell,
    content_column: usize,
    visible_column: Option<usize>,
    clipped_width: usize,
}

impl RenderedRow {
    #[must_use]
    pub fn cells(&self) -> &[RenderedCell] {
        &self.cells
    }

    #[must_use]
    pub fn text(&self) -> String {
        let mut text = self.leading.clone();
        text.push_str(&" ".repeat(self.clipped_prefix_width()));
        text.extend(self.visible_cells().map(RenderedCell::symbol));
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
        self.leading_width() + cells_width(&self.cells).saturating_sub(self.horizontal_offset)
    }

    #[must_use]
    pub fn horizontal_offset(&self) -> usize {
        self.horizontal_offset
    }

    #[must_use]
    pub fn block(&self) -> usize {
        self.block
    }

    #[must_use]
    pub fn alert_kind(&self) -> Option<AlertKind> {
        self.alert_kind
    }

    pub(crate) fn visible_cells(&self) -> impl Iterator<Item = &RenderedCell> {
        self.projected_cells()
            .filter(|projection| projection.visible_column.is_some())
            .map(|projection| projection.cell)
    }

    pub(crate) fn clipped_prefix_width(&self) -> usize {
        self.projected_cells()
            .map(|projection| projection.clipped_width)
            .find(|width| *width > 0)
            .unwrap_or_default()
    }

    fn projected_cells(&self) -> impl Iterator<Item = CellProjection<'_>> {
        let mut content_column = 0;
        self.cells.iter().map(move |cell| {
            let start = content_column;
            let end = start + cell.width;
            content_column = end;
            let visible_column = if start >= self.horizontal_offset {
                Some(self.column + start - self.horizontal_offset)
            } else {
                None
            };
            CellProjection {
                cell,
                content_column: start,
                visible_column,
                clipped_width: if start < self.horizontal_offset && end > self.horizontal_offset {
                    end - self.horizontal_offset
                } else {
                    0
                },
            }
        })
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
            for projection in row.projected_cells() {
                if projection.cell.is_navigable() && projection.cell.position == position {
                    return Some(CellLocation {
                        row: row_index,
                        column: projection.visible_column?,
                        width: projection.cell.width,
                        position,
                    });
                }
            }
            None
        })
    }

    #[must_use]
    pub(crate) fn unclipped_cell_for_position(
        &self,
        position: SemanticPosition,
    ) -> Option<CellLocation> {
        self.rows.iter().enumerate().find_map(|(row_index, row)| {
            row.projected_cells()
                .find(|projection| {
                    projection.cell.is_navigable() && projection.cell.position == position
                })
                .map(|projection| CellLocation {
                    row: row_index,
                    column: row.column + projection.content_column,
                    width: projection.cell.width,
                    position,
                })
        })
    }

    #[must_use]
    pub fn position_at(&self, row: usize, column: usize) -> Option<SemanticPosition> {
        let row = self.rows.get(row)?;
        for projection in row.projected_cells() {
            if !projection.cell.is_navigable() {
                continue;
            }
            let Some(visible_column) = projection.visible_column else {
                continue;
            };
            let width = projection.cell.width.max(1);
            if (visible_column..visible_column + width).contains(&column) {
                return Some(projection.cell.position);
            }
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
        let target_column = column
            .saturating_sub(row.column)
            .saturating_add(row.horizontal_offset);
        let mut nearest = None;
        for projection in row.projected_cells() {
            if projection.cell.is_navigable() {
                let candidate = (
                    projection.content_column.abs_diff(target_column),
                    projection.cell.position,
                );
                if nearest.is_none_or(|current| candidate < current) {
                    nearest = Some(candidate);
                }
            }
        }
        nearest.map(|(_, position)| position)
    }
}

#[must_use]
pub fn layout(document: &Document, width: u16) -> RenderedDocument {
    layout_with_offsets(document, width, &[])
}

pub(crate) fn logical_positions(document: &Document) -> Vec<SemanticPosition> {
    let mut positions = Vec::new();
    for (block_index, block) in document.blocks().iter().enumerate() {
        if matches!(block.kind(), BlockKind::ThematicBreak | BlockKind::Empty) {
            positions.push(SemanticPosition {
                block: block_index,
                grapheme: 0,
            });
        } else if let Some(table) = block.table() {
            for cell in table.rows().iter().flat_map(|row| row.cells()) {
                logical_span_positions(
                    block_index,
                    cell.grapheme_offset(),
                    cell.spans(),
                    &mut positions,
                );
            }
        } else {
            logical_span_positions(block_index, 0, block.spans(), &mut positions);
        }
    }
    positions
}

fn logical_span_positions(
    block: usize,
    mut grapheme: usize,
    spans: &[crate::InlineSpan],
    positions: &mut Vec<SemanticPosition>,
) {
    for span in spans {
        if span.image().is_some() {
            positions.push(SemanticPosition { block, grapheme });
            grapheme += 1;
            continue;
        }
        for symbol in span.text().graphemes(true) {
            if symbol != "\n" && !symbol.chars().all(char::is_whitespace) {
                positions.push(SemanticPosition { block, grapheme });
            }
            grapheme += 1;
        }
    }
}

pub(crate) fn layout_with_offsets(
    document: &Document,
    width: u16,
    horizontal_offsets: &[usize],
) -> RenderedDocument {
    let pane_width = usize::from(width.max(1));
    let mut rows = Vec::new();

    for (block_index, block, horizontal_offset) in blocks_with_offsets(document, horizontal_offsets)
    {
        rows.extend(layout_block(
            block,
            block_index,
            pane_width,
            horizontal_offset,
        ));
    }

    RenderedDocument {
        rows,
        content_width: pane_width.min(MAX_PROSE_WIDTH),
    }
}

fn blocks_with_offsets<'a>(
    document: &'a Document,
    horizontal_offsets: &'a [usize],
) -> impl Iterator<Item = (usize, &'a Block, usize)> + 'a {
    document
        .blocks()
        .iter()
        .enumerate()
        .map(|(block_index, block)| {
            (
                block_index,
                block,
                horizontal_offsets
                    .get(block_index)
                    .copied()
                    .unwrap_or_default(),
            )
        })
}

fn layout_block(
    block: &Block,
    block_index: usize,
    pane_width: usize,
    horizontal_offset: usize,
) -> Vec<RenderedRow> {
    let content_width = pane_width.min(MAX_PROSE_WIDTH);
    let base_column = pane_width.saturating_sub(content_width) / 2;
    let leading = block_leading(block);
    let leading_width = UnicodeWidthStr::width(leading.as_str());
    let mut rows = Vec::new();
    if matches!(block.kind(), BlockKind::Code | BlockKind::FrontMatter) {
        layout_code(
            block,
            block_index,
            leading_width,
            &leading,
            horizontal_offset,
            &mut rows,
        );
    } else if block.kind() == BlockKind::Table {
        layout_table(
            block,
            block_index,
            pane_width.saturating_sub(leading_width).max(1),
            leading_width,
            &leading,
            horizontal_offset,
            &mut rows,
        );
    } else {
        let block_width = content_width.saturating_sub(leading_width).max(1);
        if block.kind() == BlockKind::ThematicBreak {
            layout_thematic_break(
                block.kind(),
                block.alert_kind(),
                block_index,
                block_width,
                base_column,
                leading,
                &mut rows,
            );
        } else if block.kind() == BlockKind::Empty {
            layout_empty_list_item(block, block_index, base_column, &mut rows);
        } else {
            let column = base_column + leading_width;
            wrap_block(block, block_index, block_width, column, &leading, &mut rows);
        }
    }
    rows
}

fn layout_code(
    block: &Block,
    block_index: usize,
    column: usize,
    leading: &str,
    horizontal_offset: usize,
    rows: &mut Vec<RenderedRow>,
) {
    let first_code_row = rows.len();
    let mut row = Vec::new();
    let mut source_column = 0;
    let mut grapheme = 0;
    let mut ended_with_newline = false;

    for span in block.spans() {
        let style = CellStyle::from_semantics(block.kind(), span.style(), false);
        for symbol in span.text().graphemes(true) {
            let position = SemanticPosition {
                block: block_index,
                grapheme,
            };
            grapheme += 1;

            if symbol == "\n" {
                rows.push(RenderedRow {
                    cells: std::mem::take(&mut row),
                    column,
                    leading: leading.to_owned(),
                    horizontal_offset,
                    block: block_index,
                    alert_kind: block.alert_kind(),
                });
                source_column = 0;
                ended_with_newline = true;
                continue;
            }

            ended_with_newline = false;
            let display_symbol = if symbol == "\t" {
                " ".repeat(4 - source_column % 4)
            } else {
                display_grapheme(symbol)
            };
            let width = UnicodeWidthStr::width(display_symbol.as_str());
            source_column += width;
            row.push(RenderedCell {
                symbol: display_symbol,
                position,
                width,
                style,
                link_target: None,
                decorative: false,
            });
        }
    }

    if !ended_with_newline || rows.len() == first_code_row {
        rows.push(RenderedRow {
            cells: row,
            column,
            leading: leading.to_owned(),
            horizontal_offset,
            block: block_index,
            alert_kind: block.alert_kind(),
        });
    }
}

fn layout_table(
    block: &Block,
    block_index: usize,
    pane_width: usize,
    column: usize,
    leading: &str,
    horizontal_offset: usize,
    rows: &mut Vec<RenderedRow>,
) {
    let table = block.table().expect("table block has table semantics");
    let row_context = RowContext {
        column,
        leading,
        alert_kind: block.alert_kind(),
    };
    let column_count = table
        .rows()
        .iter()
        .map(|row| row.cells().len())
        .chain(std::iter::once(table.alignments().len()))
        .max()
        .unwrap_or_default();
    if column_count == 0 {
        return;
    }

    let mut column_widths = vec![
        TableColumnWidth {
            preferred: 3,
            minimum: 3,
        };
        column_count
    ];
    for row in table.rows() {
        for (column, cell) in row.cells().iter().enumerate() {
            let display_text = cell
                .spans()
                .iter()
                .map(inline_display_text)
                .collect::<String>();
            let content_width = display_text
                .split('\n')
                .map(UnicodeWidthStr::width)
                .max()
                .unwrap_or_default()
                .max(1);
            column_widths[column].preferred = column_widths[column]
                .preferred
                .max(content_width.saturating_add(2));
            let grapheme_width = cell
                .spans()
                .iter()
                .map(|span| {
                    let display_text = inline_display_text(span);
                    display_text
                        .graphemes(true)
                        .filter(|symbol| *symbol != "\n")
                        .map(display_width)
                        .max()
                        .unwrap_or_default()
                })
                .max()
                .unwrap_or(1);
            column_widths[column].minimum = column_widths[column]
                .minimum
                .max(grapheme_width.saturating_add(2));
        }
    }
    shrink_table_columns(&mut column_widths, pane_width);

    push_table_border(
        block_index,
        &column_widths,
        TableBorder {
            left: '┌',
            junction: '┬',
            right: '┐',
        },
        row_context,
        horizontal_offset,
        rows,
    );
    for row in table.rows() {
        let visual_cells = (0..column_count)
            .map(|column| {
                row.cells().get(column).map_or_else(
                    || vec![Vec::new()],
                    |cell| {
                        layout_table_cell(
                            block_index,
                            cell,
                            column_widths[column].preferred.saturating_sub(2).max(1),
                            row.is_header(),
                        )
                    },
                )
            })
            .collect::<Vec<_>>();
        let height = visual_cells.iter().map(Vec::len).max().unwrap_or(1);

        for visual_row in 0..height {
            let mut cells = Vec::new();
            push_table_decoration(&mut cells, block_index, "│");
            for column in 0..column_count {
                let content = visual_cells[column]
                    .get(visual_row)
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                let inner_width = column_widths[column].preferred.saturating_sub(2).max(1);
                let content_width = cells_width(content);
                let remaining = inner_width.saturating_sub(content_width);
                let alignment = table
                    .alignments()
                    .get(column)
                    .copied()
                    .unwrap_or(TableAlignment::None);
                let (left, right) = match alignment {
                    TableAlignment::None | TableAlignment::Left => (1, remaining + 1),
                    TableAlignment::Center => {
                        let left = remaining / 2;
                        (left + 1, remaining - left + 1)
                    }
                    TableAlignment::Right => (remaining + 1, 1),
                };
                push_table_spaces(&mut cells, block_index, left);
                cells.extend_from_slice(content);
                push_table_spaces(&mut cells, block_index, right);
                push_table_decoration(&mut cells, block_index, "│");
            }
            rows.push(RenderedRow {
                cells,
                column: row_context.column,
                leading: row_context.leading.to_owned(),
                horizontal_offset,
                block: block_index,
                alert_kind: row_context.alert_kind,
            });
        }

        if row.is_header() {
            push_table_border(
                block_index,
                &column_widths,
                TableBorder {
                    left: '├',
                    junction: '┼',
                    right: '┤',
                },
                row_context,
                horizontal_offset,
                rows,
            );
        }
    }
    push_table_border(
        block_index,
        &column_widths,
        TableBorder {
            left: '└',
            junction: '┴',
            right: '┘',
        },
        row_context,
        horizontal_offset,
        rows,
    );
}

fn shrink_table_columns(column_widths: &mut [TableColumnWidth], pane_width: usize) {
    let borders = column_widths.len().saturating_add(1);
    let available = pane_width.saturating_sub(borders);
    if available < column_widths.iter().map(|width| width.minimum).sum() {
        for width in column_widths {
            width.preferred = width.minimum;
        }
        return;
    }

    while column_widths
        .iter()
        .map(|width| width.preferred)
        .sum::<usize>()
        > available
    {
        let Some((column, _)) = column_widths
            .iter()
            .enumerate()
            .filter(|(_, width)| width.preferred > width.minimum)
            .max_by_key(|(_, width)| width.preferred)
        else {
            break;
        };
        column_widths[column].preferred -= 1;
    }
}

fn layout_table_cell(
    block_index: usize,
    cell: &TableCell,
    width: usize,
    table_header: bool,
) -> Vec<Vec<RenderedCell>> {
    let mut lines = vec![Vec::new()];
    let mut grapheme = cell.grapheme_offset();

    for span in cell.spans() {
        let style =
            CellStyle::from_semantics(BlockKind::Table, span.style(), span.link_target().is_some())
                .with_table_header(table_header);
        if let Some(image) = span.image() {
            let image_cell = rendered_image_cell(block_index, grapheme, style, image);
            let line = lines.last_mut().expect("table cell has a visual line");
            if !line.is_empty() && cells_width(line).saturating_add(image_cell.width) > width {
                lines.push(Vec::new());
            }
            lines
                .last_mut()
                .expect("table cell has a visual line")
                .push(image_cell);
            grapheme += 1;
            continue;
        }
        for symbol in span.text().graphemes(true) {
            let position = SemanticPosition {
                block: block_index,
                grapheme,
            };
            grapheme += 1;
            if symbol == "\n" {
                lines.push(Vec::new());
                continue;
            }

            let display_symbol = display_grapheme(symbol);
            let symbol_width = UnicodeWidthStr::width(display_symbol.as_str());
            let line = lines.last_mut().expect("table cell has a visual line");
            if !line.is_empty() && cells_width(line).saturating_add(symbol_width) > width {
                lines.push(Vec::new());
            }
            lines
                .last_mut()
                .expect("table cell has a visual line")
                .push(RenderedCell {
                    symbol: display_symbol,
                    position,
                    width: symbol_width,
                    style,
                    link_target: span.link_target().map(str::to_owned),
                    decorative: false,
                });
        }
    }
    lines
}

fn push_table_border(
    block: usize,
    column_widths: &[TableColumnWidth],
    border: TableBorder,
    context: RowContext<'_>,
    horizontal_offset: usize,
    rows: &mut Vec<RenderedRow>,
) {
    let mut cells = Vec::new();
    push_table_decoration(&mut cells, block, &border.left.to_string());
    for (column, width) in column_widths.iter().enumerate() {
        for _ in 0..width.preferred {
            push_table_decoration(&mut cells, block, "─");
        }
        let glyph = if column + 1 == column_widths.len() {
            border.right
        } else {
            border.junction
        };
        push_table_decoration(&mut cells, block, &glyph.to_string());
    }
    rows.push(RenderedRow {
        cells,
        column: context.column,
        leading: context.leading.to_owned(),
        horizontal_offset,
        block,
        alert_kind: context.alert_kind,
    });
}

fn push_table_spaces(cells: &mut Vec<RenderedCell>, block: usize, count: usize) {
    for _ in 0..count {
        push_table_decoration(cells, block, " ");
    }
}

fn push_table_decoration(cells: &mut Vec<RenderedCell>, block: usize, symbol: &str) {
    cells.push(RenderedCell {
        symbol: symbol.to_owned(),
        position: SemanticPosition { block, grapheme: 0 },
        width: UnicodeWidthStr::width(symbol),
        style: CellStyle::from_semantics(BlockKind::Table, InlineStyle::default(), false),
        link_target: None,
        decorative: true,
    });
}

fn block_leading(block: &Block) -> String {
    let mut leading = quote_leading(block);
    if block.kind() == BlockKind::FrontMatter {
        leading.push_str("metadata │ ");
    }
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

fn quote_leading(block: &Block) -> String {
    if let Some(alert) = block.alert_kind() {
        let mut leading = "│ ".repeat(block.quote_depth().saturating_sub(1));
        leading.push_str(alert_label(alert));
        leading.push_str(" │ ");
        leading
    } else {
        "│ ".repeat(block.quote_depth())
    }
}

fn alert_label(kind: AlertKind) -> &'static str {
    kind.rendered_label()
}

fn list_marker(marker: ListMarker) -> String {
    marker.rendered_text()
}

fn layout_thematic_break(
    kind: BlockKind,
    alert_kind: Option<AlertKind>,
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
            decorative: false,
        }],
        column,
        leading,
        horizontal_offset: 0,
        block,
        alert_kind,
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
    let mut leading = quote_leading(block);
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
            decorative: false,
        }],
        column,
        leading,
        horizontal_offset: 0,
        block: block_index,
        alert_kind: block.alert_kind(),
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
    let row_context = RowContext {
        column,
        leading,
        alert_kind: block.alert_kind(),
    };
    let mut row = Vec::new();
    let mut word = Vec::new();
    let mut separator = None;
    let mut grapheme = 0;

    for span in block.spans() {
        let style =
            CellStyle::from_semantics(block.kind(), span.style(), span.link_target().is_some());
        if let Some(image) = span.image() {
            word.push(rendered_image_cell(block_index, grapheme, style, image));
            grapheme += 1;
            continue;
        }
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
                    row_context,
                    rows,
                );
                push_row_if_populated(&mut row, row_context, rows);
                separator = None;
                continue;
            }

            let display_symbol = display_grapheme(symbol);
            let cell = RenderedCell {
                width: UnicodeWidthStr::width(display_symbol.as_str()),
                symbol: display_symbol,
                position,
                style,
                link_target: span.link_target().map(str::to_owned),
                decorative: false,
            };
            if symbol.chars().all(char::is_whitespace) {
                flush_word(
                    &mut word,
                    &mut separator,
                    &mut row,
                    width,
                    row_context,
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
        row_context,
        rows,
    );
    push_row_if_populated(&mut row, row_context, rows);
}

fn inline_display_text(span: &crate::InlineSpan) -> String {
    span.image()
        .map_or_else(|| span.text().to_owned(), Image::rendered_text)
}

fn rendered_image_cell(
    block: usize,
    grapheme: usize,
    style: CellStyle,
    image: &Image,
) -> RenderedCell {
    let symbol = image.rendered_text();
    RenderedCell {
        width: UnicodeWidthStr::width(symbol.as_str()),
        symbol,
        position: SemanticPosition { block, grapheme },
        style,
        link_target: Some(image.target().to_owned()),
        decorative: false,
    }
}

fn flush_word(
    word: &mut Vec<RenderedCell>,
    separator: &mut Option<RenderedCell>,
    row: &mut Vec<RenderedCell>,
    width: usize,
    context: RowContext<'_>,
    rows: &mut Vec<RenderedRow>,
) {
    if word.is_empty() {
        return;
    }

    let word_width = cells_width(word);
    let separator_width = usize::from(!row.is_empty());
    if !row.is_empty() && cells_width(row) + separator_width + word_width > width {
        push_row_if_populated(row, context, rows);
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
        push_row_if_populated(row, context, rows);
        for cell in word.drain(..) {
            if !row.is_empty() && cells_width(row) + cell.width > width {
                push_row_if_populated(row, context, rows);
            }
            row.push(cell);
        }
    }

    *separator = None;
}

fn push_row_if_populated(
    row: &mut Vec<RenderedCell>,
    context: RowContext<'_>,
    rows: &mut Vec<RenderedRow>,
) {
    if !row.is_empty() {
        let block = row[0].position.block;
        rows.push(RenderedRow {
            cells: std::mem::take(row),
            column: context.column,
            leading: context.leading.to_owned(),
            horizontal_offset: 0,
            block,
            alert_kind: context.alert_kind,
        });
    }
}

fn cells_width(cells: &[RenderedCell]) -> usize {
    cells.iter().map(|cell| cell.width).sum()
}

fn display_width(grapheme: &str) -> usize {
    let width = UnicodeWidthStr::width(grapheme);
    if needs_dotted_circle(grapheme) {
        1
    } else {
        width
    }
}

fn display_grapheme(grapheme: &str) -> String {
    if needs_dotted_circle(grapheme) {
        format!("◌{grapheme}")
    } else {
        grapheme.to_owned()
    }
}

fn needs_dotted_circle(grapheme: &str) -> bool {
    UnicodeWidthStr::width(grapheme) == 0 && !grapheme.chars().all(char::is_whitespace)
}
