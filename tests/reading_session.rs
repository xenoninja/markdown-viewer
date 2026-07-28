use std::fs;

use mdview::{Command, Harness};
use tempfile::tempdir;

#[test]
fn opens_renders_and_quits_an_extensionless_document() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("README");
    fs::write(&path, "A *small* Markdown paragraph.").expect("write fixture");

    let mut harness = Harness::open(&path, 32, 4).expect("open Reading Session");

    assert!(harness.frame().contains("A small Markdown paragraph."));
    assert!(!harness.frame().contains('*'));
    assert!(!harness.has_quit());

    harness.command(Command::Quit);

    assert!(harness.has_quit());
}

#[test]
fn renders_document_control_sequences_as_visible_text() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("untrusted");
    fs::write(
        &path,
        "before \x1b[2J after \x1b]0;owned\x07 and \u{009b}31mred",
    )
    .expect("write fixture");

    let harness = Harness::open(&path, 48, 4).expect("open Reading Session");
    let frame = harness.frame();

    assert!(frame.contains("before ␛[2J after ␛]0;owned␇"));
    assert!(frame.contains(r"and \u{009B}31mred"));
    assert!(!frame.contains('\x1b'));
    assert!(!frame.contains('\x07'));
    assert!(!frame.contains('\u{009b}'));
}

#[test]
fn wraps_paragraphs_at_the_application_viewport() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("narrow");
    fs::write(&path, "one two three four").expect("write fixture");

    let harness = Harness::open(&path, 9, 4).expect("open Reading Session");

    assert_eq!(harness.frame(), "one two\nthree\nfour\n");
}

#[test]
fn displays_raw_html_literally() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("html");
    fs::write(&path, "<aside>literal</aside>").expect("write fixture");

    let harness = Harness::open(&path, 32, 4).expect("open Reading Session");

    assert!(harness.frame().contains("<aside>literal</aside>"));
}

#[test]
fn reads_common_github_markdown_as_a_rendered_document() {
    let document = mdview::Document::parse(
        "## Title\n\nPlain *emphasis*, **strong**, and ~~old~~.\n\n> Use `cargo test` and [read more](https://example.com).\n\n- item\n  - [x] done\n\n---\n",
    );
    let harness = Harness::new(document, 48, 12);
    let frame = harness.frame();

    assert!(frame.contains("Title"));
    assert!(frame.contains("Plain emphasis, strong, and old."));
    assert!(frame.contains("│ Use cargo test and read more."));
    assert!(frame.contains("• item"));
    assert!(frame.contains("  ☑ done"));
    assert!(frame.contains("────────"));
    assert!(!frame.contains("https://example.com"));
    assert!(!frame.contains("##"));
    assert!(!frame.contains("**"));
    assert!(!frame.contains("~~"));
}
