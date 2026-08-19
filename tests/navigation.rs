use mdviewer::{Document, Harness, SemanticPosition, layout};

#[test]
fn reading_cursor_is_visible_and_moves_by_unicode_grapheme() {
    let document = Document::parse("a e\u{301} 界 👨‍👩‍👧‍👦 z");
    let mut harness = Harness::new(document, 24, 4);

    assert!(harness.cursor_is_highlighted());

    harness.keys("l");
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 2,
        }),
        "combining sequence is one Reading Cursor position"
    );

    harness.keys("2l");
    let family = harness.cursor_cell().expect("family emoji is visible");
    assert_eq!(family.width, 2);
    assert!(harness.cursor_is_highlighted());
}

#[test]
fn counted_row_motions_follow_visible_wrapped_rows() {
    let document = Document::parse("one two three four five six");
    let mut harness = Harness::new(document, 9, 3);

    harness.keys("2j");

    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 14,
        }),
        "two rows down lands on the first grapheme of 'four'"
    );
    assert!(harness.cursor_is_highlighted());

    harness.keys("k");
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 8,
        })
    );
}

#[test]
fn counted_row_targets_skip_decorative_spacing() {
    let document = Document::parse("first\n\nsecond\n\nthird");
    let mut harness = Harness::new(document, 40, 5);

    harness.keys("2gg");
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 1,
            grapheme: 0,
        }),
        "counted document-row motion skips block spacing"
    );

    harness.keys("gg2$");
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 1,
            grapheme: 5,
        }),
        "counted row-end motion skips block spacing"
    );

    harness.keys("gg2G");
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 1,
            grapheme: 0,
        }),
        "counted G motion skips block spacing"
    );
}

#[test]
fn horizontal_motion_crosses_soft_wrap_boundaries() {
    let document = Document::parse("one two three");
    let mut harness = Harness::new(document, 7, 2);

    harness.keys("6l");

    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 8,
        }),
        "the next grapheme remains reachable after a soft wrap"
    );
    harness.keys("h");
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 6,
        })
    );
}

#[test]
fn word_line_document_and_paragraph_motions_use_semantic_content() {
    let document = Document::parse("alpha beta gamma\n\nsecond paragraph\n\nlast");
    let mut harness = Harness::new(document, 40, 4);

    harness.keys("2w");
    assert_eq!(harness.cursor().expect("cursor").grapheme, 11);
    harness.keys("b");
    assert_eq!(harness.cursor().expect("cursor").grapheme, 6);
    harness.keys("w");
    assert_eq!(harness.cursor().expect("cursor").grapheme, 11);

    harness.keys("$");
    assert_eq!(harness.cursor().expect("cursor").grapheme, 15);
    harness.keys("0");
    assert_eq!(harness.cursor().expect("cursor").grapheme, 0);

    harness.keys("}");
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 1,
            grapheme: 0,
        })
    );
    harness.keys("{");
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 0,
        })
    );
    harness.keys("}");
    harness.keys("G");
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 2,
            grapheme: 3,
        })
    );
    harness.keys("{");
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 1,
            grapheme: 0,
        }),
        "paragraph-backward stops at the immediately preceding paragraph"
    );
    harness.keys("gg");
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 0,
        })
    );
}

#[test]
fn word_motion_stops_at_the_first_grapheme_of_the_next_paragraph() {
    let document = Document::parse("alpha\n\nbeta");
    let mut harness = Harness::new(document, 40, 3);

    harness.keys("w");

    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 1,
            grapheme: 0,
        })
    );
}

#[test]
fn word_motion_at_trailing_whitespace_keeps_the_cursor_rendered() {
    let document = Document::parse("word \u{2003}");
    let mut harness = Harness::new(document, 20, 2);

    harness.keys("w");

    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 3,
        })
    );
    assert!(harness.cursor_is_highlighted());
}

#[test]
fn page_motions_move_the_cursor_but_viewport_scroll_does_not() {
    let document =
        Document::parse("one two three four five six seven eight nine ten eleven twelve");
    let mut harness = Harness::new(document, 8, 4);

    harness.control('d');
    let after_half_page = harness.cursor().expect("cursor moved");
    assert_eq!(harness.cursor_cell().expect("cursor visible").row, 2);

    harness.control('f');
    assert_ne!(harness.cursor().expect("cursor moved"), after_half_page);
    assert!(harness.cursor_cell().is_some());

    let cursor = harness.cursor();
    let viewport = harness.viewport();
    harness.control('e');
    assert_eq!(harness.cursor(), cursor);
    assert_eq!(harness.viewport(), viewport + 1);
    harness.control('y');
    assert_eq!(harness.cursor(), cursor);
    assert_eq!(harness.viewport(), viewport);
    harness.control('b');
    assert!(harness.cursor_cell().is_some());
}

#[test]
fn resize_preserves_the_semantic_cursor_and_keeps_it_visible() {
    let document = Document::parse("zero one two three four five six seven eight nine ten");
    let mut harness = Harness::new(document, 60, 3);
    harness.keys("25l");
    let position = harness.cursor();

    harness.resize(9, 3);

    assert_eq!(harness.cursor(), position);
    assert!(harness.cursor_cell().is_some());
    assert!(harness.cursor_is_highlighted());
}

#[test]
fn decorative_word_separators_are_skipped_and_resize_stays_stable() {
    let document = Document::parse("one two three");
    let mut harness = Harness::new(document, 20, 2);
    harness.keys("3l");
    let first_letter_of_two = harness.cursor();

    harness.resize(7, 2);

    assert_eq!(
        first_letter_of_two,
        Some(SemanticPosition {
            block: 0,
            grapheme: 4,
        }),
        "horizontal motion skips the rendered separator"
    );
    assert_eq!(harness.cursor(), first_letter_of_two);
    assert!(harness.cursor_cell().is_some());
    assert!(harness.cursor_is_highlighted());
}

#[test]
fn large_counts_clamp_at_document_ends() {
    let document = Document::parse(&"word ".repeat(200));
    let mut harness = Harness::new(document, 10, 3);

    harness.keys("999j");
    let end = harness.cursor();
    harness.keys("999k");

    assert_ne!(harness.cursor(), end);
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 0,
        })
    );
}

#[test]
fn reading_cursor_visits_all_semantic_constructs_and_skips_decoration() {
    let document = Document::parse(
        "# Heading\n\n> quote `code`\n\n---\n\n- item\n  - [x] task\n\nsoft  \nhard [link](https://example.com)\n",
    );
    let rendered = layout(&document, 40);
    let expected = rendered
        .rows()
        .iter()
        .flat_map(|row| row.cells())
        .filter(|cell| cell.is_navigable())
        .map(|cell| cell.position())
        .collect::<Vec<_>>();
    let quote_row = rendered
        .rows()
        .iter()
        .position(|row| row.text().starts_with('│'))
        .expect("quote row");
    let quote_content_column = rendered.rows()[quote_row].column();
    assert_eq!(
        rendered.position_at(quote_row, quote_content_column - 1),
        None,
        "blockquote border is decorative"
    );

    let mut harness = Harness::new(document, 40, 20);
    for position in expected {
        assert_eq!(harness.cursor(), Some(position));
        harness.keys("l");
    }
}

#[test]
fn reading_cursor_remains_visible_on_inline_code() {
    let document = Document::parse("`ab`");
    let harness = Harness::new(document, 20, 2);
    let cursor = harness.cursor().expect("cursor starts on inline code");
    let neighbor = SemanticPosition {
        block: cursor.block,
        grapheme: cursor.grapheme + 1,
    };

    assert_ne!(
        harness.modifier_at(cursor),
        harness.modifier_at(neighbor),
        "cursor styling must differ from ordinary inline-code styling"
    );
    assert!(harness.cursor_is_highlighted());
}

#[test]
fn reading_cursor_reveals_long_code_with_independent_horizontal_viewports() {
    let document = Document::parse("```\nabcdefghijklmnop\n```\n\n```\n0123456789abcdef\n```\n");
    let mut harness = Harness::new(document, 8, 3);

    assert_eq!(harness.frame(), "│ abcdef\n\n│ 012345");

    harness.keys("10l");
    assert_eq!(harness.frame(), "│ fghijk\n\n│ 012345");
    assert!(harness.cursor_is_highlighted());

    harness.keys("j");
    assert_eq!(
        harness.frame(),
        "│ fghijk\n\n│ 012345",
        "moving into another code block leaves both viewports independent"
    );

    harness.keys("k");
    assert_eq!(harness.frame(), "│ fghijk\n\n│ 012345");
    assert!(harness.cursor_is_highlighted());
}

#[test]
fn vertical_code_motion_reveals_a_short_line_after_horizontal_scrolling() {
    let document = Document::parse("```\nabcdefghijklmnop\nxy\n```\n");
    let mut harness = Harness::new(document, 8, 2);
    harness.keys("10l");

    harness.keys("j");

    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 18,
        })
    );
    assert!(harness.cursor_is_highlighted());
    assert!(harness.frame().contains('y'));
}
