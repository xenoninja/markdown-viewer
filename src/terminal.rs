use std::io::{self, IsTerminal, stdin, stdout};
use std::sync::Arc;
use std::time::Duration;

#[cfg(unix)]
use std::fs::OpenOptions;
#[cfg(unix)]
use std::os::fd::AsRawFd;

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::Document;
use crate::app::{Effect, ReadingSession};
use crate::browser::{BrowserLauncher, SystemBrowser};
use crate::clipboard::{ClipboardWriter, SystemClipboard};
use crate::ui;

pub fn run_reading_session(document: Document) -> io::Result<()> {
    run_session(ReadingSession::new(document))
}

pub fn run_file_backed_reading_session(
    document: Document,
    path: std::path::PathBuf,
) -> io::Result<()> {
    run_session(ReadingSession::with_source(document, path))
}

fn run_session(mut session: ReadingSession) -> io::Result<()> {
    if !stdout().is_terminal() {
        return Err(io::Error::other(
            "standard output must be an interactive terminal",
        ));
    }
    connect_controlling_terminal_input()?;

    let _session = TerminalSession::enter()?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut clipboard = SystemClipboard::with_osc52_writer(stdout());
    let mut browser = SystemBrowser::new();
    let initial_area = terminal.size()?;
    session.resize(initial_area.width, initial_area.height);

    while !session.has_quit() {
        let area = terminal.size()?;
        session.prepare_highlighting(area.width, area.height);
        terminal.draw(|frame| ui::render(frame, &session))?;
        let next_event = if session.highlighting_pending() {
            event::poll(Duration::from_millis(16))?
                .then(event::read)
                .transpose()?
        } else {
            Some(event::read()?)
        };
        match next_event {
            Some(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                let area = terminal.size()?;
                session.key(key, area.width, area.height);
                apply_effects(
                    &mut session,
                    &mut clipboard,
                    &mut browser,
                    area.width,
                    area.height,
                );
            }
            Some(Event::Resize(width, height)) => session.resize(width, height),
            _ => {}
        }
    }

    Ok(())
}

fn apply_effects(
    session: &mut ReadingSession,
    clipboard: &mut SystemClipboard,
    browser: &mut SystemBrowser,
    width: u16,
    height: u16,
) {
    for effect in session.drain_effects() {
        match effect {
            Effect::WriteClipboard(text) => {
                let result = clipboard.write_text(&text);
                session.report_clipboard_result(result);
            }
            Effect::OpenBrowser(url) => {
                let result = browser.open_url(&url);
                session.report_browser_result(result);
            }
            Effect::ReloadDocument(path) => {
                crate::reload::apply(session, &path, width, height);
            }
        }
    }
}

#[cfg(unix)]
fn connect_controlling_terminal_input() -> io::Result<()> {
    if stdin().is_terminal() {
        return Ok(());
    }

    let terminal = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("cannot open controlling terminal for keyboard input: {error}"),
            )
        })?;
    // SAFETY: both descriptors are valid and dup2 atomically replaces standard input.
    if unsafe { libc::dup2(terminal.as_raw_fd(), libc::STDIN_FILENO) } == -1 {
        let error = io::Error::last_os_error();
        return Err(io::Error::new(
            error.kind(),
            format!("cannot use controlling terminal for keyboard input: {error}"),
        ));
    }

    Ok(())
}

#[cfg(not(unix))]
fn connect_controlling_terminal_input() -> io::Result<()> {
    Ok(())
}

struct TerminalSession {
    _panic_hook: PanicHookGuard,
}

impl TerminalSession {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(stdout(), EnterAlternateScreen, Hide) {
            restore_terminal();
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self {
            _panic_hook: PanicHookGuard::install(),
        })
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        restore_terminal();
        let _ = disable_raw_mode();
    }
}

fn restore_terminal() {
    let _ = execute!(stdout(), Show);
    let _ = execute!(stdout(), LeaveAlternateScreen);
}

struct PanicHookGuard {
    previous: Option<Arc<PanicHook>>,
}

type PanicHook = dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static;

impl PanicHookGuard {
    fn install() -> Self {
        let previous: Arc<PanicHook> = Arc::from(std::panic::take_hook());
        let panic_previous = Arc::clone(&previous);
        std::panic::set_hook(Box::new(move |panic| {
            restore_terminal();
            let _ = disable_raw_mode();
            panic_previous(panic);
        }));
        Self {
            previous: Some(previous),
        }
    }
}

impl Drop for PanicHookGuard {
    fn drop(&mut self) {
        if !std::thread::panicking()
            && let Some(previous) = self.previous.take()
        {
            std::panic::set_hook(Box::new(move |panic| previous(panic)));
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs::File;
    use std::io::{ErrorKind, Read};
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    use nix::fcntl::{FcntlArg, OFlag, fcntl};
    use nix::pty::{Winsize, openpty};
    use nix::sys::termios::{LocalFlags, tcgetattr};
    use nix::unistd::dup;

    use super::TerminalSession;

    const PROBE: &str = "MDVIEW_TERMINAL_RESTORATION_PROBE";

    #[test]
    fn error_and_panic_paths_restore_the_terminal() {
        match std::env::var(PROBE).ok().as_deref() {
            Some("panic") => {
                let _session = TerminalSession::enter().expect("enter terminal");
                panic!("intentional terminal panic");
            }
            Some("error") => {
                let result = (|| -> std::io::Result<()> {
                    let _session = TerminalSession::enter()?;
                    Err(std::io::Error::other("intentional terminal error"))
                })();
                assert!(result.is_err());
                std::process::exit(23);
            }
            _ => {
                run_probe("error", false);
                run_probe("panic", true);
            }
        }
    }

    fn run_probe(mode: &str, panic_output: bool) {
        let size = Winsize {
            ws_row: 12,
            ws_col: 60,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let pty = openpty(&size, None).expect("open PTY");
        let terminal_before = tcgetattr(&pty.slave).expect("read initial terminal mode");
        let test_name = "terminal::tests::error_and_panic_paths_restore_the_terminal";
        let mut child = Command::new(std::env::current_exe().expect("current test executable"))
            .args(["--exact", test_name, "--nocapture"])
            .env(PROBE, mode)
            .stdin(Stdio::from(dup(&pty.slave).expect("duplicate PTY input")))
            .stdout(Stdio::from(dup(&pty.slave).expect("duplicate PTY output")))
            .stderr(Stdio::from(dup(&pty.slave).expect("duplicate PTY error")))
            .spawn()
            .expect("start terminal restoration probe");

        let mut master = File::from(pty.master);
        let flags =
            OFlag::from_bits_truncate(fcntl(&master, FcntlArg::F_GETFL).expect("read PTY flags"));
        fcntl(&master, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))
            .expect("make PTY non-blocking");

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut output = Vec::new();
        let status = loop {
            read_available(&mut master, &mut output);
            if let Some(status) = child.try_wait().expect("poll restoration probe") {
                read_available(&mut master, &mut output);
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().expect("stop timed-out restoration probe");
                panic!("terminal restoration probe timed out: {output:?}");
            }
            thread::sleep(Duration::from_millis(5));
        };
        let terminal_after = tcgetattr(&pty.slave).expect("read restored terminal mode");

        assert!(!status.success(), "probe should use an error exit");
        assert!(contains(&output, b"\x1b[?1049h"), "alternate screen entry");
        assert!(contains(&output, b"\x1b[?1049l"), "alternate screen exit");
        assert!(contains(&output, b"\x1b[?25l"), "cursor hidden");
        assert!(contains(&output, b"\x1b[?25h"), "cursor restored");
        if panic_output {
            let restored = find(&output, b"\x1b[?1049l").expect("restoration position");
            let panic = find(&output, b"intentional terminal panic").expect("panic output");
            assert!(
                restored < panic,
                "panic hook must restore before printing the panic: {output:?}"
            );
        }
        assert_eq!(terminal_after.input_flags, terminal_before.input_flags);
        assert_eq!(terminal_after.output_flags, terminal_before.output_flags);
        assert_eq!(terminal_after.control_flags, terminal_before.control_flags);
        assert_eq!(
            terminal_after.local_flags - LocalFlags::PENDIN,
            terminal_before.local_flags - LocalFlags::PENDIN
        );
        assert_eq!(terminal_after.control_chars, terminal_before.control_chars);
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        find(haystack, needle).is_some()
    }

    fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

    fn read_available(terminal: &mut File, output: &mut Vec<u8>) {
        loop {
            let mut chunk = [0_u8; 4096];
            match terminal.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => output.extend_from_slice(&chunk[..read]),
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) if error.raw_os_error() == Some(nix::libc::EIO) => break,
                Err(error) => panic!("read PTY output: {error}"),
            }
        }
    }
}
