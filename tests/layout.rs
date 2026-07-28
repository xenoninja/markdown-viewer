use mdview::{Document, SemanticPosition, layout};
use std::collections::BTreeSet;

#[test]
fn unicode_graphemes_round_trip_between_semantic_positions_and_cells() {
    let document = Document::parse("a e\u{301} 界 👨‍👩‍👧‍👦 z");

    let rendered = layout(&document, 12);

    let family = SemanticPosition {
        block: 0,
        grapheme: 6,
    };
    let cell = rendered
        .cell_for_position(family)
        .expect("family emoji is rendered");
    assert_eq!(cell.position, family);
    assert_eq!(cell.width, 2);
    assert_eq!(rendered.position_at(cell.row, cell.column), Some(family));
    assert_eq!(
        rendered.position_at(cell.row, cell.column + 1),
        Some(family),
        "both terminal columns belong to the same grapheme"
    );
}

#[test]
fn prose_uses_a_centered_reading_column_capped_at_one_hundred_columns() {
    let document = Document::parse(&"word ".repeat(30));

    let rendered = layout(&document, 120);

    assert_eq!(rendered.content_width(), 100);
    assert_eq!(rendered.rows()[0].column(), 10);
    assert!(rendered.rows()[0].display_width() <= 100);
}

#[test]
fn every_reachable_position_round_trips_in_narrow_and_wide_layouts() {
    let document = Document::parse("alpha e\u{301} 界 👨‍👩‍👧‍👦 omega");
    let wide = layout(&document, 120);
    let expected = reachable_positions(&wide);

    for width in [4, 9, 100, 120] {
        let rendered = layout(&document, width);
        assert_eq!(reachable_positions(&rendered), expected);
        for position in &expected {
            let cell = rendered
                .cell_for_position(*position)
                .expect("reachable position has a rendered cell");
            assert_eq!(rendered.position_at(cell.row, cell.column), Some(*position));
        }
    }
}

#[test]
fn an_isolated_zero_width_grapheme_has_a_visible_cursor_cell() {
    let document = Document::parse("\u{301}accent");
    let rendered = layout(&document, 20);
    let position = SemanticPosition {
        block: 0,
        grapheme: 0,
    };

    let cell = rendered
        .cell_for_position(position)
        .expect("isolated combining mark is rendered");
    assert_eq!(cell.width, 1);
    assert_eq!(rendered.position_at(cell.row, cell.column), Some(position));
    assert_eq!(rendered.rows()[0].cells()[0].symbol(), "◌\u{301}");
}

#[test]
fn common_github_markdown_has_semantic_terminal_layout() {
    let document = Document::parse(
        "# One\n### Three\n###### Six\n\n> quoted `code` and [link](https://example.com)\n\n---\n\n- item\n  - nested\n\nsoft  \nhard\n",
    );

    let rendered = layout(&document, 40);
    let rows = rendered
        .rows()
        .iter()
        .map(mdview::RenderedRow::text)
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        [
            "One",
            "Three",
            "Six",
            "│ quoted code and link",
            "────────",
            "• item",
            "  • nested",
            "soft",
            "hard",
        ]
    );

    let one = rendered.rows()[0].cells()[0].style();
    let three = rendered.rows()[1].cells()[0].style();
    let six = rendered.rows()[2].cells()[0].style();
    assert_eq!(one.heading_level(), Some(1));
    assert_eq!(three.heading_level(), Some(3));
    assert_eq!(six.heading_level(), Some(6));
    assert_ne!(one, three);
    assert_ne!(three, six);

    let code = rendered.rows()[3]
        .cells()
        .iter()
        .find(|cell| cell.symbol() == "c")
        .expect("inline code cell");
    assert!(code.style().is_inline_code());
    let link = rendered.rows()[3]
        .cells()
        .iter()
        .rev()
        .find(|cell| cell.symbol() == "l")
        .expect("link label cell");
    assert!(link.style().is_link());
}

#[test]
fn common_markdown_meaning_and_cursor_mappings_survive_reflow() {
    let document = Document::parse(
        "# Heading\n\n> quote with *emphasis* and `code`\n\n---\n\n2. ordered\n   - [ ] nested task\n\n[linked label](https://example.com)\n",
    );
    let expected = reachable_positions(&layout(&document, 80));

    for width in [10, 40, 120] {
        let rendered = layout(&document, width);
        assert_eq!(reachable_positions(&rendered), expected);
        for position in &expected {
            let cell = rendered
                .cell_for_position(*position)
                .expect("semantic position survives reflow");
            assert_eq!(rendered.position_at(cell.row, cell.column), Some(*position));
        }
        assert!(
            rendered
                .rows()
                .iter()
                .flat_map(|row| row.cells())
                .any(|cell| cell.style().is_link())
        );
        assert!(
            rendered
                .rows()
                .iter()
                .flat_map(|row| row.cells())
                .any(|cell| cell.style().is_inline_code())
        );
    }
}

fn reachable_positions(rendered: &mdview::RenderedDocument) -> BTreeSet<SemanticPosition> {
    rendered
        .rows()
        .iter()
        .flat_map(|row| row.cells())
        .filter(|cell| cell.is_navigable())
        .map(|cell| cell.position())
        .collect()
}
