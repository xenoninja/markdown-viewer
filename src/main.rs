use std::env;
use std::ffi::OsString;
use std::io::{self, IsTerminal};
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mdview: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let input = one_input(env::args_os(), io::stdin().is_terminal())?;
    match input {
        Input::Path(path) => {
            let path = std::path::PathBuf::from(path);
            let document = mdview::load_document(&path)?;
            mdview::run_file_backed_reading_session(document, path)?;
        }
        Input::StandardInput => {
            let document = mdview::load_standard_input()?;
            mdview::run_reading_session(document)?;
        }
    }
    Ok(())
}

enum Input {
    Path(OsString),
    StandardInput,
}

fn one_input(
    arguments: impl IntoIterator<Item = OsString>,
    standard_input_is_terminal: bool,
) -> Result<Input, &'static str> {
    let mut arguments = arguments.into_iter();
    let _executable = arguments.next();
    let input = match arguments.next() {
        Some(input) if input == "-" => Input::StandardInput,
        Some(path) => Input::Path(path),
        None if !standard_input_is_terminal => Input::StandardInput,
        None => return Err("usage: mdview [<document-path> | -]"),
    };
    if arguments.next().is_some() {
        return Err("expected at most one Document input");
    }
    Ok(input)
}
