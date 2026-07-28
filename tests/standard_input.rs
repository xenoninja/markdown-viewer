#![cfg(unix)]

mod support;

use std::ffi::CString;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::libc;
use nix::pty::{ForkptyResult, Winsize, forkpty};
use nix::sys::termios::{Termios, tcgetattr};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::{Pid, pipe};
use support::{contains, contains_rendered_text, read_available};

static PTY_TEST: Mutex<()> = Mutex::new(());

#[test]
fn piped_document_uses_the_controlling_terminal_for_interaction() {
    let _serial = PTY_TEST.lock().unwrap_or_else(PoisonError::into_inner);
    let mut process = PipedSession::spawn(&[]);
    process
        .standard_input
        .as_mut()
        .expect("piped standard input")
        .write_all(b"Piped *Markdown* paragraph")
        .expect("write piped Document");
    drop(process.standard_input.take());

    process.wait_for_output(b"paragraph");
    process.terminal.write_all(b"q").expect("send quit");

    let (status, output, terminal_before, terminal_after) = process.finish();

    assert!(exited_successfully(status), "PTY output: {output:?}");
    assert!(
        contains_rendered_text(&output, b"Piped")
            && contains_rendered_text(&output, b"Markdown")
            && contains_rendered_text(&output, b"paragraph"),
        "Document rendered; PTY output: {output:?}"
    );
    assert!(contains(&output, b"\x1b[?1049h"), "alternate screen entry");
    assert!(contains(&output, b"\x1b[?1049l"), "alternate screen exit");
    assert!(contains(&output, b"\x1b[?25h"), "cursor restored");
    assert_eq!(terminal_after, terminal_before, "raw mode restored");
}

#[test]
fn dash_explicitly_selects_a_piped_document() {
    let _serial = PTY_TEST.lock().unwrap_or_else(PoisonError::into_inner);
    let mut process = PipedSession::spawn(&["-"]);
    process
        .standard_input
        .as_mut()
        .expect("piped standard input")
        .write_all(b"Explicit standard input")
        .expect("write piped Document");
    drop(process.standard_input.take());

    process.wait_for_output(b"input");
    process.terminal.write_all(b"q").expect("send quit");

    let (status, output, _, _) = process.finish();

    assert!(exited_successfully(status), "PTY output: {output:?}");
    assert!(
        contains_rendered_text(&output, b"Explicit")
            && contains_rendered_text(&output, b"standard")
            && contains_rendered_text(&output, b"input"),
        "Document rendered; PTY output: {output:?}"
    );
}

#[test]
fn reading_session_opens_only_after_the_complete_stream_arrives() {
    let _serial = PTY_TEST.lock().unwrap_or_else(PoisonError::into_inner);
    let mut process = PipedSession::spawn(&[]);
    process
        .standard_input
        .as_mut()
        .expect("piped standard input")
        .write_all(b"Complete stream")
        .expect("write first part of Document");

    thread::sleep(Duration::from_millis(50));
    process.read_available();
    assert!(
        !contains(&process.output, b"\x1b[?1049h"),
        "full-screen interface stays closed while the stream is incomplete"
    );

    process
        .standard_input
        .as_mut()
        .expect("piped standard input")
        .write_all(b" before opening")
        .expect("write final part of Document");
    drop(process.standard_input.take());
    process.wait_for_output(b"opening");
    process.terminal.write_all(b"q").expect("send quit");

    let (status, output, _, _) = process.finish();

    assert!(exited_successfully(status), "PTY output: {output:?}");
    assert!(
        contains_rendered_text(&output, b"Complete")
            && contains_rendered_text(&output, b"stream")
            && contains_rendered_text(&output, b"before")
            && contains_rendered_text(&output, b"opening"),
        "complete Document rendered; PTY output: {output:?}"
    );
}

struct PipedSession {
    child: Pid,
    standard_input: Option<File>,
    terminal: File,
    terminal_attributes: File,
    terminal_before: Termios,
    output: Vec<u8>,
    deadline: Instant,
}

impl PipedSession {
    fn spawn(arguments: &[&str]) -> Self {
        let size = Winsize {
            ws_row: 12,
            ws_col: 40,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let executable =
            CString::new(env!("CARGO_BIN_EXE_mdview")).expect("executable has no null bytes");
        let mut argument_storage = vec![executable.clone()];
        argument_storage.extend(
            arguments
                .iter()
                .map(|argument| CString::new(*argument).expect("argument has no null bytes")),
        );
        let mut argument_pointers = argument_storage
            .iter()
            .map(|argument| argument.as_ptr())
            .collect::<Vec<_>>();
        argument_pointers.push(std::ptr::null());
        let (document_input, document_output) = pipe().expect("open Document pipe");

        // SAFETY: the child calls only async-signal-safe libc functions before execv.
        let fork = unsafe { forkpty(&size, None) }.expect("fork PTY child");
        let (child, master) = match fork {
            ForkptyResult::Parent { child, master } => (child, master),
            ForkptyResult::Child => {
                if unsafe { libc::dup2(document_input.as_raw_fd(), libc::STDIN_FILENO) } == -1 {
                    unsafe { libc::_exit(126) };
                }
                unsafe {
                    libc::close(document_input.as_raw_fd());
                    libc::close(document_output.as_raw_fd());
                    libc::execv(executable.as_ptr(), argument_pointers.as_ptr());
                    libc::_exit(127);
                }
            }
        };
        drop(document_input);

        let terminal_before = tcgetattr(&master).expect("read initial terminal mode");
        let terminal_attributes = File::from(master.try_clone().expect("duplicate PTY attributes"));
        let terminal = File::from(master);
        let flags =
            OFlag::from_bits_truncate(fcntl(&terminal, FcntlArg::F_GETFL).expect("read PTY flags"));
        fcntl(&terminal, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))
            .expect("make PTY non-blocking");

        Self {
            child,
            standard_input: Some(File::from(document_output)),
            terminal,
            terminal_attributes,
            terminal_before,
            output: Vec::new(),
            deadline: Instant::now() + Duration::from_secs(5),
        }
    }

    fn wait_for_output(&mut self, expected: &[u8]) {
        while !contains(&self.output, expected) {
            self.read_available();
            if let Some(status) = self.poll_child() {
                panic!(
                    "mdview exited before rendering with {status:?}; PTY output: {:?}",
                    self.output
                );
            }
            assert!(
                Instant::now() < self.deadline,
                "mdview did not render; PTY output: {:?}",
                self.output
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn finish(mut self) -> (WaitStatus, Vec<u8>, Termios, Termios) {
        let status = loop {
            self.read_available();
            if let Some(status) = self.poll_child() {
                self.read_available();
                break status;
            }
            if Instant::now() >= self.deadline {
                unsafe {
                    libc::kill(self.child.as_raw(), libc::SIGKILL);
                }
                let _ = waitpid(self.child, None);
                panic!("mdview did not exit; PTY output: {:?}", self.output);
            }
            thread::sleep(Duration::from_millis(5));
        };
        let terminal_after =
            tcgetattr(&self.terminal_attributes).expect("read restored terminal mode");
        (status, self.output, self.terminal_before, terminal_after)
    }

    fn poll_child(&self) -> Option<WaitStatus> {
        match waitpid(self.child, Some(WaitPidFlag::WNOHANG)).expect("poll mdview") {
            WaitStatus::StillAlive => None,
            status => Some(status),
        }
    }

    fn read_available(&mut self) {
        read_available(&mut self.terminal, &mut self.output);
    }
}

fn exited_successfully(status: WaitStatus) -> bool {
    matches!(status, WaitStatus::Exited(_, 0))
}
