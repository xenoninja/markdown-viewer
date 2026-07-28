#![cfg(unix)]

mod support;

use std::fs::{self, File};
use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::pty::{Winsize, openpty};
use nix::sys::termios::{LocalFlags, tcgetattr};
use nix::unistd::dup;
use support::{contains, read_available};
use tempfile::tempdir;

#[test]
fn reading_session_enters_and_restores_the_terminal() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("document");
    fs::write(&path, "PTY paragraph").expect("write fixture");

    let size = Winsize {
        ws_row: 12,
        ws_col: 40,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let pty = openpty(&size, None).expect("open PTY");
    let terminal_before = tcgetattr(&pty.slave).expect("read initial terminal mode");

    let mut child = Command::new(env!("CARGO_BIN_EXE_mdview"))
        .arg(&path)
        .stdin(Stdio::from(dup(&pty.slave).expect("duplicate PTY input")))
        .stdout(Stdio::from(dup(&pty.slave).expect("duplicate PTY output")))
        .stderr(Stdio::from(dup(&pty.slave).expect("duplicate PTY error")))
        .spawn()
        .expect("start mdview");

    let mut master = File::from(pty.master);
    let flags =
        OFlag::from_bits_truncate(fcntl(&master, FcntlArg::F_GETFL).expect("read PTY flags"));
    fcntl(&master, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK)).expect("make PTY non-blocking");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut output = Vec::new();
    let mut sent_quit = false;
    let status = loop {
        read_available(&mut master, &mut output);

        if !sent_quit && contains(&output, b"\x1b[?1049h") {
            master.write_all(b"q").expect("send quit");
            sent_quit = true;
        }

        if let Some(status) = child.try_wait().expect("poll mdview") {
            read_available(&mut master, &mut output);
            break status;
        }

        if Instant::now() >= deadline {
            child.kill().expect("stop timed-out mdview");
            panic!("mdview did not exit; PTY output: {output:?}");
        }
        thread::sleep(Duration::from_millis(5));
    };

    let terminal_after = tcgetattr(&pty.slave).expect("read restored terminal mode");

    assert!(status.success(), "PTY output: {output:?}");
    assert!(contains(&output, b"\x1b[?1049h"), "alternate screen entry");
    assert!(contains(&output, b"\x1b[?1049l"), "alternate screen exit");
    assert!(contains(&output, b"\x1b[?25l"), "cursor hidden");
    assert!(contains(&output, b"\x1b[?25h"), "cursor restored");
    assert!(
        contains(&output, b"PTY") && contains(&output, b"paragraph"),
        "Document rendered; PTY output: {output:?}"
    );
    assert_eq!(
        terminal_after.input_flags, terminal_before.input_flags,
        "input mode restored"
    );
    assert_eq!(
        terminal_after.output_flags, terminal_before.output_flags,
        "output mode restored"
    );
    assert_eq!(
        terminal_after.control_flags, terminal_before.control_flags,
        "control mode restored"
    );
    assert_eq!(
        terminal_after.local_flags - LocalFlags::PENDIN,
        terminal_before.local_flags - LocalFlags::PENDIN,
        "local mode restored"
    );
    assert_eq!(
        terminal_after.control_chars, terminal_before.control_chars,
        "control characters restored"
    );
}
