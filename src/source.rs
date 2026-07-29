use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{Document, DocumentWarning};

#[derive(Debug)]
pub struct SourceError {
    path: PathBuf,
    kind: SourceErrorKind,
}

#[derive(Debug)]
enum SourceErrorKind {
    Directory,
    Read(io::Error),
    ChangedDuringReload,
}

impl fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            SourceErrorKind::Directory => {
                write!(
                    formatter,
                    "cannot open {:?}: path is a directory",
                    self.path
                )
            }
            SourceErrorKind::Read(error) => {
                write!(formatter, "cannot read {:?}: {error}", self.path)
            }
            SourceErrorKind::ChangedDuringReload => {
                write!(
                    formatter,
                    "cannot Reload {:?}: source changed while it was being read",
                    self.path
                )
            }
        }
    }
}

impl std::error::Error for SourceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            SourceErrorKind::Directory => None,
            SourceErrorKind::Read(error) => Some(error),
            SourceErrorKind::ChangedDuringReload => None,
        }
    }
}

pub fn load_document(path: impl AsRef<Path>) -> Result<Document, SourceError> {
    let path = path.as_ref();
    let metadata = fs::metadata(path).map_err(|error| SourceError {
        path: path.to_owned(),
        kind: SourceErrorKind::Read(error),
    })?;

    if metadata.is_dir() {
        return Err(SourceError {
            path: path.to_owned(),
            kind: SourceErrorKind::Directory,
        });
    }

    let bytes = read_document_bytes(path)?;

    Ok(parse_bytes(&bytes))
}

pub(crate) fn reload_document(path: impl AsRef<Path>) -> Result<Document, SourceError> {
    let path = path.as_ref();
    let first = read_document_bytes(path)?;
    std::thread::sleep(Duration::from_millis(50));
    let second = read_document_bytes(path)?;
    if first != second {
        return Err(SourceError {
            path: path.to_owned(),
            kind: SourceErrorKind::ChangedDuringReload,
        });
    }
    Ok(parse_bytes(&second))
}

pub fn load_standard_input() -> io::Result<Document> {
    let mut bytes = Vec::new();
    io::stdin().read_to_end(&mut bytes).map_err(|error| {
        io::Error::new(error.kind(), format!("cannot read standard input: {error}"))
    })?;

    Ok(parse_bytes(&bytes))
}

fn read_document_bytes(path: &Path) -> Result<Vec<u8>, SourceError> {
    fs::read(path).map_err(|error| SourceError {
        path: path.to_owned(),
        kind: SourceErrorKind::Read(error),
    })
}

fn parse_bytes(bytes: &[u8]) -> Document {
    match std::str::from_utf8(bytes) {
        Ok(markdown) => Document::parse(markdown),
        Err(_) => {
            let mut document = Document::parse(&String::from_utf8_lossy(bytes));
            document.add_warning(DocumentWarning::InvalidUtf8Replaced);
            document
        }
    }
}
