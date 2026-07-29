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
    let action = command_line_action(env::args_os(), io::stdin().is_terminal())?;
    match action {
        CommandLineAction::Help => print!("{HELP}"),
        CommandLineAction::Version => println!("mdview {}", env!("CARGO_PKG_VERSION")),
        CommandLineAction::Path(path) => {
            let path = std::path::PathBuf::from(path);
            let document = mdview::load_document(&path)?;
            mdview::run_file_backed_reading_session(document, path)?;
        }
        CommandLineAction::StandardInput => {
            let document = mdview::load_standard_input()?;
            mdview::run_reading_session(document)?;
        }
    }
    Ok(())
}

enum CommandLineAction {
    Help,
    Version,
    Path(OsString),
    StandardInput,
}

fn command_line_action(
    arguments: impl IntoIterator<Item = OsString>,
    standard_input_is_terminal: bool,
) -> Result<CommandLineAction, String> {
    let mut arguments = arguments.into_iter();
    let _executable = arguments.next();
    let action = match arguments.next() {
        Some(option) if option == "-h" || option == "--help" => CommandLineAction::Help,
        Some(option) if option == "-V" || option == "--version" => CommandLineAction::Version,
        Some(separator) if separator == "--" => match arguments.next() {
            Some(document) if document == "-" => CommandLineAction::StandardInput,
            Some(path) => CommandLineAction::Path(path),
            None if !standard_input_is_terminal => CommandLineAction::StandardInput,
            None => return Err(USAGE_ERROR.to_owned()),
        },
        Some(document) if document == "-" => CommandLineAction::StandardInput,
        Some(option) if option.to_string_lossy().starts_with('-') => {
            return Err(format!(
                "unknown option: {}",
                inert_argument(option.to_string_lossy().as_ref())
            ));
        }
        Some(path) => CommandLineAction::Path(path),
        None if !standard_input_is_terminal => CommandLineAction::StandardInput,
        None => return Err(USAGE_ERROR.to_owned()),
    };
    if arguments.next().is_some() {
        return Err("expected at most one Document input".to_owned());
    }
    Ok(action)
}

fn inert_argument(argument: &str) -> String {
    argument
        .chars()
        .map(|character| {
            if character.is_control() {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect()
}

const HELP: &str = "\
mdview — read one Markdown Document in the terminal

Usage: mdview [OPTIONS] [<DOCUMENT> | -]

Arguments:
  <DOCUMENT>  Read one Markdown Document from a local path
  -           Read the Document from standard input
              With no argument, piped standard input is used automatically

Options:
  -h, --help     Print help
  -V, --version  Print version
  --             Stop option processing

Exit status:
  0  Help, version, or a normally ended Reading Session
  1  Invalid usage, input failure, terminal failure, or Reading Session error
";

const USAGE_ERROR: &str = "usage: mdview [<document-path> | -]";
