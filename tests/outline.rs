use ratatui::style::Modifier;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mdview::{Document, Harness, PaneFocus, SemanticPosition};

#[test]
fn outline_shows_declared_hierarchy_without_claiming_pre_heading_content() {
    let document = Document::parse(
        "Introduction.\n\n# Parent\n\nParent body.\n\n### Deep child\n\nChild body.\n\n## Sibling\n",
    );
    let harness = Harness::new(document, 60, 10);
    let frame = harness.frame();
    let outline = frame
        .lines()
        .filter_map(|line| line.split_once('│').map(|(outline, _)| outline.trim_end()))
        .collect::<Vec<_>>();

    assert_eq!(outline[0], "Parent");
    assert_eq!(outline[1], "    Deep child");
    assert_eq!(outline[2], "  Sibling");
    assert_eq!(harness.current_section(), None);
    assert_eq!(
        harness.outline_selection(),
        Some(SemanticPosition {
            block: 1,
            grapheme: 0,
        })
    );
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 0,
        })
    );
}

#[test]
fn outline_focus_moves_selection_without_moving_the_document() {
    let document = Document::parse(
        "# Parent\n\nParent body.\n\n### Deep child\n\nChild body.\n\n## Sibling\n",
    );
    let mut harness = Harness::new(document, 60, 8);
    let cursor = harness.cursor();
    let parent = SemanticPosition {
        block: 0,
        grapheme: 0,
    };
    let deep_child = SemanticPosition {
        block: 2,
        grapheme: 0,
    };

    harness.control('w');
    harness.keys("h");
    assert_eq!(harness.focus(), PaneFocus::Outline);

    harness.keys("j");
    assert_eq!(harness.cursor(), cursor);
    assert_eq!(harness.current_section(), Some(parent));
    assert_eq!(harness.outline_selection(), Some(deep_child));
    assert!(
        harness
            .outline_modifier_at(deep_child)
            .is_some_and(|modifier| modifier.contains(Modifier::REVERSED))
    );
    assert!(
        harness
            .outline_modifier_at(parent)
            .is_some_and(|modifier| modifier.contains(Modifier::BOLD))
    );

    harness.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(harness.focus(), PaneFocus::Outline);

    harness.control('w');
    harness.keys("l");
    assert_eq!(harness.focus(), PaneFocus::Document);

    harness.control('w');
    harness.keys("h");
    harness.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(harness.focus(), PaneFocus::Document);
}

#[test]
fn document_navigation_updates_current_section_without_moving_outline_selection() {
    let document = Document::parse(
        "Introduction.\n\n# Parent\n\nParent body.\n\n### Deep child\n\nChild body.\n\n## Sibling\n",
    );
    let mut harness = Harness::new(document, 60, 10);
    let parent = SemanticPosition {
        block: 1,
        grapheme: 0,
    };
    let deep_child = SemanticPosition {
        block: 3,
        grapheme: 0,
    };

    harness.keys("3}");

    assert_eq!(harness.current_section(), Some(deep_child));
    assert_eq!(harness.outline_selection(), Some(parent));
    assert!(
        harness
            .outline_modifier_at(deep_child)
            .is_some_and(|modifier| modifier.contains(Modifier::BOLD))
    );
    assert!(
        harness
            .outline_modifier_at(parent)
            .is_some_and(|modifier| !modifier.contains(Modifier::BOLD))
    );
}

#[test]
fn activating_outline_selection_records_reversible_jump_history() {
    let document =
        Document::parse("# Parent\n\nabcdefgh\n\n## Child\n\nChild body.\n\n### Grandchild\n");
    let mut harness = Harness::new(document, 60, 8);
    harness.keys("}3l");
    let prior_location = Some(SemanticPosition {
        block: 1,
        grapheme: 3,
    });
    let child = Some(SemanticPosition {
        block: 2,
        grapheme: 0,
    });
    assert_eq!(harness.cursor(), prior_location);

    harness.control('w');
    harness.keys("h");
    harness.keys("j");
    harness.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(harness.focus(), PaneFocus::Document);
    assert_eq!(harness.cursor(), child);
    assert_eq!(harness.current_section(), child);
    assert_eq!(harness.outline_selection(), child);

    harness.control('o');
    assert_eq!(harness.cursor(), prior_location);
    harness.control('i');
    assert_eq!(harness.cursor(), child);

    harness.control('o');
    harness.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(
        harness.cursor(),
        child,
        "Tab carries the terminal encoding of Ctrl-i instead of changing pane focus"
    );
}

#[test]
fn document_cursor_reveal_uses_the_width_remaining_beside_the_outline() {
    let document = Document::parse("# Code\n\n```\nabcdefghijklmnop\n```\n");
    let mut harness = Harness::new(document, 24, 4);

    harness.keys("}15l");

    assert!(
        harness.cursor_cell().is_some(),
        "horizontal reveal keeps the Reading Cursor visible in the narrower Document pane"
    );
}

#[test]
fn long_outline_keeps_the_focused_selection_visible() {
    let document = Document::parse(
        "# One\n\n## Two\n\n## Three\n\n## Four\n\n## Five\n\n## Six\n\n## Seven\n",
    );
    let mut harness = Harness::new(document, 60, 4);
    let seven = SemanticPosition {
        block: 6,
        grapheme: 0,
    };

    harness.control('w');
    harness.keys("h6j");

    assert_eq!(harness.outline_selection(), Some(seven));
    assert!(harness.frame().contains("Seven"));
    assert!(
        harness
            .outline_modifier_at(seven)
            .is_some_and(|modifier| modifier.contains(Modifier::REVERSED))
    );

    let one = SemanticPosition {
        block: 0,
        grapheme: 0,
    };
    harness.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(harness.frame().contains("One"));
    assert!(
        harness
            .outline_modifier_at(one)
            .is_some_and(|modifier| modifier.contains(Modifier::BOLD))
    );

    harness.keys("6}");
    assert_eq!(harness.current_section(), Some(seven));
    assert!(harness.frame().contains("Seven"));
    assert!(
        harness
            .outline_modifier_at(seven)
            .is_some_and(|modifier| modifier.contains(Modifier::BOLD))
    );
}

#[test]
fn outline_uses_image_alt_text_in_readable_heading_labels() {
    let document =
        Document::parse("# System ![architecture](diagram.svg) *overview*\n\nDetails.\n");
    let harness = Harness::new(document, 90, 4);
    let frame = harness.frame();
    let outline_label = frame
        .lines()
        .next()
        .and_then(|line| line.split_once('│'))
        .map(|(outline, _)| outline.trim_end())
        .expect("Outline divider");

    assert_eq!(outline_label, "System architecture overview");
    assert!(!outline_label.contains('\u{fffc}'));
}
