use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::Document;

#[derive(Debug)]
pub struct SourceError {
    path: PathBuf,
    kind: SourceErrorKind,
}

#[derive(Debug)]
enum SourceErrorKind {
    Directory,
    Read(io::Error),
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
        }
    }
}

impl std::error::Error for SourceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            SourceErrorKind::Directory => None,
            SourceErrorKind::Read(error) => Some(error),
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

    let bytes = fs::read(path).map_err(|error| SourceError {
        path: path.to_owned(),
        kind: SourceErrorKind::Read(error),
    })?;

    Ok(parse_bytes(&bytes))
}

pub fn load_standard_input() -> io::Result<Document> {
    let mut bytes = Vec::new();
    io::stdin().read_to_end(&mut bytes).map_err(|error| {
        io::Error::new(error.kind(), format!("cannot read standard input: {error}"))
    })?;

    Ok(parse_bytes(&bytes))
}

fn parse_bytes(bytes: &[u8]) -> Document {
    Document::parse(&String::from_utf8_lossy(bytes))
}
