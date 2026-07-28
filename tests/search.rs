use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mdview::{Document, Harness, PaneFocus, SemanticPosition};
use ratatui::style::Modifier;

#[test]
fn search_prompt_preserves_the_reading_cursor_and_escape_cancels_it() {
    let mut harness = Harness::new(Document::parse("# Heading\n\nalpha needle omega"), 64, 5);
    harness.keys("}6l");
    harness.control('w');
    harness.keys("h");
    let cursor = harness.cursor();
    assert_eq!(harness.focus(), PaneFocus::Outline);

    harness.keys("/needle");

    assert_eq!(harness.cursor(), cursor);
    assert_eq!(harness.frame().lines().last(), Some("/needle"));

    harness.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    assert_eq!(harness.cursor(), cursor);
    assert_eq!(harness.focus(), PaneFocus::Outline);
    assert!(!harness.frame().contains("/needle"));
}

#[test]
fn confirming_a_literal_query_moves_to_the_match_and_updates_the_current_section() {
    let document =
        Document::parse("# First\n\nNothing here.\n\n# Second\n\nprefix a+b[c] suffix\n");
    let mut harness = Harness::new(document, 64, 8);

    harness.keys("/a+b[c]");
    harness.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let match_start = SemanticPosition {
        block: 3,
        grapheme: 7,
    };
    assert_eq!(harness.cursor(), Some(match_start));
    assert_eq!(
        harness.current_section(),
        Some(SemanticPosition {
            block: 2,
            grapheme: 0,
        })
    );
    assert!(
        harness
            .modifier_at(match_start)
            .is_some_and(|modifier| modifier.contains(Modifier::UNDERLINED)),
        "matches remain identifiable without color"
    );
}

#[test]
fn search_uses_unicode_smart_case_and_repeated_navigation_wraps() {
    let document = Document::parse("CAFÉ\n\ncafé\n\nCafé\n");
    let mut harness = Harness::new(document, 24, 4);

    harness.keys("/café");
    harness.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 1,
            grapheme: 0,
        })
    );

    harness.keys("n");
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 2,
            grapheme: 0,
        })
    );
    harness.keys("n");
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 0,
        })
    );
    harness.keys("N");
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 2,
            grapheme: 0,
        })
    );

    harness.keys("/CAFÉ");
    harness.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 0,
        }),
        "uppercase in the query makes matching case-sensitive"
    );
}

#[test]
fn repeated_matches_within_one_block_are_independently_navigable() {
    let mut harness = Harness::new(Document::parse("target x target x target"), 32, 3);

    harness.keys("/target");
    harness.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(harness.cursor().map(|position| position.grapheme), Some(9));

    harness.keys("n");
    assert_eq!(harness.cursor().map(|position| position.grapheme), Some(18));
    harness.keys("n");
    assert_eq!(harness.cursor().map(|position| position.grapheme), Some(0));
    harness.keys("N");
    assert_eq!(harness.cursor().map(|position| position.grapheme), Some(18));
}

#[test]
fn search_spans_soft_wrapping_and_survives_resize() {
    let document = Document::parse("alpha beta gamma delta\n\nfiller\n\nalpha beta gamma delta\n");
    let mut harness = Harness::new(document, 9, 3);
    harness.keys("G");

    harness.keys("/beta gamma");
    harness.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let first_match = SemanticPosition {
        block: 0,
        grapheme: 6,
    };
    assert_eq!(harness.cursor(), Some(first_match));

    harness.resize(18, 4);
    assert_eq!(harness.cursor(), Some(first_match));
    assert!(
        harness
            .modifier_at(first_match)
            .is_some_and(|modifier| modifier.contains(Modifier::UNDERLINED))
    );

    harness.keys("n");
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 2,
            grapheme: 6,
        })
    );
}

#[test]
fn search_indexes_heterogeneous_rendered_blocks_and_image_placeholders() {
    let document = Document::parse(
        "---\nproject: metadata-token\n---\n\n\
         wrapped prose-token here\n\n\
         ```rust\ncode_token();\n```\n\n\
         | Key | Value |\n|---|---|\n| table-token | cell |\n\n\
         > [!WARNING]\n> alert-token body\n\n\
         <aside>html-token</aside>\n\n\
         ![system diagram](diagram-token.svg)\n",
    );
    let mut harness = Harness::new(document, 80, 8);

    for (query, block) in [
        ("metadata-token", 0),
        ("prose-token", 1),
        ("code_token();", 2),
        ("table-token", 3),
        ("alert-token", 4),
        ("html-token", 5),
        ("diagram-token.svg", 6),
    ] {
        harness.keys(&format!("/{query}"));
        harness.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            harness.cursor().map(|position| position.block),
            Some(block),
            "query {query:?}"
        );
        assert!(harness.cursor_is_highlighted(), "query {query:?}");
    }
}

#[test]
fn absent_queries_leave_the_cursor_in_place_and_ignore_markdown_decoration() {
    let document = Document::parse("before **visible** after\n\n| A | B |\n|---|---|\n| x | y |\n");
    let mut harness = Harness::new(document, 32, 6);
    harness.keys("8l");
    let cursor = harness.cursor();

    harness.keys("/**");
    harness.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(harness.cursor(), cursor);
    assert!(harness.frame().contains("Pattern not found: **"));
    harness.keys("nN");
    assert_eq!(harness.cursor(), cursor);

    harness.keys("/┌");
    harness.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(harness.cursor(), cursor, "table borders are decorative");
}

#[test]
fn search_uses_rendered_whitespace_and_does_not_cross_table_cells() {
    let document = Document::parse(
        "alpha   beta\n\n\
         | A | B |\n|---|---|\n| one | two |\n| three | four |\n",
    );
    let mut harness = Harness::new(document, 32, 7);
    harness.keys("G");

    harness.keys("/alpha beta");
    harness.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 0,
        }),
        "prose whitespace is searched as readers see it"
    );

    harness.keys("/two three");
    harness.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        harness.frame().contains("Pattern not found: two three"),
        "matches cannot bridge distinct table cells or rows"
    );
}

#[test]
fn lowercase_queries_use_full_unicode_case_folding() {
    let mut harness = Harness::new(Document::parse("STRASSE\n\nStraße\n"), 24, 3);

    harness.keys("/strasse");
    harness.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 1,
            grapheme: 0,
        })
    );
}

#[test]
fn escape_restores_the_viewport_from_before_the_search_prompt() {
    let markdown = (0..20)
        .map(|index| format!("paragraph {index}"))
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut harness = Harness::new(Document::parse(&markdown), 20, 4);
    harness.keys("G");
    let viewport = harness.viewport();

    harness.keys("/unfinished");
    assert_ne!(harness.viewport(), viewport);
    harness.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    assert_eq!(harness.viewport(), viewport);

    harness.keys("/missing");
    harness.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(harness.frame().contains("Pattern not found: m"));
    harness.keys("/replacement");
    harness.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(harness.frame().contains("Pattern not found: m"));
}

#[test]
fn generated_semantic_labels_and_non_text_placeholders_are_searchable() {
    let document = Document::parse(
        "---\ntitle: plain\n---\n\n\
         > [!WARNING]\n> take care\n\n\
         - [x] finished\n\n\
         ---\n",
    );
    let mut harness = Harness::new(document, 48, 8);

    for (query, block) in [("metadata", 0), ("warning", 1), ("☑", 2), ("─", 3)] {
        harness.keys(&format!("/{query}"));
        harness.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            harness.cursor().map(|position| position.block),
            Some(block),
            "query {query:?}"
        );
    }
}

#[test]
fn generated_label_highlights_exclude_borders_and_continuation_markers() {
    let mut alert = Harness::new(Document::parse("> [!WARNING]\n> take care\n"), 32, 3);
    alert.keys("/warning");
    alert.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(
        alert
            .screen_modifier(0, 0)
            .is_some_and(|modifier| modifier.contains(Modifier::UNDERLINED))
    );
    assert!(
        alert
            .screen_modifier(8, 0)
            .is_some_and(|modifier| !modifier.contains(Modifier::UNDERLINED)),
        "the Alert border is decorative"
    );

    let mut list = Harness::new(
        Document::parse("- first paragraph\n\n  continuation paragraph\n\n- third item\n"),
        32,
        5,
    );
    list.keys("/•");
    list.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        list.cursor().map(|position| position.block),
        Some(2),
        "continuation indentation must not create an invisible marker match"
    );

    let mut compound = Harness::new(Document::parse("> [!WARNING]\n> - listed danger\n"), 32, 3);
    compound.keys("/warning •");
    compound.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        compound.frame().contains("Pattern not found: warning •"),
        "a query cannot bridge a decorative Alert divider"
    );
}

#[test]
fn visible_code_whitespace_is_a_literal_search_match() {
    let document = Document::parse("before\n\n```\n  indented\n```\n");
    let mut harness = Harness::new(document, 24, 4);

    harness.keys("/  ");
    harness.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 1,
            grapheme: 2,
        })
    );
    assert!(!harness.frame().contains("Pattern not found"));
}
