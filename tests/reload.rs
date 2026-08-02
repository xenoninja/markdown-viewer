use std::fs;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use mdview::{CodeHighlighter, Document, Harness, HighlightStyle, SemanticPosition};
#[cfg(unix)]
use nix::sys::stat::Mode;
#[cfg(unix)]
use nix::unistd::mkfifo;
use ratatui::style::Modifier;
use tempfile::tempdir;
use unicode_segmentation::UnicodeSegmentation;

#[test]
fn reload_replaces_a_file_backed_document() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("guide.md");
    fs::write(&path, "# Before\n\nOld content.\n").expect("write initial Document");
    let mut harness = Harness::open(&path, 48, 5).expect("open Reading Session");

    fs::write(&path, "# After\n\nNew content.\n").expect("write replacement Document");
    assert!(
        harness.frame().contains("Before"),
        "external edits are not watched automatically"
    );
    harness.keys("r");

    assert!(harness.frame().contains("After"));
    assert!(harness.frame().contains("New content."));
    assert!(!harness.frame().contains("Before"));
    assert!(!harness.frame().contains("Old content."));
    assert!(harness.frame().contains("Reloaded"));
}

#[test]
fn reload_preserves_the_heading_path_and_semantic_position() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("guide.md");
    fs::write(
        &path,
        "# Intro\n\nOpening.\n\n# Target\n\nabcdefghij\n\n# Later\n",
    )
    .expect("write initial Document");
    let mut harness = Harness::open(&path, 64, 6).expect("open Reading Session");
    harness.keys("3}4l");

    fs::write(
        &path,
        "# Intro\n\nOpening.\n\n# Inserted\n\nNew section.\n\n# Target\n\nabcdefghijklmnopqrst\n\n# Later\n",
    )
    .expect("write replacement Document");
    harness.keys("r");

    assert_eq!(
        harness.current_section(),
        Some(SemanticPosition {
            block: 4,
            grapheme: 0,
        })
    );
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 5,
            grapheme: 11,
        })
    );
}

#[test]
fn reload_keeps_a_parent_section_position_out_of_its_child_section() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("guide.md");
    fs::write(
        &path,
        format!("# Parent\n\n{}\n\n## Child\n\nx\n", "a".repeat(100)),
    )
    .expect("write initial Document");
    let mut harness = Harness::open(&path, 64, 6).expect("open Reading Session");
    harness.keys("}99l");

    fs::write(
        &path,
        format!(
            "# Parent\n\n{}\n\n## Child\n\n{}\n",
            "b".repeat(10),
            "c".repeat(100)
        ),
    )
    .expect("write replacement Document");
    harness.keys("r");

    assert_eq!(
        harness.current_section(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 0,
        })
    );
    assert_eq!(
        harness.cursor().map(|position| position.block),
        Some(1),
        "relative position is scoped to the exact Current Section"
    );
}

#[test]
fn reload_falls_back_when_the_heading_path_is_removed_and_content_is_shorter() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("guide.md");
    fs::write(
        &path,
        "# First\n\nabcdefghij\n\n# Removed\n\n0123456789\n\n# Tail\n\nlast position\n",
    )
    .expect("write initial Document");
    let mut harness = Harness::open(&path, 48, 5).expect("open Reading Session");
    harness.keys("G");

    fs::write(&path, "# Renamed\n\nshort\n").expect("write shorter replacement Document");
    harness.keys("r");

    assert_eq!(
        harness.current_section(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 0,
        })
    );
    assert_eq!(
        harness.cursor(),
        Some(SemanticPosition {
            block: 1,
            grapheme: 4,
        }),
        "global semantic progress falls back to the nearest valid end position"
    );
}

#[test]
fn reload_fallback_lands_only_on_navigable_rendered_content() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("guide.md");
    fs::write(&path, "```\naaaa\nbbbb\n```\n").expect("write initial Document");
    let mut harness = Harness::open(&path, 32, 4).expect("open Reading Session");
    harness.keys("3l");

    fs::write(&path, "```\nx\nyy\n```\n").expect("write replacement Document");
    harness.keys("r");

    assert!(
        harness.cursor_cell().is_some(),
        "Reload cannot restore the Reading Cursor onto a suppressed source newline"
    );
}

#[test]
fn failed_reload_keeps_the_last_valid_document_visible() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("guide.md");
    fs::write(&path, "# Stable\n\nLast valid content.\n").expect("write initial Document");
    let mut harness = Harness::open(&path, 64, 5).expect("open Reading Session");

    fs::remove_file(&path).expect("remove source");
    harness.keys("r");

    assert!(harness.frame().contains("Stable"));
    assert!(harness.frame().contains("Last valid content."));
    assert!(harness.frame().contains("Reload failed:"));
    assert!(!harness.has_quit());
}

#[cfg(unix)]
#[test]
fn reload_rejects_a_source_that_changes_during_a_partial_write() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("guide.md");
    fs::write(&path, "# Stable\n\nLast valid content.\n").expect("write initial Document");
    let mut harness = Harness::open(&path, 64, 5).expect("open Reading Session");

    fs::remove_file(&path).expect("replace source with first FIFO");
    let fifo_mode = Mode::S_IRUSR | Mode::S_IWUSR;
    mkfifo(&path, fifo_mode).expect("create first FIFO");
    let writer_path = path.clone();
    let writer = thread::spawn(move || {
        let completed = "# Complete\n\nFinished replacement.\n";
        fs::write(&writer_path, "# Part").expect("serve partial replacement");

        fs::remove_file(&writer_path).expect("replace source with second FIFO");
        mkfifo(&writer_path, fifo_mode).expect("create second FIFO");
        fs::write(&writer_path, completed).expect("serve completed replacement");

        fs::remove_file(&writer_path).expect("replace FIFO with completed source");
        fs::write(writer_path, completed).expect("write completed source");
    });
    harness.keys("r");
    writer.join().expect("replacement writer");

    assert!(harness.frame().contains("Stable"));
    assert!(harness.frame().contains("Last valid content."));
    assert!(harness.frame().contains("Reload failed:"));

    harness.keys("r");
    assert!(harness.frame().contains("Complete"));
    assert!(harness.frame().contains("Finished replacement."));
}

#[test]
fn reload_explains_that_standard_input_is_immutable() {
    let mut harness = Harness::new(Document::parse("# Piped\n\nImmutable content.\n"), 64, 5);

    harness.keys("r");

    assert!(harness.frame().contains("Piped"));
    assert!(harness.frame().contains("Immutable content."));
    assert!(
        harness
            .frame()
            .contains("Reload unavailable: standard-input Documents cannot be reloaded")
    );
    assert!(harness.take_effects().is_empty());
}

#[test]
fn reload_replaces_invalid_utf8_visibly_and_reports_the_warning() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("guide.md");
    fs::write(&path, "valid content").expect("write initial Document");
    let mut harness = Harness::open(&path, 64, 4).expect("open Reading Session");

    fs::write(&path, b"before \xff after").expect("write invalid UTF-8 replacement");
    harness.keys("r");

    assert!(harness.frame().contains("before � after"));
    assert!(
        harness
            .frame()
            .contains("Reloaded: warning: invalid UTF-8 replaced with �")
    );
}

#[test]
fn reload_clears_revision_derived_state_and_rebuilds_the_outline() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("guide.md");
    fs::write(&path, "# Old\n\nneedle old content\n").expect("write initial Document");
    let mut harness = Harness::open(&path, 64, 5).expect("open Reading Session");
    harness.keys("/needle");
    harness.key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ));
    harness.keys("v2l");
    assert!(harness.selection_mode().is_some());

    fs::write(&path, "# New\n\nneedle replacement\n").expect("write replacement Document");
    harness.keys("r");

    let needle = SemanticPosition {
        block: 1,
        grapheme: 0,
    };
    assert!(harness.frame().contains("New"));
    assert!(!harness.frame().contains("Old"));
    assert_eq!(
        harness.current_section(),
        Some(SemanticPosition {
            block: 0,
            grapheme: 0,
        })
    );
    assert_eq!(harness.selection_mode(), None);
    assert!(
        harness
            .modifier_at(needle)
            .is_some_and(|modifier| !modifier.contains(Modifier::UNDERLINED)),
        "the old search highlight is not applied to the new revision"
    );
    let cursor = harness.cursor();
    harness.keys("n");
    assert_eq!(harness.cursor(), cursor, "old search matches are discarded");
}

#[test]
fn reload_discards_stale_highlighting_and_requests_the_new_revision() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("guide.md");
    fs::write(&path, "```rust\nold();\n```\n").expect("write initial Document");
    let (started_sender, started_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::channel();
    let highlighted = Arc::new(Mutex::new(Vec::new()));
    let highlighter = BlockingHighlighter {
        started: started_sender,
        release: release_receiver,
        highlighted: Arc::clone(&highlighted),
    };
    let mut harness =
        Harness::open_with_highlighter(&path, 40, 3, highlighter).expect("open Reading Session");
    started_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("old highlighting started");

    fs::write(&path, "```rust\nnew();\n```\n").expect("write replacement Document");
    harness.keys("r");
    release_sender.send(()).expect("release old highlighting");
    harness.settle_highlighting();

    assert_eq!(
        *highlighted.lock().expect("highlight log"),
        ["old();\n", "new();\n"],
        "the old result cannot satisfy the new revision's request"
    );
}

struct BlockingHighlighter {
    started: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
    highlighted: Arc<Mutex<Vec<String>>>,
}

impl CodeHighlighter for BlockingHighlighter {
    fn highlight(
        &mut self,
        _language: &str,
        code: &str,
    ) -> Result<Option<Vec<HighlightStyle>>, String> {
        let first = {
            let mut highlighted = self.highlighted.lock().expect("highlight log");
            highlighted.push(code.to_owned());
            highlighted.len() == 1
        };
        if first {
            self.started.send(()).expect("report old highlighting");
            self.release
                .recv()
                .expect("wait to release old highlighting");
        }
        Ok(Some(vec![
            HighlightStyle::default();
            code.graphemes(true).count()
        ]))
    }
}
