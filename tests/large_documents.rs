use std::sync::mpsc;

use mdview::{CodeHighlighter, HighlightStyle};
use mdview::{Document, DocumentWarning, Harness};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;

#[test]
fn documents_above_the_v1_line_scale_warn_without_rejecting_content() {
    let markdown = (0..=100_000)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");

    let document = Document::parse(&markdown);

    assert_eq!(
        document.warnings(),
        &[DocumentWarning::LargeDocument],
        "Documents above the v1 design scale remain available with a warning"
    );
    assert!(
        document.blocks()[0].text().contains("line 100000"),
        "the complete oversized Document is still semantically parsed"
    );
}

#[test]
fn ordinary_frames_reuse_block_layout_work() {
    let markdown = (0..200)
        .map(|section| {
            format!(
                "{} Section {section}\n\n\
                 Prose for section {section} with a searchable needle.\n\n\
                 ```rust\nlet section_{section} = {section};\n```\n\n\
                 | name | value |\n| --- | ---: |\n| section | {section} |\n",
                "#".repeat(section % 6 + 1)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let document = Document::parse(&markdown);
    let mut harness = Harness::new(document, 80, 12);

    harness.resize(72, 10);
    harness.resize(80, 12);
    harness.reset_layout_metrics();
    harness.keys("20j/needle");
    harness.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    harness.keys("nnNN");
    harness.control('w');
    harness.keys("hjj");
    harness.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    harness.control('o');

    assert_eq!(
        harness.layout_metrics().blocks_laid_out(),
        0,
        "an ordinary frame must not rebuild blocks outside the viewport"
    );
    assert!(
        harness.cursor_cell().is_some(),
        "rapid navigation, reflow, Outline and search jumps preserve cursor mappings"
    );
}

#[test]
fn horizontal_navigation_relayouts_only_the_changed_block() {
    let markdown = format!(
        "```text\n{}\n```\n\n{}",
        "wide-code-".repeat(30),
        (0..200)
            .map(|section| format!("# Section {section}\n\nbody {section}"))
            .collect::<Vec<_>>()
            .join("\n\n")
    );
    let document = Document::parse(&markdown);
    let mut harness = Harness::new(document, 40, 8);

    harness.reset_layout_metrics();
    harness.keys("$");

    assert_eq!(
        harness.layout_metrics().blocks_laid_out(),
        1,
        "changing one block's horizontal viewport must reuse every other block"
    );
    assert_eq!(
        harness.layout_metrics().documents_assembled(),
        0,
        "changing one block's horizontal viewport must patch the cached Rendered Document"
    );
    assert!(
        harness.cursor_cell().is_some(),
        "the logical Reading Cursor remains mapped after horizontal scrolling"
    );
}

#[test]
fn highlight_completion_does_not_reassemble_the_rendered_document() {
    let markdown = format!(
        "```rust\nvisible();\n```\n\n{}",
        (0..500)
            .map(|section| format!("# Section {section}\n\nbody {section}"))
            .collect::<Vec<_>>()
            .join("\n\n")
    );
    let (started_sender, started_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::channel();
    let highlighter = BlockingHighlighter {
        started: started_sender,
        release: release_receiver,
    };
    let mut harness = Harness::with_highlighter(Document::parse(&markdown), 80, 12, highlighter);
    started_receiver
        .recv()
        .expect("visible highlighting request starts");

    harness.reset_layout_metrics();
    release_sender.send(()).expect("release highlighting");
    harness.settle_highlighting();

    assert_eq!(harness.layout_metrics().blocks_laid_out(), 0);
    assert_eq!(harness.layout_metrics().documents_assembled(), 0);
}

#[test]
fn a_hundred_thousand_line_document_remains_navigable_and_searchable() {
    let mut markdown = String::from("```text\n");
    for _ in 0..99_997 {
        markdown.push_str("x\n");
    }
    markdown.push_str("final needle\n```\n");
    assert_eq!(markdown.lines().count(), 100_000);
    let document = Document::parse(&markdown);
    let mut harness = Harness::new(document, 40, 6);

    harness.keys("/needle");
    harness.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(harness.frame().contains("final needle"));
    assert!(harness.cursor_cell().is_some());
    harness.keys("G");
    assert!(harness.cursor_cell().is_some());
}

struct BlockingHighlighter {
    started: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}

impl CodeHighlighter for BlockingHighlighter {
    fn highlight(
        &mut self,
        _language: &str,
        code: &str,
    ) -> Result<Option<Vec<HighlightStyle>>, String> {
        self.started.send(()).expect("report highlighting start");
        self.release.recv().expect("wait for highlighting release");
        Ok(Some(vec![
            HighlightStyle::default();
            code.graphemes(true).count()
        ]))
    }
}
