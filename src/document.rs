use std::fmt::Write as _;

use pulldown_cmark::{Event, HeadingLevel as ParsedHeadingLevel, Options, Parser, Tag, TagEnd};

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
    ListItem {
        depth: usize,
        marker: ListMarker,
        continuation: bool,
        content: ListItemContent,
    },
    ThematicBreak,
    RawHtml,
}

/// The supported block role carried by a list item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListItemContent {
    Paragraph,
    Heading(HeadingLevel),
    ThematicBreak,
    RawHtml,
    Empty,
}

impl BlockKind {
    pub(crate) fn heading_level(self) -> Option<HeadingLevel> {
        match self {
            Self::Heading(level)
            | Self::ListItem {
                content: ListItemContent::Heading(level),
                ..
            } => Some(level),
            _ => None,
        }
    }

    pub(crate) fn is_thematic_break(self) -> bool {
        matches!(
            self,
            Self::ThematicBreak
                | Self::ListItem {
                    content: ListItemContent::ThematicBreak,
                    ..
                }
        )
    }

    pub(crate) fn is_empty_list_item(self) -> bool {
        matches!(
            self,
            Self::ListItem {
                content: ListItemContent::Empty,
                ..
            }
        )
    }
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
        let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
        let mut blocks = Vec::new();
        let mut builder = None;
        let mut style = InlineStyle::default();
        let mut link_target = None;
        let mut quote_depth = 0;
        let mut lists = Vec::new();
        let mut items = Vec::new();

        for event in Parser::new_ext(markdown, options) {
            match event {
                Event::Start(Tag::Paragraph) => {
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
                        block_builder_for_item(
                            &mut items,
                            quote_depth,
                            ListItemContent::Heading(level),
                        )
                    });
                }
                Event::Start(Tag::HtmlBlock) => {
                    builder = Some(if items.is_empty() {
                        BlockBuilder::new(BlockKind::RawHtml, quote_depth)
                    } else {
                        block_builder_for_item(&mut items, quote_depth, ListItemContent::RawHtml)
                    });
                }
                Event::Start(Tag::BlockQuote(_)) => quote_depth += 1,
                Event::Start(Tag::List(start)) => {
                    finish_builder(&mut builder, &mut blocks);
                    if items.last().is_some_and(|item| !item.started) {
                        blocks.push(
                            block_builder_for_item(&mut items, quote_depth, ListItemContent::Empty)
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
                    if builder.is_none() && !items.is_empty() {
                        builder = Some(block_builder_for_leaf(&mut items, quote_depth));
                    }
                    if let Some(builder) = &mut builder {
                        builder.push(&make_inert(&text), style, link_target.as_deref());
                    }
                }
                Event::Code(text) => {
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
                    if let Some(builder) = &mut builder {
                        builder.push(" ", style, link_target.as_deref());
                    }
                }
                Event::HardBreak => {
                    if let Some(builder) = &mut builder {
                        builder.push("\n", style, link_target.as_deref());
                    }
                }
                Event::End(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::HtmlBlock) => {
                    finish_builder(&mut builder, &mut blocks);
                }
                Event::End(TagEnd::BlockQuote(_)) => quote_depth = quote_depth.saturating_sub(1),
                Event::End(TagEnd::Item) => {
                    finish_builder(&mut builder, &mut blocks);
                    if items.last().is_some_and(|item| !item.started) {
                        blocks.push(
                            block_builder_for_item(&mut items, quote_depth, ListItemContent::Empty)
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
                        block_builder_for_item(
                            &mut items,
                            quote_depth,
                            ListItemContent::ThematicBreak,
                        )
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
}

impl BlockBuilder {
    fn new(kind: BlockKind, quote_depth: usize) -> Self {
        Self {
            kind,
            spans: Vec::new(),
            quote_depth,
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
        }
    }
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
        return block_builder_for_item(items, quote_depth, ListItemContent::Paragraph);
    }

    BlockBuilder::new(BlockKind::Paragraph, quote_depth)
}

fn block_builder_for_item(
    items: &mut [ItemContext],
    quote_depth: usize,
    content: ListItemContent,
) -> BlockBuilder {
    let item = items.last_mut().expect("list content is inside an item");
    let continuation = item.started;
    item.started = true;
    BlockBuilder::new(
        BlockKind::ListItem {
            depth: item.depth,
            marker: item.marker,
            continuation,
            content,
        },
        quote_depth,
    )
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
