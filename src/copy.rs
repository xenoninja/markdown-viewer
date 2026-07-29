use unicode_segmentation::UnicodeSegmentation;

use crate::layout::RenderedDocument;
use crate::{Block, BlockKind, Document, SemanticPosition};

/// Characterwise or rendered-row visual Selection over semantic content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionMode {
    Characterwise,
    Row,
}

/// Plain rendered text for a Selection range, without decorative borders.
pub(crate) fn selected_text(
    document: &Document,
    rendered: &RenderedDocument,
    mode: SelectionMode,
    anchor: SemanticPosition,
    cursor: SemanticPosition,
) -> String {
    match mode {
        SelectionMode::Characterwise => copy_characterwise(document, anchor, cursor),
        SelectionMode::Row => copy_rows(document, rendered, anchor, cursor),
    }
}

fn copy_characterwise(
    document: &Document,
    anchor: SemanticPosition,
    cursor: SemanticPosition,
) -> String {
    let (start, end) = ordered_endpoints(anchor, cursor);
    let mut output = String::new();
    let mut previous: Option<SemanticPosition> = None;
    let mut emitted_block_leadings = Vec::new();

    for atom in copy_atoms(document) {
        let Some(position) = atom.position else {
            continue;
        };
        if position < start || position > end {
            continue;
        }
        if let Some(prior) = previous
            && (prior.block != position.block || atom.hard_break_from_prior)
        {
            push_newline(&mut output);
        }
        if !atom.leading.is_empty()
            && !emitted_block_leadings.contains(&position.block)
            && position.grapheme == first_content_grapheme(document, position.block)
        {
            emitted_block_leadings.push(position.block);
            output.push_str(&atom.leading);
        }
        if atom.cell_separator && !output.is_empty() && !output.ends_with(['\n', '\t', ' ']) {
            output.push(' ');
        }
        output.push_str(&atom.text);
        previous = Some(position);
    }
    output
}

fn copy_rows(
    document: &Document,
    rendered: &RenderedDocument,
    anchor: SemanticPosition,
    cursor: SemanticPosition,
) -> String {
    let Some(anchor_row) = rendered.row_for_position(anchor) else {
        return String::new();
    };
    let Some(cursor_row) = rendered.row_for_position(cursor) else {
        return String::new();
    };
    let start_row = anchor_row.min(cursor_row);
    let end_row = anchor_row.max(cursor_row);
    let mut lines = Vec::new();
    let mut emitted_block_leadings = Vec::new();

    for row_index in start_row..=end_row {
        let row = &rendered.rows()[row_index];
        let content_cells = row
            .cells()
            .iter()
            .filter(|cell| !cell.is_decorative())
            .collect::<Vec<_>>();
        if content_cells.is_empty() {
            continue;
        }

        let mut line = String::new();
        let leading = semantic_leading(&document.blocks()[row.block()]);
        if !leading.is_empty() && !emitted_block_leadings.contains(&row.block()) {
            line.push_str(&leading);
            emitted_block_leadings.push(row.block());
        }

        let mut last_position: Option<SemanticPosition> = None;
        for cell in content_cells {
            let position = cell.position();
            if let Some(prior) = last_position
                && (prior.block != position.block
                    || table_cell_boundary(document, prior, position))
                && !line.is_empty()
                && !line.ends_with(' ')
            {
                line.push(' ');
            }
            line.push_str(&source_grapheme(document, position));
            last_position = Some(position);
        }

        if !line.is_empty() {
            lines.push(line);
        }
    }

    lines.join("\n")
}

#[derive(Debug)]
struct CopyAtom {
    position: Option<SemanticPosition>,
    text: String,
    leading: String,
    hard_break_from_prior: bool,
    cell_separator: bool,
}

fn copy_atoms(document: &Document) -> Vec<CopyAtom> {
    let mut atoms = Vec::new();
    for (block_index, block) in document.blocks().iter().enumerate() {
        let leading = semantic_leading(block);
        if let Some(table) = block.table() {
            for (row_index, row) in table.rows().iter().enumerate() {
                for (cell_index, cell) in row.cells().iter().enumerate() {
                    let mut grapheme = cell.grapheme_offset();
                    let mut first_in_cell = true;
                    for span in cell.spans() {
                        if let Some(image) = span.image() {
                            let position = SemanticPosition {
                                block: block_index,
                                grapheme,
                            };
                            atoms.push(CopyAtom {
                                position: Some(position),
                                text: image.rendered_text(),
                                leading: if row_index == 0 && cell_index == 0 && first_in_cell {
                                    leading.clone()
                                } else {
                                    String::new()
                                },
                                hard_break_from_prior: row_index > 0 && cell_index == 0 && first_in_cell,
                                cell_separator: cell_index > 0 && first_in_cell,
                            });
                            grapheme += span.text().graphemes(true).count();
                            first_in_cell = false;
                            continue;
                        }
                        for symbol in span.text().graphemes(true) {
                            if symbol == "\n" {
                                grapheme += 1;
                                continue;
                            }
                            let position = SemanticPosition {
                                block: block_index,
                                grapheme,
                            };
                            atoms.push(CopyAtom {
                                position: Some(position),
                                text: symbol.to_owned(),
                                leading: if row_index == 0 && cell_index == 0 && first_in_cell {
                                    leading.clone()
                                } else {
                                    String::new()
                                },
                                hard_break_from_prior: row_index > 0
                                    && cell_index == 0
                                    && first_in_cell,
                                cell_separator: cell_index > 0 && first_in_cell,
                            });
                            grapheme += 1;
                            first_in_cell = false;
                        }
                    }
                }
            }
            continue;
        }

        if block.kind() == BlockKind::Empty
            && let Some(item) = block.list_item()
            && !item.continuation
        {
            atoms.push(CopyAtom {
                position: Some(SemanticPosition {
                    block: block_index,
                    grapheme: 0,
                }),
                text: item.marker.rendered_text().trim_end().to_owned(),
                leading: String::new(),
                hard_break_from_prior: false,
                cell_separator: false,
            });
            continue;
        }

        let mut grapheme = 0;
        let mut first = true;
        let mut hard_break = false;
        for span in block.spans() {
            if let Some(image) = span.image() {
                let position = SemanticPosition {
                    block: block_index,
                    grapheme,
                };
                atoms.push(CopyAtom {
                    position: Some(position),
                    text: image.rendered_text(),
                    leading: if first {
                        leading.clone()
                    } else {
                        String::new()
                    },
                    hard_break_from_prior: hard_break,
                    cell_separator: false,
                });
                grapheme += span.text().graphemes(true).count();
                first = false;
                hard_break = false;
                continue;
            }
            for symbol in span.text().graphemes(true) {
                if symbol == "\n" {
                    grapheme += 1;
                    hard_break = true;
                    continue;
                }
                let position = SemanticPosition {
                    block: block_index,
                    grapheme,
                };
                atoms.push(CopyAtom {
                    position: Some(position),
                    text: symbol.to_owned(),
                    leading: if first {
                        leading.clone()
                    } else {
                        String::new()
                    },
                    hard_break_from_prior: hard_break,
                    cell_separator: false,
                });
                grapheme += 1;
                first = false;
                hard_break = false;
            }
        }
    }
    atoms
}

fn semantic_leading(block: &Block) -> String {
    let mut leading = String::new();
    if let Some(alert) = block.alert_kind() {
        leading.push_str(alert.rendered_label());
        leading.push(' ');
    }
    if block.kind() == BlockKind::FrontMatter {
        leading.push_str("metadata ");
    }
    if block.kind() != BlockKind::Empty
        && let Some(item) = block.list_item()
        && !item.continuation
    {
        leading.push_str(&"  ".repeat(item.depth.saturating_sub(1)));
        leading.push_str(item.marker.rendered_text().trim_end());
        leading.push(' ');
    }
    leading
}

fn source_grapheme(document: &Document, position: SemanticPosition) -> String {
    let block = &document.blocks()[position.block];
    if block.kind() == BlockKind::Empty
        && let Some(item) = block.list_item()
    {
        return item.marker.rendered_text().trim_end().to_owned();
    }
    if let Some(table) = block.table() {
        for row in table.rows() {
            for cell in row.cells() {
                let mut grapheme = cell.grapheme_offset();
                for span in cell.spans() {
                    if let Some(image) = span.image() {
                        if grapheme == position.grapheme {
                            return image.rendered_text();
                        }
                        grapheme += span.text().graphemes(true).count();
                        continue;
                    }
                    for symbol in span.text().graphemes(true) {
                        if grapheme == position.grapheme {
                            return if symbol == "\n" {
                                String::new()
                            } else {
                                symbol.to_owned()
                            };
                        }
                        grapheme += 1;
                    }
                }
            }
        }
        return String::new();
    }
    let mut grapheme = 0;
    for span in block.spans() {
        if let Some(image) = span.image() {
            if grapheme == position.grapheme {
                return image.rendered_text();
            }
            grapheme += span.text().graphemes(true).count();
            continue;
        }
        for symbol in span.text().graphemes(true) {
            if grapheme == position.grapheme {
                return if symbol == "\n" {
                    String::new()
                } else {
                    symbol.to_owned()
                };
            }
            grapheme += 1;
        }
    }
    String::new()
}

fn first_content_grapheme(document: &Document, block: usize) -> usize {
    document.blocks()[block]
        .text()
        .graphemes(true)
        .enumerate()
        .find(|(_, symbol)| *symbol != "\n")
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn table_cell_boundary(
    document: &Document,
    prior: SemanticPosition,
    position: SemanticPosition,
) -> bool {
    let Some(table) = document.blocks()[position.block].table() else {
        return false;
    };
    let prior_cell = table
        .rows()
        .iter()
        .flat_map(|row| row.cells())
        .position(|cell| {
            let start = cell.grapheme_offset();
            let end = start
                + cell
                    .spans()
                    .iter()
                    .map(|span| span.text().graphemes(true).count())
                    .sum::<usize>();
            (start..end).contains(&prior.grapheme)
        });
    let next_cell = table
        .rows()
        .iter()
        .flat_map(|row| row.cells())
        .position(|cell| {
            let start = cell.grapheme_offset();
            let end = start
                + cell
                    .spans()
                    .iter()
                    .map(|span| span.text().graphemes(true).count())
                    .sum::<usize>();
            (start..end).contains(&position.grapheme)
        });
    prior_cell != next_cell
}

pub(crate) fn ordered_endpoints(
    anchor: SemanticPosition,
    cursor: SemanticPosition,
) -> (SemanticPosition, SemanticPosition) {
    if anchor <= cursor {
        (anchor, cursor)
    } else {
        (cursor, anchor)
    }
}

fn push_newline(output: &mut String) {
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
}
