use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mdview::{
    ClipboardResult, Document, Effect, Harness, SelectionMode, SemanticPosition,
};

#[test]
fn characterwise_and_row_selection_start_from_the_reading_cursor() {
    let mut harness = Harness::new(Document::parse("alpha beta"), 20, 2);
    harness.keys("2l");
    let anchor = harness.cursor().expect("cursor");

    harness.keys("v");
    assert_eq!(harness.selection_mode(), Some(SelectionMode::Characterwise));
    assert_eq!(harness.selection_anchor(), Some(anchor));
    assert_eq!(harness.cursor(), Some(anchor));

    harness.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(harness.selection_mode(), None);
    assert_eq!(harness.cursor(), Some(anchor));

    harness.keys("V");
    assert_eq!(harness.selection_mode(), Some(SelectionMode::Row));
    assert_eq!(harness.selection_anchor(), Some(anchor));
}

#[test]
fn escape_cancels_selection_without_moving_or_modifying_the_document() {
    let mut harness = Harness::new(Document::parse("# Title\n\nalpha beta gamma"), 24, 4);
    harness.keys("}wvll");
    let cursor = harness.cursor();
    let content = harness.frame();

    harness.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    assert_eq!(harness.selection_mode(), None);
    assert_eq!(
        harness.cursor(),
        cursor,
        "Esc leaves the Reading Cursor where Selection ended"
    );
    assert_eq!(
        harness.frame(),
        content,
        "cancelling Selection does not modify the Document"
    );
}

#[test]
fn supported_motions_extend_an_active_selection() {
    let mut harness = Harness::new(Document::parse("one two three"), 20, 2);
    harness.keys("v2w");

    assert_eq!(harness.selection_mode(), Some(SelectionMode::Characterwise));
    assert_eq!(
        harness.selection_anchor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 0,
        })
    );
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 8,
        })
    );
    assert!(
        harness.selection_contains(SemanticPosition {
            block: 0,
            grapheme: 4,
        }),
        "positions between the anchor and cursor remain selected"
    );
}

#[test]
fn yank_copies_plain_rendered_text_and_leaves_selection() {
    let mut harness = Harness::new(Document::parse("alpha beta gamma"), 24, 2);
    harness.keys("v4l");
    harness.keys("y");

    assert_eq!(harness.selection_mode(), None);
    assert_eq!(
        harness.take_effects(),
        vec![Effect::WriteClipboard("alpha".to_owned())]
    );
    assert!(
        harness.frame().contains("Copied"),
        "clipboard success is reported in the status bar: {}",
        harness.frame()
    );
}

#[test]
fn yank_preserves_list_and_task_markers() {
    let mut harness = Harness::new(Document::parse("- item one\n- [x] done\n"), 24, 4);

    harness.keys("Vy");
    assert_eq!(
        harness.take_effects(),
        vec![Effect::WriteClipboard("• item one".to_owned())]
    );

    harness.keys("jVy");
    assert_eq!(
        harness.take_effects(),
        vec![Effect::WriteClipboard("☑ done".to_owned())]
    );
}

#[test]
fn yank_preserves_code_tabs_and_omits_decorative_table_borders() {
    let mut code = Harness::new(Document::parse("```\n\tindented\n```\n"), 20, 2);
    code.keys("Vy");
    let yanked = match code.take_effects().pop() {
        Some(Effect::WriteClipboard(text)) => text,
        other => panic!("expected clipboard effect, got {other:?}"),
    };
    assert_eq!(yanked, "\tindented");

    let mut table = Harness::new(
        Document::parse("| a | b |\n| - | - |\n| 1 | 2 |\n"),
        24,
        6,
    );
    table.keys("GVy");
    let yanked = match table.take_effects().pop() {
        Some(Effect::WriteClipboard(text)) => text,
        other => panic!("expected clipboard effect, got {other:?}"),
    };
    assert!(
        yanked.contains('1') && yanked.contains('2'),
        "table cell content is copied: {yanked:?}"
    );
    assert!(
        !yanked.contains('│') && !yanked.contains('─') && !yanked.contains('|'),
        "table borders and markdown pipes are omitted: {yanked:?}"
    );
}

#[test]
fn clipboard_failure_is_reported_truthfully() {
    let mut harness = Harness::new(Document::parse("copy me"), 20, 2);
    harness.set_clipboard_result(ClipboardResult::Failed("clipboard unavailable".into()));
    harness.keys("Vy");

    assert_eq!(
        harness.take_effects(),
        vec![Effect::WriteClipboard("copy me".to_owned())]
    );
    assert!(
        harness.frame().contains("clipboard unavailable")
            || harness.frame().contains("Copy failed"),
        "failure feedback is visible: {}",
        harness.frame()
    );
    assert_eq!(harness.selection_mode(), None);
}

#[test]
fn selection_survives_resize_across_wrapped_and_heterogeneous_blocks() {
    let document = Document::parse("one two three four\n\n```\ncode\n```\n\n| x | y |\n| - | - |\n| 1 | 2 |\n");
    let mut harness = Harness::new(document, 40, 8);
    harness.keys("v2w");
    let anchor = harness.selection_anchor();
    let cursor = harness.cursor();

    harness.resize(9, 8);

    assert_eq!(harness.selection_mode(), Some(SelectionMode::Characterwise));
    assert_eq!(harness.selection_anchor(), anchor);
    assert_eq!(harness.cursor(), cursor);
    assert!(harness.selection_contains(SemanticPosition {
        block: 0,
        grapheme: 4,
    }));
}

#[test]
fn characterwise_selection_copies_unicode_grapheme_clusters() {
    let mut harness = Harness::new(Document::parse("a e\u{301} 界 z"), 20, 2);
    harness.keys("vly");
    assert_eq!(
        harness.take_effects(),
        vec![Effect::WriteClipboard("a e\u{301}".to_owned())]
    );
}

#[test]
fn yank_preserves_nested_list_indentation() {
    let mut harness = Harness::new(Document::parse("- outer\n  - inner\n"), 24, 3);
    harness.keys("jVy");
    assert_eq!(
        harness.take_effects(),
        vec![Effect::WriteClipboard("  • inner".to_owned())]
    );
}

#[test]
fn yank_copies_image_placeholders_as_rendered_text() {
    let mut harness = Harness::new(Document::parse("![diagram](./pic.png)\n"), 40, 2);
    harness.keys("Vy");
    let yanked = match harness.take_effects().pop() {
        Some(Effect::WriteClipboard(text)) => text,
        other => panic!("expected clipboard effect, got {other:?}"),
    };
    assert!(
        yanked.contains("diagram") && yanked.contains("./pic.png") && !yanked.contains('\u{fffc}'),
        "image placeholder is plain rendered text: {yanked:?}"
    );
}
