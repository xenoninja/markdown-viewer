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

fn reachable_positions(rendered: &mdview::RenderedDocument) -> BTreeSet<SemanticPosition> {
    rendered
        .rows()
        .iter()
        .flat_map(|row| row.cells())
        .filter(|cell| cell.is_navigable())
        .map(|cell| cell.position())
        .collect()
}
