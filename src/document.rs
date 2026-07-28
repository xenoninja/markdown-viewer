use std::fmt::Write as _;

use pulldown_cmark::{
    Alignment as ParsedAlignment, CodeBlockKind, Event, HeadingLevel as ParsedHeadingLevel,
    Options, Parser, Tag, TagEnd,
};

/// An owned, width-independent representation of a Markdown Document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Document {
    blocks: Vec<Block>,
}

/// The semantic role of a block in a Rendered Document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockKind {
    Paragraph,
    Heading(HeadingLevel),
    Code,
    ThematicBreak,
    RawHtml,
    Empty,
    Table,
}

/// List hierarchy attached independently to a block's semantic role.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListItem {
    pub depth: usize,
    pub marker: ListMarker,
    pub continuation: bool,
}

/// A Markdown heading's declared level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeadingLevel {
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
}

/// The meaningful marker attached to a list item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListMarker {
    Unordered,
    Ordered(u64),
    Task { checked: bool, number: Option<u64> },
}

/// Semantic content that can be laid out independently of Markdown parsing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Block {
    kind: BlockKind,
    spans: Vec<InlineSpan>,
    text: String,
    quote_depth: usize,
    list_item: Option<ListItem>,
    language: Option<String>,
    table: Option<Table>,
}

impl Block {
    #[must_use]
    pub fn kind(&self) -> BlockKind {
        self.kind
    }

    #[must_use]
    pub fn spans(&self) -> &[InlineSpan] {
        &self.spans
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn quote_depth(&self) -> usize {
        self.quote_depth
    }

    #[must_use]
    pub fn list_item(&self) -> Option<ListItem> {
        self.list_item
    }

    /// The first fenced-code info token, when one was supplied.
    #[must_use]
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    #[must_use]
    pub fn table(&self) -> Option<&Table> {
        self.table.as_ref()
    }
}

/// A GFM table's declared column alignment and semantic rows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Table {
    alignments: Vec<TableAlignment>,
    rows: Vec<TableRow>,
}

impl Table {
    #[must_use]
    pub fn alignments(&self) -> &[TableAlignment] {
        &self.alignments
    }

    #[must_use]
    pub fn rows(&self) -> &[TableRow] {
        &self.rows
    }
}

/// The alignment declared for one GFM table column.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableAlignment {
    None,
    Left,
    Center,
    Right,
}

/// One semantic row in a GFM table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableRow {
    cells: Vec<TableCell>,
    header: bool,
}

impl TableRow {
    #[must_use]
    pub fn cells(&self) -> &[TableCell] {
        &self.cells
    }

    #[must_use]
    pub fn is_header(&self) -> bool {
        self.header
    }
}

/// Inline semantic content in one GFM table cell.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableCell {
    spans: Vec<InlineSpan>,
    text: String,
    grapheme_offset: usize,
}

impl TableCell {
    #[must_use]
    pub fn spans(&self) -> &[InlineSpan] {
        &self.spans
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn grapheme_offset(&self) -> usize {
        self.grapheme_offset
    }
}

/// Inline meaning that can be combined on one run of text.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InlineStyle {
    emphasis: bool,
    strong: bool,
    strikethrough: bool,
    inline_code: bool,
}

impl InlineStyle {
    #[must_use]
    pub fn is_emphasis(self) -> bool {
        self.emphasis
    }

    #[must_use]
    pub fn is_strong(self) -> bool {
        self.strong
    }

    #[must_use]
    pub fn is_strikethrough(self) -> bool {
        self.strikethrough
    }

    #[must_use]
    pub fn is_inline_code(self) -> bool {
        self.inline_code
    }
}

/// A styled run of inline semantic text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InlineSpan {
    text: String,
    style: InlineStyle,
    link_target: Option<String>,
}

impl InlineSpan {
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn style(&self) -> InlineStyle {
        self.style
    }

    #[must_use]
    pub fn link_target(&self) -> Option<&str> {
        self.link_target.as_deref()
    }
}

impl Document {
    #[must_use]
    pub fn parse(markdown: &str) -> Self {
        let options =
            Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS | Options::ENABLE_TABLES;
        let mut blocks = Vec::new();
        let mut builder = None;
        let mut table = None;
        let mut style = InlineStyle::default();
        let mut link_target = None;
        let mut quote_depth = 0;
        let mut lists = Vec::new();
        let mut items = Vec::new();

        for event in Parser::new_ext(markdown, options) {
            match event {
                Event::Start(Tag::Table(alignments)) => {
                    finish_builder(&mut builder, &mut blocks);
                    let list_item = if items.is_empty() {
                        None
                    } else {
                        block_builder_for_item(&mut items, quote_depth, BlockKind::Table).list_item
                    };
                    table = Some(TableBuilder::new(
                        alignments.into_iter().map(table_alignment).collect(),
                        quote_depth,
                        list_item,
                    ));
                }
                Event::Start(Tag::TableHead) => {
                    table
                        .as_mut()
                        .expect("table head is inside a table")
                        .start_row(true);
                }
                Event::Start(Tag::TableRow) => {
                    table
                        .as_mut()
                        .expect("table row is inside a table")
                        .start_row(false);
                }
                Event::Start(Tag::TableCell) => {
                    table
                        .as_mut()
                        .expect("table cell is inside a table")
                        .start_cell();
                }
                Event::Start(Tag::Paragraph) => {
                    if table.is_some() {
                        continue;
                    }
                    finish_builder(&mut builder, &mut blocks);
                    if items.is_empty() {
                        builder = Some(BlockBuilder::new(BlockKind::Paragraph, quote_depth));
                    }
                }
                Event::Start(Tag::Heading { level, .. }) => {
                    let level = heading_level(level);
                    builder = Some(if items.is_empty() {
                        BlockBuilder::new(BlockKind::Heading(level), quote_depth)
                    } else {
                        block_builder_for_item(&mut items, quote_depth, BlockKind::Heading(level))
                    });
                }
                Event::Start(Tag::HtmlBlock) => {
                    builder = Some(if items.is_empty() {
                        BlockBuilder::new(BlockKind::RawHtml, quote_depth)
                    } else {
                        block_builder_for_item(&mut items, quote_depth, BlockKind::RawHtml)
                    });
                }
                Event::Start(Tag::CodeBlock(kind)) => {
                    finish_builder(&mut builder, &mut blocks);
                    let language = match kind {
                        CodeBlockKind::Fenced(info) => info
                            .split_whitespace()
                            .next()
                            .filter(|token| !token.is_empty())
                            .map(make_inert),
                        CodeBlockKind::Indented => None,
                    };
                    let mut code = if items.is_empty() {
                        BlockBuilder::new(BlockKind::Code, quote_depth)
                    } else {
                        block_builder_for_item(&mut items, quote_depth, BlockKind::Code)
                    };
                    code.language = language;
                    builder = Some(code);
                }
                Event::Start(Tag::BlockQuote(_)) => quote_depth += 1,
                Event::Start(Tag::List(start)) => {
                    finish_builder(&mut builder, &mut blocks);
                    if items.last().is_some_and(|item| !item.started) {
                        blocks.push(
                            block_builder_for_item(&mut items, quote_depth, BlockKind::Empty)
                                .finish(),
                        );
                    }
                    lists.push(ListContext { next_number: start });
                }
                Event::Start(Tag::Item) => {
                    let list = lists.last_mut().expect("list items are inside lists");
                    let marker = match &mut list.next_number {
                        Some(number) => {
                            let marker = ListMarker::Ordered(*number);
                            *number = number.saturating_add(1);
                            marker
                        }
                        None => ListMarker::Unordered,
                    };
                    items.push(ItemContext {
                        depth: lists.len(),
                        marker,
                        started: false,
                    });
                }
                Event::Start(Tag::Emphasis) => style.emphasis = true,
                Event::Start(Tag::Strong) => style.strong = true,
                Event::Start(Tag::Strikethrough) => style.strikethrough = true,
                Event::Start(Tag::Link { dest_url, .. }) => {
                    link_target = Some(make_inert(&dest_url));
                }
                Event::Text(text) | Event::InlineHtml(text) | Event::Html(text) => {
                    if let Some(table) = &mut table {
                        table.push(&make_inert(&text), style, link_target.as_deref());
                        continue;
                    }
                    if builder.is_none() && !items.is_empty() {
                        builder = Some(block_builder_for_leaf(&mut items, quote_depth));
                    }
                    if let Some(builder) = &mut builder {
                        let text = if builder.kind == BlockKind::Code {
                            make_code_inert(&text)
                        } else {
                            make_inert(&text)
                        };
                        builder.push(&text, style, link_target.as_deref());
                    }
                }
                Event::Code(text) => {
                    if let Some(table) = &mut table {
                        let mut code_style = style;
                        code_style.inline_code = true;
                        table.push(&make_inert(&text), code_style, link_target.as_deref());
                        continue;
                    }
                    if builder.is_none() && !items.is_empty() {
                        builder = Some(block_builder_for_leaf(&mut items, quote_depth));
                    }
                    if let Some(builder) = &mut builder {
                        let mut code_style = style;
                        code_style.inline_code = true;
                        builder.push(&make_inert(&text), code_style, link_target.as_deref());
                    }
                }
                Event::SoftBreak => {
                    if let Some(table) = &mut table {
                        table.push(" ", style, link_target.as_deref());
                    } else if let Some(builder) = &mut builder {
                        builder.push(" ", style, link_target.as_deref());
                    }
                }
                Event::HardBreak => {
                    if let Some(table) = &mut table {
                        table.push("\n", style, link_target.as_deref());
                    } else if let Some(builder) = &mut builder {
                        builder.push("\n", style, link_target.as_deref());
                    }
                }
                Event::End(TagEnd::TableCell) => {
                    table
                        .as_mut()
                        .expect("table cell is inside a table")
                        .finish_cell();
                }
                Event::End(TagEnd::TableHead | TagEnd::TableRow) => {
                    table
                        .as_mut()
                        .expect("table row is inside a table")
                        .finish_row();
                }
                Event::End(TagEnd::Table) => {
                    let completed = table.take().expect("table end follows table start");
                    blocks.push(completed.finish());
                }
                Event::End(
                    TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::HtmlBlock | TagEnd::CodeBlock,
                ) => {
                    finish_builder(&mut builder, &mut blocks);
                }
                Event::End(TagEnd::BlockQuote(_)) => quote_depth = quote_depth.saturating_sub(1),
                Event::End(TagEnd::Item) => {
                    finish_builder(&mut builder, &mut blocks);
                    if items.last().is_some_and(|item| !item.started) {
                        blocks.push(
                            block_builder_for_item(&mut items, quote_depth, BlockKind::Empty)
                                .finish(),
                        );
                    }
                    items.pop();
                }
                Event::End(TagEnd::List(_)) => {
                    finish_builder(&mut builder, &mut blocks);
                    lists.pop();
                }
                Event::End(TagEnd::Emphasis) => style.emphasis = false,
                Event::End(TagEnd::Strong) => style.strong = false,
                Event::End(TagEnd::Strikethrough) => style.strikethrough = false,
                Event::End(TagEnd::Link) => link_target = None,
                Event::TaskListMarker(checked) => {
                    if let Some(item) = items.last_mut() {
                        item.marker = item.marker.with_task_state(checked);
                    }
                }
                Event::Rule => {
                    finish_builder(&mut builder, &mut blocks);
                    let rule = if items.is_empty() {
                        BlockBuilder::new(BlockKind::ThematicBreak, quote_depth)
                    } else {
                        block_builder_for_item(&mut items, quote_depth, BlockKind::ThematicBreak)
                    };
                    blocks.push(rule.finish());
                }
                _ => {}
            }
        }

        finish_builder(&mut builder, &mut blocks);
        Self { blocks }
    }

    #[must_use]
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }
}

#[derive(Debug)]
struct BlockBuilder {
    kind: BlockKind,
    spans: Vec<InlineSpan>,
    quote_depth: usize,
    list_item: Option<ListItem>,
    language: Option<String>,
}

impl BlockBuilder {
    fn new(kind: BlockKind, quote_depth: usize) -> Self {
        Self {
            kind,
            spans: Vec::new(),
            quote_depth,
            list_item: None,
            language: None,
        }
    }

    fn push(&mut self, text: &str, style: InlineStyle, link_target: Option<&str>) {
        if text.is_empty() {
            return;
        }

        if let Some(last) = self.spans.last_mut()
            && last.style == style
            && last.link_target.as_deref() == link_target
        {
            last.text.push_str(text);
            return;
        }

        self.spans.push(InlineSpan {
            text: text.to_owned(),
            style,
            link_target: link_target.map(str::to_owned),
        });
    }

    fn finish(self) -> Block {
        let text = self.spans.iter().map(InlineSpan::text).collect::<String>();
        Block {
            kind: self.kind,
            spans: self.spans,
            text,
            quote_depth: self.quote_depth,
            list_item: self.list_item,
            language: self.language,
            table: None,
        }
    }
}

#[derive(Debug)]
struct TableBuilder {
    alignments: Vec<TableAlignment>,
    rows: Vec<TableRowBuilder>,
    row: Option<TableRowBuilder>,
    cell: Option<BlockBuilder>,
    quote_depth: usize,
    list_item: Option<ListItem>,
}

impl TableBuilder {
    fn new(
        alignments: Vec<TableAlignment>,
        quote_depth: usize,
        list_item: Option<ListItem>,
    ) -> Self {
        Self {
            alignments,
            rows: Vec::new(),
            row: None,
            cell: None,
            quote_depth,
            list_item,
        }
    }

    fn start_row(&mut self, header: bool) {
        self.finish_row();
        self.row = Some(TableRowBuilder {
            cells: Vec::new(),
            header,
        });
    }

    fn start_cell(&mut self) {
        self.finish_cell();
        if self.row.is_none() {
            self.start_row(false);
        }
        self.cell = Some(BlockBuilder::new(BlockKind::Paragraph, 0));
    }

    fn push(&mut self, text: &str, style: InlineStyle, link_target: Option<&str>) {
        if self.cell.is_none() {
            self.start_cell();
        }
        self.cell
            .as_mut()
            .expect("table content has a cell")
            .push(text, style, link_target);
    }

    fn finish_cell(&mut self) {
        let Some(cell) = self.cell.take() else {
            return;
        };
        let cell = cell.finish();
        self.row
            .as_mut()
            .expect("table cell belongs to a row")
            .cells
            .push(TableCell {
                spans: cell.spans,
                text: cell.text,
                grapheme_offset: 0,
            });
    }

    fn finish_row(&mut self) {
        self.finish_cell();
        if let Some(row) = self.row.take() {
            self.rows.push(row);
        }
    }

    fn finish(mut self) -> Block {
        use unicode_segmentation::UnicodeSegmentation;

        self.finish_row();
        let mut grapheme_offset = 0;
        let mut text = String::new();
        let mut first_cell = true;
        let rows = self
            .rows
            .into_iter()
            .map(|row| {
                let cells = row
                    .cells
                    .into_iter()
                    .map(|mut cell| {
                        if !first_cell {
                            text.push(' ');
                            grapheme_offset += 1;
                        }
                        first_cell = false;
                        cell.grapheme_offset = grapheme_offset;
                        grapheme_offset += cell.text.graphemes(true).count();
                        text.push_str(&cell.text);
                        cell
                    })
                    .collect();
                TableRow {
                    cells,
                    header: row.header,
                }
            })
            .collect();

        Block {
            kind: BlockKind::Table,
            spans: Vec::new(),
            text,
            quote_depth: self.quote_depth,
            list_item: self.list_item,
            language: None,
            table: Some(Table {
                alignments: self.alignments,
                rows,
            }),
        }
    }
}

#[derive(Debug)]
struct TableRowBuilder {
    cells: Vec<TableCell>,
    header: bool,
}

#[derive(Debug)]
struct ListContext {
    next_number: Option<u64>,
}

#[derive(Debug)]
struct ItemContext {
    depth: usize,
    marker: ListMarker,
    started: bool,
}

impl ListMarker {
    fn with_task_state(self, checked: bool) -> Self {
        let number = match self {
            Self::Ordered(number) => Some(number),
            Self::Task { number, .. } => number,
            Self::Unordered => None,
        };
        Self::Task { checked, number }
    }
}

fn block_builder_for_leaf(items: &mut [ItemContext], quote_depth: usize) -> BlockBuilder {
    if !items.is_empty() {
        return block_builder_for_item(items, quote_depth, BlockKind::Paragraph);
    }

    BlockBuilder::new(BlockKind::Paragraph, quote_depth)
}

fn block_builder_for_item(
    items: &mut [ItemContext],
    quote_depth: usize,
    kind: BlockKind,
) -> BlockBuilder {
    let item = items.last_mut().expect("list content is inside an item");
    let continuation = item.started;
    item.started = true;
    let mut builder = BlockBuilder::new(kind, quote_depth);
    builder.list_item = Some(ListItem {
        depth: item.depth,
        marker: item.marker,
        continuation,
    });
    builder
}

fn finish_builder(builder: &mut Option<BlockBuilder>, blocks: &mut Vec<Block>) {
    if let Some(builder) = builder.take() {
        blocks.push(builder.finish());
    }
}

fn heading_level(level: ParsedHeadingLevel) -> HeadingLevel {
    match level {
        ParsedHeadingLevel::H1 => HeadingLevel::H1,
        ParsedHeadingLevel::H2 => HeadingLevel::H2,
        ParsedHeadingLevel::H3 => HeadingLevel::H3,
        ParsedHeadingLevel::H4 => HeadingLevel::H4,
        ParsedHeadingLevel::H5 => HeadingLevel::H5,
        ParsedHeadingLevel::H6 => HeadingLevel::H6,
    }
}

fn table_alignment(alignment: ParsedAlignment) -> TableAlignment {
    match alignment {
        ParsedAlignment::None => TableAlignment::None,
        ParsedAlignment::Left => TableAlignment::Left,
        ParsedAlignment::Center => TableAlignment::Center,
        ParsedAlignment::Right => TableAlignment::Right,
    }
}

fn make_inert(text: &str) -> String {
    let mut inert = String::with_capacity(text.len());

    for character in text.chars() {
        let inert_character = match character {
            '\n' | '\r' => ' ',
            '\t' => '⇥',
            '\u{00}'..='\u{1f}' => {
                char::from_u32(u32::from(character) + 0x2400).expect("control picture exists")
            }
            '\u{7f}' => '␡',
            _ if character.is_control() => {
                write!(inert, "\\u{{{:04X}}}", u32::from(character))
                    .expect("writing to a String cannot fail");
                continue;
            }
            _ => character,
        };
        inert.push(inert_character);
    }

    inert
}

fn make_code_inert(text: &str) -> String {
    let mut inert = String::with_capacity(text.len());

    for character in text.chars() {
        match character {
            '\n' | '\t' => inert.push(character),
            '\r' => inert.push(' '),
            '\u{00}'..='\u{1f}' => inert.push(
                char::from_u32(u32::from(character) + 0x2400).expect("control picture exists"),
            ),
            '\u{7f}' => inert.push('␡'),
            _ if character.is_control() => {
                write!(inert, "\\u{{{:04X}}}", u32::from(character))
                    .expect("writing to a String cannot fail");
            }
            _ => inert.push(character),
        }
    }

    inert
}
