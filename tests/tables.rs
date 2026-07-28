use mdview::{Document, Harness, SemanticPosition, layout};

#[test]
fn aligned_table_renders_as_a_grid_with_inline_semantics() {
    let document = Document::parse(
        "| Name | Count | State |\n| :--- | ---: | :---: |\n| *alpha* | 2 | `ok` |\n",
    );
    let harness = Harness::new(document.clone(), 40, 8);

    assert_eq!(
        harness.frame(),
        concat!(
            "┌───────┬───────┬───────┐\n",
            "│ Name  │ Count │ State │\n",
            "├───────┼───────┼───────┤\n",
            "│ alpha │     2 │  ok   │\n",
            "└───────┴───────┴───────┘\n",
            "\n",
            "\n",
            ""
        )
    );

    let rendered = layout(&document, 40);
    let alpha = rendered
        .rows()
        .iter()
        .flat_map(|row| row.cells())
        .find(|cell| cell.symbol() == "a" && cell.style().is_emphasis())
        .expect("emphasized alpha is rendered");
    let inline_code = rendered
        .rows()
        .iter()
        .flat_map(|row| row.cells())
        .find(|cell| cell.symbol() == "o" && cell.style().is_inline_code())
        .expect("inline code is rendered");
    assert!(alpha.style().is_emphasis());
    assert!(inline_code.style().is_inline_code());
    assert!(
        harness
            .modifier_at(alpha.position())
            .expect("alpha is visible")
            .contains(ratatui::style::Modifier::ITALIC)
    );
    assert!(
        harness
            .modifier_at(inline_code.position())
            .expect("inline code is visible")
            .contains(ratatui::style::Modifier::UNDERLINED)
    );
}

#[test]
fn reading_cursor_reveals_wide_tables_with_independent_viewports() {
    let markdown = concat!(
        "| A | B | C | D |\n",
        "|---|---|---|---|\n",
        "| a | b | c | d |\n\n",
        "| W | X | Y | Z |\n",
        "|---|---|---|---|\n",
        "| w | x | y | z |\n",
    );
    let document = Document::parse(markdown);
    let rendered = layout(&document, 9);
    for column in 0..9 {
        assert_eq!(
            rendered.position_at(0, column),
            None,
            "top border cannot receive the Reading Cursor"
        );
    }

    let mut harness = Harness::new(document, 9, 10);
    harness.keys("3l");
    let first_scrolled = harness.frame();
    assert!(harness.cursor_is_highlighted());
    assert!(
        first_scrolled
            .lines()
            .nth(1)
            .expect("first header")
            .contains('D')
    );

    harness.keys("5l3l");
    let both_scrolled = harness.frame();
    assert!(harness.cursor_is_highlighted());
    assert_eq!(
        first_scrolled.lines().take(5).collect::<Vec<_>>(),
        both_scrolled.lines().take(5).collect::<Vec<_>>(),
        "moving into another table leaves the first viewport unchanged"
    );
    assert!(
        both_scrolled
            .lines()
            .nth(6)
            .expect("second header")
            .contains('Z')
    );
}

#[test]
fn vertical_motion_skips_table_borders() {
    let document = Document::parse("| A | B |\n|---|---|\n| a | b |\n| c | d |\n");
    let mut harness = Harness::new(document, 20, 8);

    harness.keys("j");

    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 4,
        })
    );
    assert!(harness.cursor_is_highlighted());
}

#[test]
fn unicode_and_multiline_cells_keep_grid_alignment() {
    let document = Document::parse("| K | V |\n|---|---|\n| 界界 | abcdef |\n");
    let rendered = layout(&document, 13);
    let rows = rendered
        .rows()
        .iter()
        .map(mdview::RenderedRow::text)
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        [
            "┌─────┬─────┐",
            "│ K   │ V   │",
            "├─────┼─────┤",
            "│ 界  │ abc │",
            "│ 界  │ def │",
            "└─────┴─────┘",
        ]
    );
    let positions = rendered
        .rows()
        .iter()
        .flat_map(|row| row.cells())
        .filter(|cell| cell.is_navigable())
        .map(|cell| cell.position())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(positions.len(), 10);
    for position in positions {
        let cell = rendered
            .cell_for_position(position)
            .expect("table content remains cursor-reachable");
        assert_eq!(rendered.position_at(cell.row, cell.column), Some(position));
    }
}

#[test]
fn empty_and_uneven_rows_degrade_safely_in_very_narrow_panes() {
    let markdown = "| A | B | C |\n|-|-|-|\n| x ||\n| y | z |\n";
    let document = Document::parse(markdown);
    let rows = layout(&document, 20)
        .rows()
        .iter()
        .map(mdview::RenderedRow::text)
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        [
            "┌───┬───┬───┐",
            "│ A │ B │ C │",
            "├───┼───┼───┤",
            "│ x │   │   │",
            "│ y │ z │   │",
            "└───┴───┴───┘",
        ]
    );

    let harness = Harness::new(Document::parse(markdown), 1, 8);
    assert!(
        harness.cursor_is_highlighted(),
        "the first semantic cell remains visible in a one-column pane"
    );
}

#[test]
fn a_narrow_column_never_splits_a_wide_grapheme() {
    let document = Document::parse("| 界 | A |\n|-|-|\n| 界 | a |\n");
    let rendered = layout(&document, 5);
    let widths = rendered
        .rows()
        .iter()
        .map(mdview::RenderedRow::display_width)
        .collect::<Vec<_>>();

    assert!(
        widths.iter().all(|width| *width == widths[0]),
        "wide graphemes must not make content rows wider than their borders: {widths:?}"
    );
}

#[test]
fn tables_retain_enclosing_quote_and_list_context() {
    let document =
        Document::parse("> | A | B |\n> |-|-|\n> | a | b |\n\n- | C | D |\n  |-|-|\n  | c | d |\n");
    let tables = document
        .blocks()
        .iter()
        .filter(|block| block.kind() == mdview::BlockKind::Table)
        .collect::<Vec<_>>();

    assert_eq!(tables.len(), 2);
    assert_eq!(tables[0].quote_depth(), 1);
    assert!(tables[1].list_item().is_some());

    let rows = layout(&document, 30)
        .rows()
        .iter()
        .map(mdview::RenderedRow::text)
        .collect::<Vec<_>>();
    assert!(rows.iter().any(|row| row.starts_with("│ ┌")));
    assert!(rows.iter().any(|row| row.starts_with("• ┌")));
}

#[test]
fn word_motion_stops_at_the_next_table_cell() {
    let document = Document::parse("| A | B |\n|-|-|\n| alpha | beta |\n");
    let rendered = layout(&document, 30);
    let beta = rendered
        .rows()
        .iter()
        .flat_map(|row| row.cells())
        .find(|cell| cell.symbol() == "b")
        .expect("beta is rendered")
        .position();
    let mut harness = Harness::new(document, 30, 8);
    harness.keys("jw");

    assert_eq!(harness.cursor(), Some(beta));
}
