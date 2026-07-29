use std::fs;
use std::process::Command;

use tempfile::tempdir;

#[test]
fn help_documents_the_complete_command_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_mdview"))
        .arg("--help")
        .output()
        .expect("run mdview --help");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let help = String::from_utf8(output.stdout).expect("help is UTF-8");
    for required in [
        "Usage: mdview [OPTIONS] [<DOCUMENT> | -]",
        "one Markdown Document",
        "piped standard input",
        "-h, --help",
        "-V, --version",
        "--",
        "Exit status:",
        "0",
        "1",
    ] {
        assert!(
            help.contains(required),
            "help omitted {required:?}:\n{help}"
        );
    }
}

#[test]
fn version_matches_the_v1_package_metadata() {
    let output = Command::new(env!("CARGO_BIN_EXE_mdview"))
        .arg("--version")
        .output()
        .expect("run mdview --version");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "mdview 1.0.0\n");
    assert_eq!(env!("CARGO_PKG_VERSION"), "1.0.0");
}

#[test]
fn unknown_options_are_rejected_before_terminal_entry() {
    let output = Command::new(env!("CARGO_BIN_EXE_mdview"))
        .arg("--unknown")
        .output()
        .expect("run mdview with an unknown option");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unknown option: --unknown"),
        "{:?}",
        output.stderr
    );
    assert!(!output.stderr.contains(&b'\x1b'));
}

#[test]
fn option_terminator_allows_a_hyphen_prefixed_document_path() {
    let directory = tempdir().expect("temporary directory");
    fs::write(directory.path().join("--version"), "# Local Document")
        .expect("write hyphen-prefixed Document");

    let output = Command::new(env!("CARGO_BIN_EXE_mdview"))
        .args(["--", "--version"])
        .current_dir(directory.path())
        .output()
        .expect("run mdview with an option terminator");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("standard output must be an interactive terminal"),
        "{:?}",
        output.stderr
    );
}
