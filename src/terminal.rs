use std::io::{self, IsTerminal, stdin, stdout};

#[cfg(unix)]
use std::fs::OpenOptions;
#[cfg(unix)]
use std::os::fd::AsRawFd;

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::Document;
use crate::app::{Command, ReadingSession};
use crate::ui;

pub fn run_reading_session(document: Document) -> io::Result<()> {
    if !stdout().is_terminal() {
        return Err(io::Error::other(
            "standard output must be an interactive terminal",
        ));
    }
    connect_controlling_terminal_input()?;

    let _session = TerminalSession::enter()?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut session = ReadingSession::new(document);

    while !session.has_quit() {
        terminal.draw(|frame| ui::render(frame, &session))?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && key.code == KeyCode::Char('q')
        {
            session.command(Command::Quit);
        }
    }

    Ok(())
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

struct TerminalSession;

impl TerminalSession {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(stdout(), EnterAlternateScreen, Hide) {
            restore_terminal();
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self)
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
