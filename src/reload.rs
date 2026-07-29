use std::path::Path;

use crate::app::ReadingSession;
use crate::source::reload_document;

pub(crate) fn apply(session: &mut ReadingSession, path: &Path, width: u16, height: u16) {
    session.report_reload_result(reload_document(path), width, height);
}
