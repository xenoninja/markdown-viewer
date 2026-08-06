use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mdview::{BrowserResult, ColorMode, Document, Harness, PaneFocus, SemanticPosition};
use ratatui::style::{Color, Modifier};

#[test]
fn help_overlay_documents_only_the_fixed_grouped_keymap_and_is_dismissible() {
    let mut harness = Harness::viewer(
        Document::parse("# Guide\n\nRead this Document.\n"),
        "guide.md",
        84,
        24,
        ColorMode::Monochrome,
    );

    harness.keys("?");
    let help = harness.frame();

    for group in [
        "NAVIGATION",
        "SCROLLING",
        "OUTLINE",
        "SEARCH",
        "SELECTION",
        "LINKS",
        "RELOAD",
        "APPLICATION",
    ] {
        assert!(help.contains(group), "missing {group} group:\n{help}");
    }
    for shortcut in [
        "h j k l       Left/down/up/right",
        "gg / G        Document start/end",
        "number+motion Repeat motion",
        "Ctrl-w h/l    Focus outline/doc",
        "n / N         Next/previous match",
        "v / V         Characters/rows",
        "gx            Open link",
        "Ctrl-o/i      Back/forward",
        "r             Reload local file",
        "? / Esc       Close help/cancel",
        "q / Ctrl-c    Quit",
    ] {
        assert!(
            help.contains(shortcut),
            "missing explained shortcut {shortcut}:\n{help}"
        );
    }
    let action_column = |action: &str| {
        let line = help
            .lines()
            .find(|line| line.contains(action))
            .unwrap_or_else(|| panic!("missing action {action}:\n{help}"));
        line.find(action).expect("action is present")
    };
    assert_eq!(
        action_column("Left/down/up/right"),
        action_column("Repeat motion")
    );
    assert_eq!(action_column("Open link"), action_column("Back/forward"));

    for unsupported in ["edit", "register", "macro", "Tab", ":"] {
        assert!(
            !help.to_lowercase().contains(&unsupported.to_lowercase()),
            "advertised unsupported interaction {unsupported}:\n{help}"
        );
    }

    harness.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!harness.frame().contains("KEYBOARD SHORTCUTS"));

    harness.keys("?");
    harness.keys("?");
    assert!(!harness.frame().contains("KEYBOARD SHORTCUTS"));

    harness.resize(40, 10);
    harness.keys("?");
    let compact_help = harness.frame();
    assert_eq!(
        compact_help,
        "\
┌ KEYBOARD SHORTCUTS ──────────────────┐
│NAVIGATION  h/j/k/l w/b 0/^/$ gg/G    │
│            {/} count Ctrl-u/d/f/b/e/y│
│OUTLINE     o Ctrl-w h/l j/k h/l Enter│
│SEARCH      / find; n/N next/prev     │
│SELECTION   v/V start; y copy         │
│LINKS       gx opens; Ctrl-o/i history│
│RELOAD      r reload local file       │
│APPLICATION ?/Esc close q/Ctrl-c quit │
└──────────────────────────────────────┘"
    );
    for group in [
        "NAVIGATION",
        "OUTLINE",
        "SEARCH",
        "SELECTION",
        "LINKS",
        "RELOAD",
        "APPLICATION",
    ] {
        assert!(
            compact_help.contains(group),
            "40×10 help omitted {group}:\n{compact_help}"
        );
    }
    for keys in [
        "Ctrl-u/d/f/b/e/y",
        "Ctrl-w h/l j/k h/l Enter",
        "gx opens; Ctrl-o/i history",
        "?/Esc close q/Ctrl-c quit",
    ] {
        assert!(
            compact_help.contains(keys),
            "40×10 help clipped {keys}:\n{compact_help}"
        );
    }
}

#[test]
fn status_bar_tracks_document_section_progress_focus_link_and_messages() {
    let document = Document::parse(
        "# Start\n\nalpha\n\n# Links\n\n[docs](https://example.com/reference)\n\n# End\n\nomega\n",
    );
    let mut harness = Harness::viewer(document, "guide.md", 96, 14, ColorMode::Monochrome);

    let initial = harness.status_line();
    assert_eq!(initial, "guide.md │ Start │ 0% │ Document");
    assert!(initial.contains("guide.md"), "{initial}");
    assert!(initial.contains("Start"), "{initial}");
    assert!(initial.contains("Document"), "{initial}");
    assert!(initial.contains('%'), "{initial}");
    assert!(!initial.contains("? help"), "{initial}");

    harness.control('w');
    harness.keys("h");
    assert_eq!(harness.focus(), PaneFocus::Outline);
    assert_eq!(harness.status_line(), "guide.md │ Start │ 0% │ Outline");

    harness.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    harness.keys("2}");
    assert!(harness.status_line().contains("Links"));
    harness.keys("}");
    assert!(
        harness
            .status_line()
            .contains("https://example.com/reference")
    );

    harness.keys("v");
    assert!(harness.status_line().contains("Selection"));
    harness.keys("y");
    assert_eq!(
        harness.status_line(),
        "Copied │ → https://example.com/reference │ guide.md │ Links │ 57% │ Document"
    );
    harness.keys("j");
    assert!(
        !harness.status_line().contains("Copied"),
        "transient message should clear after the next interaction"
    );
}

#[test]
fn narrow_status_keeps_every_context_category_visible() {
    let document = Document::parse(
        "# A very long Current Section name\n\n\
         [documentation](https://example.com/a/very/long/contextual/target)\n",
    );
    let mut harness = Harness::viewer(
        document,
        "a-very-long-document-identity.md",
        40,
        10,
        ColorMode::Monochrome,
    );
    harness.keys("}");
    harness.set_browser_result(BrowserResult::Failed(
        "the configured browser launcher is unavailable".to_owned(),
    ));
    harness.keys("gx");

    let status = harness.status_line();
    for marker in ["!", "→", "D:", "S:", "%", "Doc"] {
        assert!(
            status.contains(marker),
            "narrow status omitted {marker}: {status}"
        );
    }
}

#[test]
fn scrollbar_follows_the_reading_cursor_without_accepting_focus() {
    let markdown = (1..=20)
        .map(|number| format!("# Section {number}\n\nbody {number}\n"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut harness = Harness::viewer(
        Document::parse(&markdown),
        "long.md",
        72,
        12,
        ColorMode::Monochrome,
    );

    assert_eq!(harness.scrollbar_thumb_row(), Some(0));
    harness.keys("G");
    assert_eq!(harness.scrollbar_thumb_row(), Some(10));
    assert_eq!(harness.focus(), PaneFocus::Document);
}

#[test]
fn monochrome_and_color_frames_preserve_the_same_semantic_layout() {
    let markdown = "# Heading\n\n> [!WARNING]\n> beware\n\n[link](https://example.com) needle\n";
    let mut monochrome = Harness::viewer(
        Document::parse(markdown),
        "semantic.md",
        72,
        12,
        ColorMode::Monochrome,
    );
    let mut color = Harness::viewer(
        Document::parse(markdown),
        "semantic.md",
        72,
        12,
        ColorMode::Color,
    );

    monochrome.keys("/needle");
    monochrome.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    color.keys("/needle");
    color.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(monochrome.frame(), color.frame());
    assert!(!monochrome.has_color());
    assert!(color.has_color());

    let match_position = SemanticPosition {
        block: 2,
        grapheme: 5,
    };
    assert!(
        monochrome
            .modifier_at(match_position)
            .is_some_and(|modifier| {
                modifier.contains(Modifier::UNDERLINED) && modifier.contains(Modifier::BOLD)
            })
    );
    assert_eq!(monochrome.foreground_at(match_position), Some(Color::Reset));
}

#[test]
fn monochrome_uses_text_and_modifiers_for_interactive_distinctions() {
    let markdown = "# Heading\n\n> [!WARNING]\n> beware\n\n[link](https://example.com) needle\n";
    let mut harness = Harness::viewer(
        Document::parse(markdown),
        "semantic.md",
        72,
        12,
        ColorMode::Monochrome,
    );
    let heading = SemanticPosition {
        block: 0,
        grapheme: 0,
    };
    let link = SemanticPosition {
        block: 2,
        grapheme: 0,
    };

    assert!(harness.frame().contains("WARNING │"));
    assert!(
        harness
            .modifier_at(heading)
            .is_some_and(|modifier| modifier.contains(Modifier::BOLD))
    );
    assert!(
        harness
            .modifier_at(link)
            .is_some_and(|modifier| modifier.contains(Modifier::UNDERLINED))
    );
    assert!(
        harness
            .outline_modifier_at(heading)
            .is_some_and(|modifier| modifier.contains(Modifier::BOLD))
    );

    harness.control('w');
    harness.keys("h");
    assert!(
        harness
            .outline_modifier_at(heading)
            .is_some_and(|modifier| modifier.contains(Modifier::REVERSED))
    );

    let mut selection = Harness::viewer(
        Document::parse("alpha"),
        "selection.md",
        48,
        10,
        ColorMode::Monochrome,
    );
    selection.keys("vl");
    let anchor = SemanticPosition {
        block: 0,
        grapheme: 0,
    };
    let cursor = SemanticPosition {
        block: 0,
        grapheme: 1,
    };
    assert!(selection.modifier_at(anchor).is_some_and(|modifier| {
        modifier.contains(Modifier::REVERSED) && !modifier.contains(Modifier::BOLD)
    }));
    assert!(selection.modifier_at(cursor).is_some_and(|modifier| {
        modifier.contains(Modifier::REVERSED) && modifier.contains(Modifier::BOLD)
    }));

    selection.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    selection.keys("r");
    assert!(
        selection
            .screen_modifier(0, 9)
            .is_some_and(|modifier| modifier.contains(Modifier::UNDERLINED))
    );

    let mut failed_link = Harness::viewer(
        Document::parse("[link](https://example.com)"),
        "error.md",
        48,
        10,
        ColorMode::Monochrome,
    );
    failed_link.set_browser_result(BrowserResult::Failed("unavailable".to_owned()));
    failed_link.keys("gx");
    assert!(
        failed_link
            .screen_modifier(0, 9)
            .is_some_and(|modifier| modifier.contains(Modifier::REVERSED))
    );
}

#[test]
fn selection_and_reading_cursor_use_the_terminal_palette() {
    let mut harness = Harness::viewer(
        Document::parse("alpha"),
        "selection.md",
        48,
        10,
        ColorMode::Color,
    );
    harness.keys("vl");

    let anchor = SemanticPosition {
        block: 0,
        grapheme: 0,
    };
    let cursor = SemanticPosition {
        block: 0,
        grapheme: 1,
    };

    for position in [anchor, cursor] {
        assert_eq!(harness.foreground_at(position), Some(Color::Reset));
        assert_eq!(harness.background_at(position), Some(Color::Reset));
        assert!(
            harness
                .modifier_at(position)
                .is_some_and(|modifier| modifier.contains(Modifier::REVERSED))
        );
    }
}

#[test]
fn selection_highlight_covers_spaces_between_words() {
    let mut harness = Harness::viewer(
        Document::parse("alpha beta"),
        "selection.md",
        48,
        10,
        ColorMode::Color,
    );
    let start = harness.cursor_cell().expect("selection starts on alpha");
    harness.keys("vllllll");

    let space = SemanticPosition {
        block: 0,
        grapheme: 5,
    };
    assert!(harness.selection_contains(space));
    let space_column = u16::try_from(start.column + 5).expect("space column fits");
    let row = u16::try_from(start.row).expect("space row fits");
    assert!(
        harness
            .screen_modifier(space_column, row)
            .is_some_and(|modifier| modifier.contains(Modifier::REVERSED))
    );
}

#[test]
fn terminal_too_small_message_recovers_after_resize() {
    let mut harness = Harness::viewer(
        Document::parse("# Recoverable\n\ncontent\n"),
        "recover.md",
        39,
        9,
        ColorMode::Monochrome,
    );

    let warning = harness.frame();
    assert_eq!(
        warning,
        concat!(
            "\n\n\n",
            "          Terminal too small\n",
            "      Resize to at least 40 × 10\n",
            "            Current: 39 × 9\n",
            "\n\n"
        )
    );
    assert!(warning.contains("Terminal too small"), "{warning}");
    assert!(warning.contains("40 × 10"), "{warning}");
    assert!(!warning.contains("Recoverable"), "{warning}");

    harness.resize(60, 12);
    let recovered = harness.frame();
    assert_eq!(
        recovered,
        concat!(
            "  Recoverable       │Recoverable                           █\n",
            "                    │content                               │\n",
            "                    │                                      │\n",
            "                    │                                      │\n",
            "                    │                                      │\n",
            "                    │                                      │\n",
            "                    │                                      │\n",
            "                    │                                      │\n",
            "                    │                                      │\n",
            "                    │                                      │\n",
            "                    │                                      │\n",
            "recover.md │ Recoverable │ 0% │ Document"
        )
    );
    assert!(recovered.contains("Recoverable"), "{recovered}");
    assert!(!recovered.contains("Terminal too small"), "{recovered}");
}
