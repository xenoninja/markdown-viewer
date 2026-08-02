#![cfg(unix)]

mod support;

use std::fs::{self, File};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::libc;
use nix::pty::{Winsize, openpty};
use nix::sys::termios::{LocalFlags, tcgetattr};
use nix::unistd::dup;
use support::{contains, contains_rendered_text, read_available};
use tempfile::tempdir;

#[test]
fn reading_session_enters_and_restores_the_terminal() {
    assert_session_restores(ExitAction::Keys(b"q"), false);
}

#[test]
fn control_c_interrupt_restores_the_terminal() {
    assert_session_restores(ExitAction::Keys(b"\x03"), false);
}

#[test]
fn no_color_disables_terminal_color_enhancement() {
    let color = assert_session_restores(ExitAction::Keys(b"q"), false);
    let monochrome = assert_session_restores(ExitAction::Keys(b"q"), true);

    assert!(contains_color_sgr(&color), "color output: {color:?}");
    assert!(
        !contains_color_sgr(&monochrome),
        "NO_COLOR output: {monochrome:?}"
    );
}

#[test]
fn terminal_resize_event_recovers_the_normal_frame() {
    let output = assert_session_restores(ExitAction::ResizeThenQuit, false);

    assert!(
        contains_rendered_text(&output, b"Terminal too small"),
        "initial size warning: {output:?}"
    );
    assert!(
        contains_rendered_text(&output, b"PTY") && contains_rendered_text(&output, b"paragraph"),
        "resized Document frame: {output:?}"
    );
}

#[derive(Clone, Copy)]
enum ExitAction {
    Keys(&'static [u8]),
    ResizeThenQuit,
}

fn assert_session_restores(action: ExitAction, no_color: bool) -> Vec<u8> {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("document");
    fs::write(&path, "PTY paragraph").expect("write fixture");

    let size = Winsize {
        ws_row: if matches!(action, ExitAction::ResizeThenQuit) {
            9
        } else {
            12
        },
        ws_col: if matches!(action, ExitAction::ResizeThenQuit) {
            39
        } else {
            40
        },
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let pty = openpty(&size, None).expect("open PTY");
    let terminal_before = tcgetattr(&pty.slave).expect("read initial terminal mode");

    let mut command = Command::new(env!("CARGO_BIN_EXE_mdview"));
    command.arg(&path).env("TERM", "xterm-256color");
    if no_color {
        command.env("NO_COLOR", "1");
    } else {
        command.env_remove("NO_COLOR");
    }
    let mut child = command
        .stdin(Stdio::from(dup(&pty.slave).expect("duplicate PTY input")))
        .stdout(Stdio::from(dup(&pty.slave).expect("duplicate PTY output")))
        .stderr(Stdio::from(dup(&pty.slave).expect("duplicate PTY error")))
        .spawn()
        .expect("start mdview");

    let mut master = File::from(pty.master);
    let flags =
        OFlag::from_bits_truncate(fcntl(&master, FcntlArg::F_GETFL).expect("read PTY flags"));
    fcntl(&master, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK)).expect("make PTY non-blocking");

    let stage_timeout = Duration::from_secs(5);
    let mut deadline = Instant::now() + stage_timeout;
    let mut output = Vec::new();
    let mut action_finished = false;
    let mut resized = false;
    let status = loop {
        read_available(&mut master, &mut output);

        match action {
            ExitAction::Keys(keys) if !action_finished && contains(&output, b"\x1b[?1049h") => {
                master.write_all(keys).expect("send exit input");
                action_finished = true;
                deadline = Instant::now() + stage_timeout;
            }
            ExitAction::ResizeThenQuit
                if !resized && contains_rendered_text(&output, b"Terminal too small") =>
            {
                let normal_size = Winsize {
                    ws_row: 12,
                    ws_col: 60,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                };
                // SAFETY: master is a valid PTY descriptor and normal_size is initialized.
                assert_eq!(
                    unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ, &normal_size) },
                    0,
                    "resize PTY"
                );
                // SAFETY: the child process is live and SIGWINCH has its standard meaning.
                assert_eq!(
                    unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGWINCH) },
                    0,
                    "deliver resize event"
                );
                resized = true;
                deadline = Instant::now() + stage_timeout;
            }
            ExitAction::ResizeThenQuit
                if resized && !action_finished && contains_rendered_text(&output, b"paragraph") =>
            {
                master.write_all(b"q").expect("send quit after resize");
                action_finished = true;
                deadline = Instant::now() + stage_timeout;
            }
            _ => {}
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
        !contains(&output, b"\x1b[?1000h")
            && !contains(&output, b"\x1b[?1002h")
            && !contains(&output, b"\x1b[?1003h")
            && !contains(&output, b"\x1b[?1006h"),
        "mouse capture must stay disabled so native drag selection remains available; PTY output: {output:?}"
    );
    assert!(
        contains_rendered_text(&output, b"PTY") && contains_rendered_text(&output, b"paragraph"),
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
    output
}

fn contains_color_sgr(output: &[u8]) -> bool {
    let mut index = 0;
    while index + 2 < output.len() {
        if output[index] != b'\x1b' || output[index + 1] != b'[' {
            index += 1;
            continue;
        }
        let parameters_start = index + 2;
        let Some(end) = output[parameters_start..]
            .iter()
            .position(|byte| (0x40..=0x7e).contains(byte))
            .map(|offset| parameters_start + offset)
        else {
            return false;
        };
        if output[end] == b'm'
            && output[parameters_start..end]
                .split(|byte| *byte == b';')
                .filter_map(|parameter| std::str::from_utf8(parameter).ok()?.parse::<u16>().ok())
                .any(|parameter| {
                    matches!(
                        parameter,
                        30..=38 | 40..=48 | 90..=97 | 100..=107
                    )
                })
        {
            return true;
        }
        index = end + 1;
    }
    false
}
