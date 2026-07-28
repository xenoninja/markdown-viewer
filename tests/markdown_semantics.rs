use mdview::{BlockKind, Document, HeadingLevel, ListMarker};

#[test]
fn headings_and_inline_formatting_keep_meaning_without_authoring_markers() {
    let document = Document::parse(
        "# Read *carefully*, **please**, ~~later~~ and [`follow`](https://example.com).\n",
    );

    let heading = &document.blocks()[0];
    assert_eq!(heading.kind(), BlockKind::Heading(HeadingLevel::H1));
    assert_eq!(heading.text(), "Read carefully, please, later and follow.");

    let carefully = heading
        .spans()
        .iter()
        .find(|span| span.text() == "carefully")
        .expect("emphasis span");
    assert!(carefully.style().is_emphasis());

    let please = heading
        .spans()
        .iter()
        .find(|span| span.text() == "please")
        .expect("strong span");
    assert!(please.style().is_strong());

    let later = heading
        .spans()
        .iter()
        .find(|span| span.text() == "later")
        .expect("strikethrough span");
    assert!(later.style().is_strikethrough());

    let follow = heading
        .spans()
        .iter()
        .find(|span| span.text() == "follow")
        .expect("linked inline-code span");
    assert!(follow.style().is_inline_code());
    assert_eq!(follow.link_target(), Some("https://example.com"));
}

#[test]
fn ordered_unordered_nested_and_task_lists_keep_hierarchy_and_markers() {
    let document = Document::parse(
        "3. first\n4. second\n   - nested\n     - [x] complete\n     - [ ] pending\n",
    );

    let expected = [
        (
            BlockKind::ListItem {
                depth: 1,
                marker: ListMarker::Ordered(3),
                continuation: false,
            },
            "first",
        ),
        (
            BlockKind::ListItem {
                depth: 1,
                marker: ListMarker::Ordered(4),
                continuation: false,
            },
            "second",
        ),
        (
            BlockKind::ListItem {
                depth: 2,
                marker: ListMarker::Unordered,
                continuation: false,
            },
            "nested",
        ),
        (
            BlockKind::ListItem {
                depth: 3,
                marker: ListMarker::Task {
                    checked: true,
                    number: None,
                },
                continuation: false,
            },
            "complete",
        ),
        (
            BlockKind::ListItem {
                depth: 3,
                marker: ListMarker::Task {
                    checked: false,
                    number: None,
                },
                continuation: false,
            },
            "pending",
        ),
    ];

    assert_eq!(document.blocks().len(), expected.len());
    for (block, (kind, text)) in document.blocks().iter().zip(expected) {
        assert_eq!(block.kind(), kind);
        assert_eq!(block.text(), text);
    }
}

#[test]
fn breaks_quotes_inline_code_and_thematic_breaks_are_semantic() {
    let document = Document::parse("soft\nwrap  \nhard break\n\n> quoted `code`\n\n---\n");

    assert_eq!(document.blocks()[0].kind(), BlockKind::Paragraph);
    assert_eq!(document.blocks()[0].text(), "soft wrap\nhard break");

    let quote = &document.blocks()[1];
    assert_eq!(quote.kind(), BlockKind::Paragraph);
    assert_eq!(quote.quote_depth(), 1);
    assert_eq!(quote.text(), "quoted code");
    assert!(
        quote
            .spans()
            .iter()
            .find(|span| span.text() == "code")
            .expect("inline code")
            .style()
            .is_inline_code()
    );

    assert_eq!(document.blocks()[2].kind(), BlockKind::ThematicBreak);
    assert_eq!(document.blocks()[2].text(), "");
}

#[test]
fn skipped_heading_levels_remain_declared_without_synthetic_blocks() {
    let document = Document::parse("# Parent\n\n#### Declared child\n");

    assert_eq!(document.blocks().len(), 2);
    assert_eq!(
        document.blocks()[0].kind(),
        BlockKind::Heading(HeadingLevel::H1)
    );
    assert_eq!(
        document.blocks()[1].kind(),
        BlockKind::Heading(HeadingLevel::H4)
    );
}

#[test]
fn controls_are_inert_inside_every_supported_textual_construct() {
    let fixtures = [
        "# head\u{1b}",
        "plain\u{1b}",
        "*em\u{1b}*",
        "**strong\u{1b}**",
        "~~strike\u{1b}~~",
        "`code\u{1b}`",
        "> quote\u{1b}",
        "- item\u{1b}",
        "- [x] task\u{1b}",
        "[link\u{1b}](https://example.com/a\u{7})",
    ];

    for markdown in fixtures {
        let document = Document::parse(markdown);
        let text = document
            .blocks()
            .iter()
            .map(mdview::Block::text)
            .collect::<String>();
        assert!(!text.chars().any(char::is_control), "{markdown:?}");
        assert!(text.contains('␛'), "{markdown:?}");

        for span in document.blocks().iter().flat_map(mdview::Block::spans) {
            assert!(
                span.link_target()
                    .is_none_or(|target| !target.chars().any(char::is_control)),
                "{markdown:?}"
            );
        }
    }
}

#[test]
fn an_inline_code_only_list_item_is_not_dropped() {
    let document = Document::parse("- `cargo test`\n");

    assert_eq!(document.blocks().len(), 1);
    assert!(
        document.blocks()[0]
            .spans()
            .iter()
            .any(|span| span.text() == "cargo test" && span.style().is_inline_code())
    );
}

#[test]
fn list_item_continuations_keep_their_parent_hierarchy() {
    let document = Document::parse("- first paragraph\n\n  continuation paragraph\n");

    assert_eq!(document.blocks().len(), 2);
    assert_eq!(
        document.blocks()[0].kind(),
        BlockKind::ListItem {
            depth: 1,
            marker: ListMarker::Unordered,
            continuation: false,
        }
    );
    assert_eq!(
        document.blocks()[1].kind(),
        BlockKind::ListItem {
            depth: 1,
            marker: ListMarker::Unordered,
            continuation: true,
        }
    );
    assert_eq!(document.blocks()[0].text(), "first paragraph");
    assert_eq!(document.blocks()[1].text(), "continuation paragraph");
}
