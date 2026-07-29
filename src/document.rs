use std::fmt::Write as _;

use pulldown_cmark::{
    Alignment as ParsedAlignment, BlockQuoteKind as ParsedBlockQuoteKind, CodeBlockKind, Event,
    HeadingLevel as ParsedHeadingLevel, Options, Parser, Tag, TagEnd,
};

const OBJECT_REPLACEMENT_CHARACTER: &str = "\u{fffc}";

/// An owned, width-independent representation of a Markdown Document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Document {
    blocks: Vec<Block>,
    warnings: Vec<DocumentWarning>,
}

/// A non-fatal condition encountered while loading a Document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentWarning {
    InvalidUtf8Replaced,
}

impl DocumentWarning {
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            Self::InvalidUtf8Replaced => "warning: invalid UTF-8 replaced with �",
        }
    }
}

/// The semantic role of a block in a Rendered Document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockKind {
    Paragraph,
    Heading(HeadingLevel),
    Code,
    FrontMatter,
    ThematicBreak,
    RawHtml,
    Empty,
    Table,
}

/// The declared role of a GitHub Alert.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl AlertKind {
    pub(crate) const fn rendered_label(self) -> &'static str {
        match self {
            Self::Note => "NOTE",
            Self::Tip => "TIP",
            Self::Important => "IMPORTANT",
            Self::Warning => "WARNING",
            Self::Caution => "CAUTION",
        }
    }
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

impl HeadingLevel {
    pub(crate) const fn depth(self) -> usize {
        match self {
            Self::H1 => 1,
            Self::H2 => 2,
            Self::H3 => 3,
            Self::H4 => 4,
            Self::H5 => 5,
            Self::H6 => 6,
        }
    }
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
    alert_kind: Option<AlertKind>,
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

    #[must_use]
    pub fn alert_kind(&self) -> Option<AlertKind> {
        self.alert_kind
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

/// An image reference retained as inert semantic data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Image {
    alt_text: String,
    target: String,
}

impl Image {
    #[must_use]
    pub fn alt_text(&self) -> &str {
        &self.alt_text
    }

    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    pub(crate) fn rendered_text(&self) -> String {
        let alt = if self.alt_text.is_empty() {
            "(no alt text)"
        } else {
            &self.alt_text
        };
        format!("[image: {alt} → {}]", self.target)
    }
}

/// A styled run of inline semantic text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InlineSpan {
    text: String,
    style: InlineStyle,
    link_target: Option<String>,
    image: Option<Image>,
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
        self.image
            .as_ref()
            .map(Image::target)
            .or(self.link_target.as_deref())
    }

    #[must_use]
    pub fn image(&self) -> Option<&Image> {
        self.image.as_ref()
    }
}

impl Document {
    #[must_use]
    pub fn parse(markdown: &str) -> Self {
        let options = Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_TABLES
            | Options::ENABLE_GFM
            | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
            | Options::ENABLE_FOOTNOTES;
        let mut blocks = Vec::new();
        let mut builder = None;
        let mut table = None;
        let mut style = InlineStyle::default();
        let mut link_target = None;
        let mut image = None;
        let mut quote_depth = 0;
        let mut alerts = Vec::new();
        let mut lists = Vec::new();
        let mut items = Vec::new();
        let mut footnote_definition = None;
        let mut footnote_needs_label = false;

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
                        current_alert(&alerts),
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
                    attach_footnote_definition_label(
                        &mut builder,
                        &mut footnote_needs_label,
                        footnote_definition.as_deref(),
                        style,
                    );
                }
                Event::Start(Tag::FootnoteDefinition(name)) => {
                    finish_builder(&mut builder, &mut blocks);
                    footnote_definition = Some(make_inert(&name));
                    footnote_needs_label = true;
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
                Event::Start(Tag::MetadataBlock(_)) => {
                    finish_builder(&mut builder, &mut blocks);
                    builder = Some(BlockBuilder::new(BlockKind::FrontMatter, quote_depth));
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
                Event::Start(Tag::BlockQuote(kind)) => {
                    quote_depth += 1;
                    alerts.push(kind.map(alert_kind));
                }
                Event::Start(Tag::List(start)) => {
                    finish_builder(&mut builder, &mut blocks);
                    if items.last().is_some_and(|item| !item.started) {
                        let empty =
                            block_builder_for_item(&mut items, quote_depth, BlockKind::Empty);
                        blocks.push(finish_with_alert(empty, &alerts));
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
                Event::Start(Tag::Image { dest_url, .. }) => {
                    image = Some(ImageBuilder {
                        alt: String::new(),
                        target: make_inert(&dest_url),
                    });
                }
                Event::FootnoteReference(name) => {
                    let label = make_inert(&name);
                    let target = format!("#fn-{label}");
                    if builder.is_none() && !items.is_empty() {
                        builder = Some(block_builder_for_leaf(&mut items, quote_depth));
                    }
                    attach_footnote_definition_label(
                        &mut builder,
                        &mut footnote_needs_label,
                        footnote_definition.as_deref(),
                        style,
                    );
                    if let Some(builder) = &mut builder {
                        builder.push(&format!("[{label}]"), style, Some(target.as_str()));
                    }
                }
                Event::Text(text) | Event::InlineHtml(text) | Event::Html(text) => {
                    if let Some(image) = &mut image {
                        image.alt.push_str(&make_inert(&text));
                        continue;
                    }
                    if let Some(table) = &mut table {
                        table.push(&make_inert(&text), style, link_target.as_deref());
                        continue;
                    }
                    if builder.is_none() && !items.is_empty() {
                        builder = Some(block_builder_for_leaf(&mut items, quote_depth));
                    }
                    attach_footnote_definition_label(
                        &mut builder,
                        &mut footnote_needs_label,
                        footnote_definition.as_deref(),
                        style,
                    );
                    if let Some(builder) = &mut builder {
                        let text = match builder.kind {
                            BlockKind::Code | BlockKind::FrontMatter => make_code_inert(&text),
                            BlockKind::RawHtml => make_literal_inert(&text),
                            _ => make_inert(&text),
                        };
                        builder.push(&text, style, link_target.as_deref());
                    }
                }
                Event::Code(text) => {
                    if let Some(image) = &mut image {
                        image.alt.push_str(&make_inert(&text));
                        continue;
                    }
                    if let Some(table) = &mut table {
                        let mut code_style = style;
                        code_style.inline_code = true;
                        table.push(&make_inert(&text), code_style, link_target.as_deref());
                        continue;
                    }
                    if builder.is_none() && !items.is_empty() {
                        builder = Some(block_builder_for_leaf(&mut items, quote_depth));
                    }
                    attach_footnote_definition_label(
                        &mut builder,
                        &mut footnote_needs_label,
                        footnote_definition.as_deref(),
                        style,
                    );
                    if let Some(builder) = &mut builder {
                        let mut code_style = style;
                        code_style.inline_code = true;
                        builder.push(&make_inert(&text), code_style, link_target.as_deref());
                    }
                }
                Event::SoftBreak => {
                    if let Some(image) = &mut image {
                        image.alt.push(' ');
                    } else if let Some(table) = &mut table {
                        table.push(" ", style, link_target.as_deref());
                    } else if let Some(builder) = &mut builder {
                        builder.push(" ", style, link_target.as_deref());
                    }
                }
                Event::HardBreak => {
                    if let Some(image) = &mut image {
                        image.alt.push(' ');
                    } else if let Some(table) = &mut table {
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
                    TagEnd::Paragraph
                    | TagEnd::Heading(_)
                    | TagEnd::HtmlBlock
                    | TagEnd::CodeBlock
                    | TagEnd::MetadataBlock(_),
                ) => {
                    finish_builder(&mut builder, &mut blocks);
                }
                Event::End(TagEnd::BlockQuote(_)) => {
                    quote_depth = quote_depth.saturating_sub(1);
                    alerts.pop();
                }
                Event::End(TagEnd::Item) => {
                    finish_builder(&mut builder, &mut blocks);
                    if items.last().is_some_and(|item| !item.started) {
                        let empty =
                            block_builder_for_item(&mut items, quote_depth, BlockKind::Empty);
                        blocks.push(finish_with_alert(empty, &alerts));
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
                Event::End(TagEnd::FootnoteDefinition) => {
                    finish_builder(&mut builder, &mut blocks);
                    footnote_definition = None;
                    footnote_needs_label = false;
                }
                Event::End(TagEnd::Image) => {
                    let image = image.take().expect("image end follows image start");
                    if let Some(table) = &mut table {
                        table.push_image(image, style);
                    } else {
                        if builder.is_none() && !items.is_empty() {
                            builder = Some(block_builder_for_leaf(&mut items, quote_depth));
                        }
                        attach_footnote_definition_label(
                            &mut builder,
                            &mut footnote_needs_label,
                            footnote_definition.as_deref(),
                            style,
                        );
                        if let Some(builder) = &mut builder {
                            builder.push_image(image, style);
                        }
                    }
                }
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
                    blocks.push(finish_with_alert(rule, &alerts));
                }
                _ => {}
            }

            if let Some(builder) = &mut builder
                && builder.alert_kind.is_none()
            {
                builder.alert_kind = current_alert(&alerts);
            }
        }

        finish_builder(&mut builder, &mut blocks);
        Self {
            blocks,
            warnings: Vec::new(),
        }
    }

    #[must_use]
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    #[must_use]
    pub fn warnings(&self) -> &[DocumentWarning] {
        &self.warnings
    }

    pub(crate) fn add_warning(&mut self, warning: DocumentWarning) {
        self.warnings.push(warning);
    }
}

#[derive(Debug)]
struct BlockBuilder {
    kind: BlockKind,
    spans: Vec<InlineSpan>,
    quote_depth: usize,
    list_item: Option<ListItem>,
    language: Option<String>,
    alert_kind: Option<AlertKind>,
}

#[derive(Debug)]
struct ImageBuilder {
    alt: String,
    target: String,
}

impl BlockBuilder {
    fn new(kind: BlockKind, quote_depth: usize) -> Self {
        Self {
            kind,
            spans: Vec::new(),
            quote_depth,
            list_item: None,
            language: None,
            alert_kind: None,
        }
    }

    fn push(&mut self, text: &str, style: InlineStyle, link_target: Option<&str>) {
        if text.is_empty() {
            return;
        }

        if let Some(last) = self.spans.last_mut()
            && last.image.is_none()
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
            image: None,
        });
    }

    fn push_image(&mut self, image: ImageBuilder, style: InlineStyle) {
        self.spans.push(InlineSpan {
            text: OBJECT_REPLACEMENT_CHARACTER.to_owned(),
            style,
            link_target: None,
            image: Some(Image {
                alt_text: image.alt,
                target: image.target,
            }),
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
            alert_kind: self.alert_kind,
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
    alert_kind: Option<AlertKind>,
}

impl TableBuilder {
    fn new(
        alignments: Vec<TableAlignment>,
        quote_depth: usize,
        list_item: Option<ListItem>,
        alert_kind: Option<AlertKind>,
    ) -> Self {
        Self {
            alignments,
            rows: Vec::new(),
            row: None,
            cell: None,
            quote_depth,
            list_item,
            alert_kind,
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

    fn push_image(&mut self, image: ImageBuilder, style: InlineStyle) {
        if self.cell.is_none() {
            self.start_cell();
        }
        self.cell
            .as_mut()
            .expect("table content has a cell")
            .push_image(image, style);
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
            alert_kind: self.alert_kind,
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
    pub(crate) fn rendered_text(self) -> String {
        match self {
            Self::Unordered => "• ".to_owned(),
            Self::Ordered(number) => format!("{number}. "),
            Self::Task {
                checked,
                number: None,
            } => format!("{} ", if checked { '☑' } else { '☐' }),
            Self::Task {
                checked,
                number: Some(number),
            } => format!("{number}. {} ", if checked { '☑' } else { '☐' }),
        }
    }

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

fn finish_with_alert(mut builder: BlockBuilder, alerts: &[Option<AlertKind>]) -> Block {
    builder.alert_kind = current_alert(alerts);
    builder.finish()
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

fn alert_kind(kind: ParsedBlockQuoteKind) -> AlertKind {
    match kind {
        ParsedBlockQuoteKind::Note => AlertKind::Note,
        ParsedBlockQuoteKind::Tip => AlertKind::Tip,
        ParsedBlockQuoteKind::Important => AlertKind::Important,
        ParsedBlockQuoteKind::Warning => AlertKind::Warning,
        ParsedBlockQuoteKind::Caution => AlertKind::Caution,
    }
}

fn current_alert(alerts: &[Option<AlertKind>]) -> Option<AlertKind> {
    alerts.iter().rev().copied().flatten().next()
}

fn attach_footnote_definition_label(
    builder: &mut Option<BlockBuilder>,
    footnote_needs_label: &mut bool,
    footnote_definition: Option<&str>,
    style: InlineStyle,
) {
    if !*footnote_needs_label {
        return;
    }
    let Some(label) = footnote_definition else {
        return;
    };
    let Some(builder) = builder.as_mut() else {
        return;
    };
    let reverse = format!("#fnref-{label}");
    builder.push(&format!("[{label}]"), style, Some(reverse.as_str()));
    builder.push(" ", style, None);
    *footnote_needs_label = false;
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

fn make_literal_inert(text: &str) -> String {
    let mut inert = String::with_capacity(text.len());

    for character in text.chars() {
        match character {
            '\n' => inert.push(character),
            '\r' => inert.push(' '),
            '\t' => inert.push('⇥'),
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
