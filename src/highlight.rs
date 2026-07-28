use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use unicode_segmentation::UnicodeSegmentation;

/// Presentation-only styling for one semantic code grapheme.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HighlightStyle {
    foreground: Option<(u8, u8, u8)>,
    bold: bool,
    italic: bool,
    underlined: bool,
}

impl HighlightStyle {
    #[must_use]
    pub fn foreground(self) -> Option<(u8, u8, u8)> {
        self.foreground
    }

    #[must_use]
    pub fn is_bold(self) -> bool {
        self.bold
    }

    #[must_use]
    pub fn is_italic(self) -> bool {
        self.italic
    }

    #[must_use]
    pub fn is_underlined(self) -> bool {
        self.underlined
    }
}

/// Replaceable highlighting boundary used by the application harness.
pub trait CodeHighlighter: Send + 'static {
    /// Returns `None` for an unknown language and `Err` for a highlighting failure.
    fn highlight(
        &mut self,
        language: &str,
        code: &str,
    ) -> Result<Option<Vec<HighlightStyle>>, String>;
}

#[derive(Debug)]
struct HighlightRequest {
    block: usize,
    language: String,
    code: String,
}

#[derive(Debug)]
struct HighlightResult {
    block: usize,
    styles: Option<Vec<HighlightStyle>>,
}

#[derive(Debug)]
pub(crate) struct HighlightCache {
    requests: Sender<HighlightRequest>,
    results: Receiver<HighlightResult>,
    requested: HashSet<usize>,
    styles: HashMap<usize, Vec<HighlightStyle>>,
    pending: usize,
}

impl HighlightCache {
    pub(crate) fn syntect() -> Self {
        Self::spawn(|| Box::new(SyntectHighlighter::new()))
    }

    pub(crate) fn with_highlighter(highlighter: impl CodeHighlighter) -> Self {
        Self::spawn(|| Box::new(highlighter))
    }

    fn spawn(factory: impl FnOnce() -> Box<dyn CodeHighlighter> + Send + 'static) -> Self {
        let (request_sender, request_receiver) = mpsc::channel::<HighlightRequest>();
        let (result_sender, result_receiver) = mpsc::channel::<HighlightResult>();
        thread::spawn(move || {
            let mut factory = Some(factory);
            let mut highlighter = None;
            while let Ok(request) = request_receiver.recv() {
                if highlighter.is_none() {
                    highlighter = Some(factory.take().expect("highlighter factory is used once")());
                }
                let highlighter = highlighter.as_mut().expect("highlighter is initialized");
                let styles = highlighter
                    .highlight(&request.language, &request.code)
                    .ok()
                    .flatten();
                if result_sender
                    .send(HighlightResult {
                        block: request.block,
                        styles,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        Self {
            requests: request_sender,
            results: result_receiver,
            requested: HashSet::new(),
            styles: HashMap::new(),
            pending: 0,
        }
    }

    pub(crate) fn request(&mut self, block: usize, language: &str, code: &str) {
        if !self.requested.insert(block) {
            return;
        }
        if self
            .requests
            .send(HighlightRequest {
                block,
                language: language.to_owned(),
                code: code.to_owned(),
            })
            .is_ok()
        {
            self.pending += 1;
        }
    }

    pub(crate) fn collect(&mut self) -> bool {
        let mut changed = false;
        while let Ok(result) = self.results.try_recv() {
            self.pending = self.pending.saturating_sub(1);
            if let Some(styles) = result.styles {
                self.styles.insert(result.block, styles);
            }
            changed = true;
        }
        changed
    }

    pub(crate) fn styles(&self) -> &HashMap<usize, Vec<HighlightStyle>> {
        &self.styles
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.pending > 0
    }
}

struct SyntectHighlighter {
    syntaxes: SyntaxSet,
    theme: Theme,
}

impl SyntectHighlighter {
    fn new() -> Self {
        let syntaxes = SyntaxSet::load_defaults_newlines();
        let mut themes = ThemeSet::load_defaults().themes;
        let theme = themes
            .remove("base16-ocean.dark")
            .expect("syntect default theme exists");
        Self { syntaxes, theme }
    }
}

impl CodeHighlighter for SyntectHighlighter {
    fn highlight(
        &mut self,
        language: &str,
        code: &str,
    ) -> Result<Option<Vec<HighlightStyle>>, String> {
        let Some(syntax) = self.syntaxes.find_syntax_by_token(language) else {
            return Ok(None);
        };
        let mut highlighter = HighlightLines::new(syntax, &self.theme);
        let mut byte_styles = Vec::with_capacity(code.len());

        for line in LinesWithEndings::from(code) {
            let ranges = highlighter
                .highlight_line(line, &self.syntaxes)
                .map_err(|error| error.to_string())?;
            for (style, text) in ranges {
                byte_styles.extend(std::iter::repeat_n(style, text.len()));
            }
        }

        if byte_styles.len() != code.len() {
            return Err("highlighted text did not preserve the semantic code".to_owned());
        }

        let styles = code
            .grapheme_indices(true)
            .map(|(byte, _)| {
                let style = byte_styles[byte];
                HighlightStyle {
                    foreground: Some((style.foreground.r, style.foreground.g, style.foreground.b)),
                    bold: style.font_style.contains(FontStyle::BOLD),
                    italic: style.font_style.contains(FontStyle::ITALIC),
                    underlined: style.font_style.contains(FontStyle::UNDERLINE),
                }
            })
            .collect();
        Ok(Some(styles))
    }
}

#[cfg(test)]
mod tests {
    use super::{CodeHighlighter, SyntectHighlighter};

    #[test]
    fn syntect_highlights_rust_without_changing_grapheme_count() {
        let code = "fn main() { println!(\"界\"); }\n";
        let styles = SyntectHighlighter::new()
            .highlight("rust", code)
            .expect("highlighting succeeds")
            .expect("Rust is recognized");

        assert_eq!(
            styles.len(),
            unicode_segmentation::UnicodeSegmentation::graphemes(code, true).count()
        );
    }
}
