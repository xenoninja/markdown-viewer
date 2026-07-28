use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use mdview::{CodeHighlighter, Document, Harness, HighlightStyle, SemanticPosition};
use ratatui::style::Color;

#[test]
fn recognized_code_is_highlighted_without_changing_semantic_text() {
    let document = Document::parse("```rust linenos=1\nfn main() { println!(\"界\"); }\n```\n");
    let expected = document.blocks()[0].text().to_owned();
    let mut harness = Harness::new(document.clone(), 40, 2);

    harness.settle_highlighting();

    assert!(harness.frame().contains("fn main()"));
    assert!(harness.frame().contains("println!"));
    assert_eq!(document.blocks()[0].text(), expected);
    assert!(
        harness
            .highlight_at(SemanticPosition {
                block: 0,
                grapheme: 0,
            })
            .and_then(HighlightStyle::foreground)
            .is_some()
    );
}

#[test]
fn unknown_and_missing_languages_remain_readable_plain_code() {
    let document =
        Document::parse("```not-a-real-language\nunknown();\n```\n\n```\nplain();\n```\n");
    let mut harness = Harness::new(document, 32, 3);

    harness.settle_highlighting();

    assert!(harness.frame().contains("unknown();"));
    assert!(harness.frame().contains("plain();"));
    for block in [0, 1] {
        assert_eq!(
            harness.foreground_at(SemanticPosition { block, grapheme: 0 }),
            Some(Color::Reset)
        );
    }
}

#[test]
fn highlighting_failure_falls_back_to_unchanged_plain_code() {
    let document = Document::parse("```rust\nlet answer = 42;\n```\n");
    let mut harness = Harness::with_highlighter(document, 32, 2, FailingHighlighter);

    harness.settle_highlighting();

    assert!(harness.frame().contains("let answer = 42;"));
    assert_eq!(
        harness.foreground_at(SemanticPosition {
            block: 0,
            grapheme: 0,
        }),
        Some(Color::Reset)
    );
}

#[test]
fn highlighting_is_requested_lazily_near_the_viewport_and_cached() {
    let requests = Arc::new(AtomicUsize::new(0));
    let markdown = (0..20)
        .map(|index| format!("```rust\nblock_{index}();\n```\n"))
        .collect::<Vec<_>>()
        .join("\n");
    let document = Document::parse(&markdown);
    let mut harness =
        Harness::with_highlighter(document, 20, 2, CountingHighlighter(Arc::clone(&requests)));

    harness.settle_highlighting();
    let initial_requests = requests.load(Ordering::SeqCst);
    assert!(initial_requests > 0);
    assert!(initial_requests < 20);

    harness.keys("G");
    harness.settle_highlighting();
    let after_scroll = requests.load(Ordering::SeqCst);
    assert!(after_scroll > initial_requests);
    assert!(after_scroll < 20);

    harness.keys("ggG");
    harness.settle_highlighting();
    assert_eq!(
        requests.load(Ordering::SeqCst),
        after_scroll,
        "completed code-block highlights are reused"
    );
}

struct FailingHighlighter;

impl CodeHighlighter for FailingHighlighter {
    fn highlight(
        &mut self,
        _language: &str,
        _code: &str,
    ) -> Result<Option<Vec<HighlightStyle>>, String> {
        Err("fixture failure".to_owned())
    }
}

struct CountingHighlighter(Arc<AtomicUsize>);

impl CodeHighlighter for CountingHighlighter {
    fn highlight(
        &mut self,
        _language: &str,
        _code: &str,
    ) -> Result<Option<Vec<HighlightStyle>>, String> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }
}
