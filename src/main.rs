use std::env;
use std::ffi::OsString;
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
    let path = one_path(env::args_os())?;
    let document = mdview::load_document(path)?;
    mdview::run_reading_session(document)?;
    Ok(())
}

fn one_path(arguments: impl IntoIterator<Item = OsString>) -> Result<OsString, &'static str> {
    let mut arguments = arguments.into_iter();
    let _executable = arguments.next();
    let path = arguments.next().ok_or("usage: mdview <document-path>")?;
    if arguments.next().is_some() {
        return Err("expected exactly one document path");
    }
    Ok(path)
}
