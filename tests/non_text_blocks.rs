use mdviewer::{AlertKind, BlockKind, Document, layout};

#[test]
fn front_matter_is_literal_metadata_and_not_a_heading() {
    let document = Document::parse(
        "---\n\
         title: \"Terminal \u{1b}[2J\"\n\
         tags: [Rust, 终端]\n\
         ---\n\
         # Visible heading\n",
    );

    assert_eq!(document.blocks().len(), 2);
    assert_eq!(document.blocks()[0].kind(), BlockKind::FrontMatter);
    assert_eq!(
        document.blocks()[0].text(),
        "title: \"Terminal ␛[2J\"\ntags: [Rust, 终端]\n"
    );
    assert_eq!(
        document.blocks()[1].kind(),
        BlockKind::Heading(mdviewer::HeadingLevel::H1)
    );

    let rows = layout(&document, 48)
        .rows()
        .iter()
        .map(mdviewer::RenderedRow::text)
        .collect::<Vec<_>>();
    assert_eq!(
        rows,
        [
            "metadata │ title: \"Terminal ␛[2J\"",
            "metadata │ tags: [Rust, 终端]",
            "",
            "Visible heading",
        ]
    );
}

#[test]
fn github_alerts_have_distinct_text_labels_without_color() {
    let markdown = [
        ("NOTE", AlertKind::Note),
        ("TIP", AlertKind::Tip),
        ("IMPORTANT", AlertKind::Important),
        ("WARNING", AlertKind::Warning),
        ("CAUTION", AlertKind::Caution),
    ]
    .into_iter()
    .map(|(label, _)| format!("> [!{label}]\n> {label} body\n"))
    .collect::<Vec<_>>()
    .join("\n");

    let document = Document::parse(&markdown);

    assert_eq!(document.blocks().len(), 5);
    for (block, (_, kind)) in document.blocks().iter().zip([
        ("NOTE", AlertKind::Note),
        ("TIP", AlertKind::Tip),
        ("IMPORTANT", AlertKind::Important),
        ("WARNING", AlertKind::Warning),
        ("CAUTION", AlertKind::Caution),
    ]) {
        assert_eq!(block.alert_kind(), Some(kind));
    }
    let rows = layout(&document, 48)
        .rows()
        .iter()
        .map(mdviewer::RenderedRow::text)
        .collect::<Vec<_>>();
    assert_eq!(
        rows,
        [
            "NOTE │ NOTE body",
            "",
            "TIP │ TIP body",
            "",
            "IMPORTANT │ IMPORTANT body",
            "",
            "WARNING │ WARNING body",
            "",
            "CAUTION │ CAUTION body",
        ]
    );
}

#[test]
fn images_are_inert_placeholders_with_alt_text_and_target() {
    let document = Document::parse(
        "before ![diagram \u{1b}]\
         (https://example.com/diagram.svg) after",
    );

    let block = &document.blocks()[0];
    assert_eq!(block.text(), "before \u{fffc} after");
    let image = block
        .spans()
        .iter()
        .find_map(mdviewer::InlineSpan::image)
        .expect("image placeholder span");
    assert_eq!(image.alt_text(), "diagram ␛");
    assert_eq!(image.target(), "https://example.com/diagram.svg");
    assert!(!image.alt_text().chars().any(char::is_control));

    let rows = layout(&document, 80)
        .rows()
        .iter()
        .map(mdviewer::RenderedRow::text)
        .collect::<Vec<_>>();
    assert_eq!(
        rows,
        ["before [image: diagram ␛ → https://example.com/diagram.svg] after"]
    );

    let empty_alt = layout(&Document::parse("![](local.png)"), 40);
    assert_eq!(
        empty_alt.rows()[0].text(),
        "[image: (no alt text) → local.png]"
    );
}

#[test]
fn inline_and_block_html_render_literally_with_controls_inert() {
    let document = Document::parse(
        "press <kbd>Ctrl\u{1b}</kbd> now\n\n\
         <aside>\nraw\t\u{1b}]0;owned\u{7}\n</aside>\n",
    );

    assert_eq!(document.blocks()[0].kind(), BlockKind::Paragraph);
    assert_eq!(document.blocks()[0].text(), "press <kbd>Ctrl␛</kbd> now");
    assert_eq!(document.blocks()[1].kind(), BlockKind::RawHtml);
    assert_eq!(
        document.blocks()[1].text(),
        "<aside>\nraw⇥␛]0;owned␇\n</aside>\n"
    );

    let rows = layout(&document, 48)
        .rows()
        .iter()
        .map(mdviewer::RenderedRow::text)
        .collect::<Vec<_>>();
    assert_eq!(
        rows,
        [
            "press <kbd>Ctrl␛</kbd> now",
            "",
            "<aside>",
            "raw⇥␛]0;owned␇",
            "</aside>",
        ]
    );
    assert!(
        rows.iter()
            .flat_map(|row| row.chars())
            .all(|character| !character.is_control())
    );
}

#[test]
fn malformed_and_control_heavy_constructs_stay_visible_and_safe() {
    let fixtures = [
        "---\nunclosed: 界\u{1b}]0;metadata\u{7}\n",
        "> [!UNKNOWN]\n> 界\u{1b}[31m alert-like text\n",
        "![broken image](https://example.invalid/\u{1b}\n",
        "<broken 界\u{1b}]0;html\u{7}\n",
    ];

    for markdown in fixtures {
        let document = Document::parse(markdown);
        let rendered = layout(&document, 40);
        let visible = rendered
            .rows()
            .iter()
            .map(mdviewer::RenderedRow::text)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(!visible.is_empty(), "{markdown:?}");
        assert!(visible.contains('界') || visible.contains("broken"));
        assert!(
            rendered
                .rows()
                .iter()
                .all(|row| !row.text().chars().any(char::is_control)),
            "terminal control escaped from {markdown:?}"
        );
    }
}

#[test]
fn alert_content_cannot_emit_terminal_control_sequences() {
    let document = Document::parse(
        "> [!WARNING]\n\
         > Unicode 界 and \u{1b}[2J plus \u{1b}]0;owned\u{7}\n",
    );
    let rendered = layout(&document, 80);
    let visible = rendered.rows()[0].text();

    assert_eq!(visible, "WARNING │ Unicode 界 and ␛[2J plus ␛]0;owned␇");
    assert!(!visible.chars().any(char::is_control));
}

#[test]
fn alerts_keep_their_role_for_immediately_finished_thematic_breaks() {
    let document = Document::parse("> [!WARNING]\n> ---\n");

    assert_eq!(document.blocks()[0].alert_kind(), Some(AlertKind::Warning));
    let rows = layout(&document, 40)
        .rows()
        .iter()
        .map(mdviewer::RenderedRow::text)
        .collect::<Vec<_>>();
    assert_eq!(rows, ["WARNING │ ────────"]);
}
