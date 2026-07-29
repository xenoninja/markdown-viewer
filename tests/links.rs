use mdview::{
    BlockKind, BrowserResult, Document, Effect, Harness, PaneFocus, SemanticPosition, layout,
};
use ratatui::style::Modifier;

#[test]
fn link_labels_are_styled_without_inline_destinations() {
    let document = Document::parse("See [docs](https://example.com/path) now.\n");
    let block = &document.blocks()[0];
    assert_eq!(block.text(), "See docs now.");
    let label = block
        .spans()
        .iter()
        .find(|span| span.text() == "docs")
        .expect("link label span");
    assert_eq!(label.link_target(), Some("https://example.com/path"));

    let rendered = layout(&document, 40);
    assert_eq!(rendered.rows()[0].text(), "See docs now.");
    let cell = rendered.rows()[0]
        .cells()
        .iter()
        .find(|cell| cell.symbol() == "d")
        .expect("link label cell");
    assert!(cell.style().is_link());
    assert_eq!(cell.link_target(), Some("https://example.com/path"));
}

#[test]
fn reading_cursor_on_link_shows_destination_in_status_bar() {
    let document = Document::parse("See [docs](https://example.com/path) now.\n");
    let mut harness = Harness::new(document, 40, 3);
    // Spaces are non-navigable: "See" then "docs".
    harness.keys("w");

    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 4
        })
    );
    assert!(
        harness
            .modifier_at(SemanticPosition {
                block: 0,
                grapheme: 4
            })
            .is_some_and(|modifier| modifier.contains(Modifier::UNDERLINED))
    );
    assert_eq!(
        harness.frame().lines().last(),
        Some("https://example.com/path")
    );
}

#[test]
fn gx_jumps_to_in_document_heading_fragment_and_records_history() {
    let document = Document::parse(
        "Jump to [section](#target-section).\n\n\
         # Intro\n\n\
         filler\n\n\
         # Target Section\n\n\
         body\n",
    );
    let mut harness = Harness::new(document, 48, 8);
    let prior = Some(SemanticPosition {
        block: 0,
        grapheme: 8,
    });
    let target = Some(SemanticPosition {
        block: 3,
        grapheme: 0,
    });
    // "Jump" → "to" → "section"
    harness.keys("2w");
    assert_eq!(harness.cursor(), prior);

    harness.keys("gx");

    assert_eq!(harness.cursor(), target);
    assert_eq!(harness.current_section(), target);
    harness.control('o');
    assert_eq!(harness.cursor(), prior);
    harness.control('i');
    assert_eq!(harness.cursor(), target);
}

#[test]
fn duplicate_heading_fragments_are_disambiguated_compatibly() {
    let document = Document::parse(
        "[first](#hello) [second](#hello-1)\n\n\
         # Hello\n\n\
         # Hello\n",
    );
    let mut harness = Harness::new(document, 48, 6);
    let first = Some(SemanticPosition {
        block: 1,
        grapheme: 0,
    });
    let second = Some(SemanticPosition {
        block: 2,
        grapheme: 0,
    });

    harness.keys("gx");
    assert_eq!(harness.cursor(), first);

    harness.keys("gg");
    harness.keys("7l");
    harness.keys("gx");
    assert_eq!(harness.cursor(), second);
}

#[test]
fn gx_on_http_link_requests_browser_effect_without_moving_cursor() {
    let document = Document::parse("Open [site](https://example.com/docs).\n");
    let mut harness = Harness::new(document, 40, 3);
    harness.keys("w");
    let prior = harness.cursor();

    harness.keys("gx");

    assert_eq!(harness.cursor(), prior);
    assert_eq!(
        harness.take_effects(),
        vec![Effect::OpenBrowser("https://example.com/docs".to_owned())]
    );
}

#[test]
fn relative_and_unsupported_schemes_remain_display_only() {
    let document =
        Document::parse("[rel](./other.md) [mail](mailto:a@b.c) [js](javascript:alert(1))\n");
    let mut harness = Harness::new(document, 64, 3);

    for start in [0usize, 4, 9] {
        harness.keys("gg");
        if start > 0 {
            harness.keys(&"l".repeat(start));
        }
        let prior = harness.cursor();
        harness.keys("gx");
        assert_eq!(harness.cursor(), prior, "cursor moved for start {start}");
        assert!(
            harness.take_effects().is_empty(),
            "effects requested for start {start}"
        );
        assert_eq!(harness.focus(), PaneFocus::Document);
    }

    assert!(!harness.frame().contains("other.md content"));
}

#[test]
fn footnotes_render_and_gx_jumps_with_history() {
    let document = Document::parse(
        "[^1] See note and again.[^note]\n\n\
         # Body\n\n\
         prose\n\n\
         [^1]: First definition.\n\n\
         [^note]: Named definition.\n",
    );

    let blocks = document.blocks();
    let reference = blocks
        .iter()
        .find(|block| block.text().contains("[1]"))
        .expect("footnote reference block");
    assert!(
        reference
            .spans()
            .iter()
            .any(|span| span.text() == "[1]" && span.link_target() == Some("#fn-1"))
    );

    let definition = blocks
        .iter()
        .find(|block| {
            block
                .spans()
                .iter()
                .any(|span| span.text() == "[1]" && span.link_target() == Some("#fnref-1"))
        })
        .expect("footnote definition");
    assert!(definition.text().contains("First definition."));
    assert_ne!(
        definition.kind(),
        BlockKind::Heading(mdview::HeadingLevel::H1)
    );

    let mut harness = Harness::new(document, 48, 10);
    // Document starts on the first footnote reference label "[1]".
    let prior = harness.cursor();
    assert_eq!(
        prior,
        Some(SemanticPosition {
            block: 0,
            grapheme: 0
        })
    );
    assert_eq!(
        harness.frame().lines().last(),
        Some("#fn-1"),
        "status shows footnote destination"
    );

    harness.keys("gx");
    let at_definition = harness.cursor();
    assert_ne!(at_definition, prior);
    assert!(
        harness
            .document()
            .blocks()
            .get(at_definition.expect("definition cursor").block)
            .is_some_and(|block| block.text().contains("First definition."))
    );

    harness.control('o');
    assert_eq!(harness.cursor(), prior);
    harness.control('i');
    assert_eq!(harness.cursor(), at_definition);

    harness.keys("gx");
    assert_eq!(
        harness.cursor(),
        prior,
        "gx on definition returns to the reference"
    );
}

#[test]
fn missing_fragment_and_browser_failure_are_non_fatal_status_messages() {
    let missing = Document::parse("[gone](#no-such-heading)\n\n# Present\n");
    let mut harness = Harness::new(missing, 40, 4);
    harness.keys("gx");
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 0
        })
    );
    assert!(
        harness.frame().lines().last().is_some_and(|line| line
            .to_lowercase()
            .contains("not found")
            || line.to_lowercase().contains("missing")
            || line.to_lowercase().contains("no such")),
        "missing fragment status: {}",
        harness.frame()
    );
    assert!(harness.take_effects().is_empty());

    let web = Document::parse("[site](https://example.com)\n");
    let mut harness = Harness::new(web, 40, 3);
    harness.set_browser_result(BrowserResult::Failed("launcher unavailable".to_owned()));
    harness.keys("gx");
    assert_eq!(
        harness.take_effects(),
        vec![Effect::OpenBrowser("https://example.com".to_owned())]
    );
    assert!(
        harness
            .frame()
            .lines()
            .last()
            .is_some_and(|line| line.to_lowercase().contains("fail")
                || line.to_lowercase().contains("unavailable")
                || line.to_lowercase().contains("browser")),
        "browser failure status: {}",
        harness.frame()
    );
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 0
        })
    );
}

#[test]
fn inspecting_links_images_and_targets_never_requests_network_fetch() {
    let document = Document::parse(
        "![pic](https://example.com/a.png)\n\n\
         [page](https://example.com/page)\n\n\
         [local](./readme.md)\n\n\
         note[^1]\n\n\
         [^1]: body\n",
    );
    let mut harness = Harness::new(document, 48, 8);
    // Merely loading and moving across targets must not open a browser.
    harness.keys("j");
    harness.keys("j");
    harness.keys("j");
    assert!(harness.take_effects().is_empty());
    assert!(harness.frame().contains("page") || harness.frame().contains("pic"));
}
