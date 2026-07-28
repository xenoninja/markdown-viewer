use std::fmt;
use std::fs;
use std::io;
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
    let markdown = String::from_utf8_lossy(&bytes);

    Ok(Document::parse(&markdown))
}
