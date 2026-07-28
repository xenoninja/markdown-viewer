use std::fmt::Write as _;

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

/// An owned, width-independent representation of a Markdown Document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Document {
    blocks: Vec<Block>,
}

/// Semantic content that can be laid out independently of Markdown parsing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Block {
    Paragraph(String),
    RawHtml(String),
}

impl Document {
    #[must_use]
    pub fn parse(markdown: &str) -> Self {
        let mut blocks = Vec::new();
        let mut paragraph = None;

        for event in Parser::new(markdown) {
            match event {
                Event::Start(Tag::Paragraph) => paragraph = Some(String::new()),
                Event::Text(text) | Event::Code(text) | Event::InlineHtml(text) => {
                    if let Some(paragraph) = &mut paragraph {
                        paragraph.push_str(&make_inert(&text));
                    }
                }
                Event::Html(html) => {
                    let html = make_inert(&html);
                    if let Some(paragraph) = &mut paragraph {
                        paragraph.push_str(&html);
                    } else {
                        blocks.push(Block::RawHtml(html.trim_end().to_owned()));
                    }
                }
                Event::SoftBreak | Event::HardBreak => {
                    if let Some(paragraph) = &mut paragraph {
                        paragraph.push(' ');
                    }
                }
                Event::End(TagEnd::Paragraph) => {
                    if let Some(paragraph) = paragraph.take() {
                        blocks.push(Block::Paragraph(paragraph));
                    }
                }
                _ => {}
            }
        }

        Self { blocks }
    }

    #[must_use]
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
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
