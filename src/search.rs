use std::ops::Range;
use unicode_casefold::UnicodeCaseFold;
use unicode_segmentation::UnicodeSegmentation;

use crate::{BlockKind, Document, InlineSpan, SemanticPosition};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SearchMatch {
    pub(crate) start: SemanticPosition,
    pub(crate) positions: Vec<SemanticPosition>,
    pub(crate) leading_blocks: Vec<usize>,
}

#[derive(Debug)]
struct SearchableBlock {
    graphemes: Vec<(String, SearchMapping)>,
}

#[derive(Clone, Copy, Debug, Default)]
struct SearchMapping {
    target: Option<SemanticPosition>,
    highlight: Option<SemanticPosition>,
    leading_block: Option<usize>,
}

pub(crate) fn find_matches(document: &Document, query: &str) -> Vec<SearchMatch> {
    let case_sensitive = query.chars().any(char::is_uppercase);
    let needle = search_case(query, case_sensitive);
    if needle.is_empty() {
        return Vec::new();
    }

    let mut matches = Vec::new();
    for block in searchable_blocks(document) {
        let mut haystack = String::new();
        let mut byte_mappings = Vec::new();
        for (text, mapping) in block.graphemes {
            let text = search_case(&text, case_sensitive);
            haystack.push_str(&text);
            byte_mappings.extend(std::iter::repeat_n(mapping, text.len()));
        }

        let mut offset = 0;
        while offset < haystack.len() {
            let Some(found) = haystack[offset..].find(&needle) else {
                break;
            };
            let start = offset + found;
            let end = start + needle.len();
            let mut target = None;
            let mut positions = Vec::new();
            let mut leading_blocks = Vec::new();
            for mapping in &byte_mappings[start..end] {
                target = target.or(mapping.target);
                if let Some(position) = mapping.highlight
                    && !positions.contains(&position)
                {
                    positions.push(position);
                }
                if let Some(block) = mapping.leading_block
                    && !leading_blocks.contains(&block)
                {
                    leading_blocks.push(block);
                }
            }
            target = target
                .or_else(|| {
                    byte_mappings[end..]
                        .iter()
                        .find_map(|mapping| mapping.target)
                })
                .or_else(|| {
                    byte_mappings[..start]
                        .iter()
                        .rev()
                        .find_map(|mapping| mapping.target)
                });
            if let Some(start) = target
                && !matches.iter().any(|search_match: &SearchMatch| {
                    search_match.start == start
                        && search_match.positions == positions
                        && search_match.leading_blocks == leading_blocks
                })
            {
                matches.push(SearchMatch {
                    start,
                    positions,
                    leading_blocks,
                });
            }
            offset = start
                + haystack[start..]
                    .chars()
                    .next()
                    .map(char::len_utf8)
                    .unwrap_or(1);
        }
    }
    matches
}

fn search_case(text: &str, case_sensitive: bool) -> String {
    if case_sensitive {
        text.to_owned()
    } else {
        text.case_fold().collect()
    }
}

fn searchable_blocks(document: &Document) -> Vec<SearchableBlock> {
    let mut blocks = Vec::new();
    for (block_index, block) in document.blocks().iter().enumerate() {
        if let Some(table) = block.table() {
            for row in table.rows() {
                for cell in row.cells() {
                    let mut grapheme = cell.grapheme_offset();
                    let mut raw = Vec::new();
                    push_searchable_spans(cell.spans(), block_index, &mut grapheme, &mut raw);
                    blocks.extend(split_searchable_lines(raw));
                }
            }
            continue;
        }

        let mut grapheme = 0;
        let mut raw = Vec::new();
        push_searchable_spans(block.spans(), block_index, &mut grapheme, &mut raw);
        let target = raw
            .iter()
            .find_map(|(_, mapping)| mapping.target)
            .or_else(|| {
                matches!(block.kind(), BlockKind::Empty | BlockKind::ThematicBreak).then_some(
                    SemanticPosition {
                        block: block_index,
                        grapheme: 0,
                    },
                )
            });

        if let Some(target) = target {
            for leading in searchable_leading_fragments(block) {
                blocks.push(SearchableBlock {
                    graphemes: leading
                        .graphemes(true)
                        .map(|text| {
                            (
                                text.to_owned(),
                                SearchMapping {
                                    target: Some(target),
                                    highlight: None,
                                    leading_block: Some(block_index),
                                },
                            )
                        })
                        .collect(),
                });
            }
        }
        if block.kind() == BlockKind::Empty
            && let Some(item) = block.list_item()
        {
            blocks.push(SearchableBlock {
                graphemes: item
                    .marker
                    .rendered_text()
                    .trim_end()
                    .graphemes(true)
                    .map(|text| {
                        (
                            text.to_owned(),
                            SearchMapping {
                                target,
                                highlight: target,
                                leading_block: None,
                            },
                        )
                    })
                    .collect(),
            });
        }

        match block.kind() {
            BlockKind::Code | BlockKind::FrontMatter => {
                blocks.extend(searchable_code_lines(raw));
            }
            _ => blocks.extend(searchable_wrapped_lines(raw)),
        }
    }
    blocks
}

fn push_searchable_spans(
    spans: &[InlineSpan],
    block: usize,
    grapheme: &mut usize,
    searchable: &mut Vec<(String, SearchMapping)>,
) {
    for span in spans {
        if let Some(image) = span.image() {
            let position = SemanticPosition {
                block,
                grapheme: *grapheme,
            };
            searchable.extend(image.rendered_text().graphemes(true).map(|text| {
                (
                    text.to_owned(),
                    SearchMapping {
                        target: Some(position),
                        highlight: Some(position),
                        leading_block: None,
                    },
                )
            }));
            *grapheme += span.text().graphemes(true).count();
            continue;
        }
        for text in span.text().graphemes(true) {
            let position = SemanticPosition {
                block,
                grapheme: *grapheme,
            };
            let target = (!text.chars().all(char::is_whitespace)).then_some(position);
            searchable.push((
                text.to_owned(),
                SearchMapping {
                    target,
                    highlight: Some(position),
                    leading_block: None,
                },
            ));
            *grapheme += 1;
        }
    }
}

fn split_searchable_lines(raw: Vec<(String, SearchMapping)>) -> Vec<SearchableBlock> {
    let mut blocks = Vec::new();
    let mut line = Vec::new();
    for (text, mapping) in raw {
        if text == "\n" {
            push_searchable_block(&mut blocks, &mut line);
        } else {
            line.push((text, mapping));
        }
    }
    push_searchable_block(&mut blocks, &mut line);
    blocks
}

fn searchable_code_lines(raw: Vec<(String, SearchMapping)>) -> Vec<SearchableBlock> {
    let mut blocks = Vec::new();
    let mut line = Vec::new();
    let mut source_column = 0;
    for (text, mapping) in raw {
        if text == "\n" {
            push_searchable_block(&mut blocks, &mut line);
            source_column = 0;
        } else if text == "\t" {
            let width = 4 - source_column % 4;
            line.push((" ".repeat(width), mapping));
            source_column += width;
        } else {
            source_column += unicode_width::UnicodeWidthStr::width(text.as_str());
            line.push((text, mapping));
        }
    }
    push_searchable_block(&mut blocks, &mut line);
    blocks
}

fn searchable_wrapped_lines(raw: Vec<(String, SearchMapping)>) -> Vec<SearchableBlock> {
    let mut blocks = Vec::new();
    let mut line = Vec::new();
    let mut separator_pending = None;
    for (text, mapping) in raw {
        if text == "\n" {
            push_searchable_block(&mut blocks, &mut line);
            separator_pending = None;
        } else if text.chars().all(char::is_whitespace) {
            if !line.is_empty() {
                separator_pending.get_or_insert(mapping);
            }
        } else {
            if let Some(separator) = separator_pending.take() {
                line.push((" ".to_owned(), separator));
            }
            line.push((text, mapping));
        }
    }
    push_searchable_block(&mut blocks, &mut line);
    blocks
}

fn push_searchable_block(
    blocks: &mut Vec<SearchableBlock>,
    graphemes: &mut Vec<(String, SearchMapping)>,
) {
    if !graphemes.is_empty() {
        blocks.push(SearchableBlock {
            graphemes: std::mem::take(graphemes),
        });
    }
}

fn searchable_leading_fragments(block: &crate::Block) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(alert) = block.alert_kind() {
        parts.push(alert.rendered_label().to_owned());
    }
    if block.kind() == BlockKind::FrontMatter {
        parts.push("metadata".to_owned());
    }
    if block.kind() != BlockKind::Empty
        && let Some(item) = block.list_item()
        && !item.continuation
    {
        parts.push(item.marker.rendered_text().trim_end().to_owned());
    }
    if block.kind() == BlockKind::ThematicBreak {
        parts.push("─".to_owned());
    }
    parts
}

pub(crate) fn literal_match_ranges(text: &str, query: &str) -> Vec<Range<usize>> {
    let case_sensitive = query.chars().any(char::is_uppercase);
    let needle = search_case(query, case_sensitive);
    if needle.is_empty() {
        return Vec::new();
    }

    let mut haystack = String::new();
    let mut original_ranges = Vec::new();
    for (start, character) in text.char_indices() {
        let end = start + character.len_utf8();
        let folded = search_case(&character.to_string(), case_sensitive);
        haystack.push_str(&folded);
        original_ranges.extend(std::iter::repeat_n(start..end, folded.len()));
    }

    let mut matches = Vec::new();
    let mut offset = 0;
    while offset < haystack.len() {
        let Some(found) = haystack[offset..].find(&needle) else {
            break;
        };
        let start = offset + found;
        let end = start + needle.len();
        if let (Some(first), Some(last)) = (
            original_ranges.get(start),
            original_ranges.get(end.saturating_sub(1)),
        ) {
            let range = first.start..last.end;
            if matches
                .last()
                .is_none_or(|prior: &Range<usize>| prior.end < range.start)
            {
                matches.push(range);
            } else if let Some(prior) = matches.last_mut() {
                prior.end = prior.end.max(range.end);
            }
        }
        offset = start
            + haystack[start..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or(1);
    }
    matches
}
