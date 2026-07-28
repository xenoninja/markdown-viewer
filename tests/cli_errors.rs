use std::fs;
use std::process::Command;

use tempfile::tempdir;

#[test]
fn unreadable_path_fails_before_emitting_terminal_controls() {
    let output = Command::new(env!("CARGO_BIN_EXE_mdview"))
        .arg("/definitely/missing/mdview-document")
        .output()
        .expect("run mdview");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot read"));
    assert!(!output.stderr.contains(&b'\x1b'));
}

#[test]
fn directory_fails_before_emitting_terminal_controls() {
    let directory = tempdir().expect("temporary directory");
    let output = Command::new(env!("CARGO_BIN_EXE_mdview"))
        .arg(directory.path())
        .output()
        .expect("run mdview");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("path is a directory"));
    assert!(!output.stderr.contains(&b'\x1b'));
}

#[test]
fn redirected_output_is_rejected_before_terminal_entry() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("document");
    fs::write(&path, "ordinary text").expect("write fixture");

    let output = Command::new(env!("CARGO_BIN_EXE_mdview"))
        .arg(path)
        .output()
        .expect("run mdview");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("standard output must be an interactive terminal")
    );
    assert!(!output.stderr.contains(&b'\x1b'));
}

#[cfg(unix)]
#[test]
fn error_paths_escape_terminal_controls_in_filenames() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("bad\x1b]0;owned\x07");
    fs::create_dir(&path).expect("create control-bearing directory");

    let output = Command::new(env!("CARGO_BIN_EXE_mdview"))
        .arg(path)
        .output()
        .expect("run mdview");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("owned"));
    assert!(!output.stderr.contains(&b'\x1b'));
    assert!(!output.stderr.contains(&b'\x07'));
}
