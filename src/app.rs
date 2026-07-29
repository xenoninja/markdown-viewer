use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use unicode_segmentation::UnicodeSegmentation;

use crate::browser::{BrowserLauncher, BrowserResult, FakeBrowser};
use crate::clipboard::{ClipboardResult, ClipboardWriter, FakeClipboard};
use crate::copy::{self, SelectionMode};
use crate::highlight::{CodeHighlighter, HighlightCache};
use crate::layout::{
    CellLocation, RenderedDocument, SemanticPosition, layout, layout_with_state, logical_positions,
};
use crate::search::{SearchMatch, find_matches};
use crate::source::{SourceError, load_document};
use crate::ui;
use crate::{BlockKind, Document, HeadingLevel};

const MIN_OUTLINE_WIDTH: u16 = 16;
const MAX_OUTLINE_WIDTH: u16 = 40;
const MIN_DOCUMENT_PANE_WIDTH: u16 = 32;
const PANE_DIVIDER_WIDTH: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    Quit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaneFocus {
    Document,
    Outline,
}

/// Explicit side-effect requests observed by adapters and tests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    WriteClipboard(String),
    OpenBrowser(String),
    ReloadDocument(PathBuf),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Selection {
    mode: SelectionMode,
    anchor: SemanticPosition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OutlineEntry {
    pub(crate) position: SemanticPosition,
    pub(crate) level: HeadingLevel,
    pub(crate) label: String,
    collapsed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutlineBranchState {
    Leaf,
    Collapsed,
    Expanded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PaneLayout {
    pub(crate) outline_width: u16,
    pub(crate) document_x: u16,
    pub(crate) document_width: u16,
}

#[derive(Debug, Default)]
struct JumpHistory {
    back: Vec<SemanticPosition>,
    forward: Vec<SemanticPosition>,
}

impl JumpHistory {
    fn record(&mut self, prior_location: SemanticPosition) {
        self.back.push(prior_location);
        self.forward.clear();
    }

    fn traverse(
        &mut self,
        current_location: SemanticPosition,
        direction: JumpDirection,
    ) -> Option<SemanticPosition> {
        let (source, destination) = match direction {
            JumpDirection::Backward => (&mut self.back, &mut self.forward),
            JumpDirection::Forward => (&mut self.forward, &mut self.back),
        };
        let target = source.pop()?;
        destination.push(current_location);
        Some(target)
    }
}

#[derive(Clone, Copy)]
enum JumpDirection {
    Backward,
    Forward,
}

#[derive(Debug)]
struct SearchPrompt {
    query: String,
    prior_focus: PaneFocus,
    prior_viewport: usize,
    prior_outline_viewport: usize,
    prior_message: Option<String>,
}

#[derive(Debug)]
struct ReloadContext {
    heading_path: Option<Vec<(HeadingLevel, String)>>,
    heading_path_occurrence: usize,
    section_anchor: RelativeAnchor,
    document_anchor: RelativeAnchor,
}

#[derive(Clone, Copy, Debug, Default)]
struct RelativeAnchor {
    ordinal: usize,
    extent: usize,
}

#[derive(Debug)]
pub struct ReadingSession {
    source: Option<PathBuf>,
    document: Document,
    cursor: Option<SemanticPosition>,
    outline: Vec<OutlineEntry>,
    outline_enabled: bool,
    outline_selection: Option<usize>,
    outline_viewport: usize,
    focus: PaneFocus,
    jump_history: JumpHistory,
    fragment_targets: BTreeMap<String, SemanticPosition>,
    viewport: usize,
    horizontal_offsets: Vec<usize>,
    preferred_column: Option<usize>,
    pending_count: Option<usize>,
    pending_g: bool,
    pending_control_w: bool,
    search_prompt: Option<SearchPrompt>,
    search_matches: Vec<SearchMatch>,
    search_highlights: BTreeSet<SemanticPosition>,
    search_leading_highlights: BTreeSet<usize>,
    search_query: Option<String>,
    search_message: Option<String>,
    selection: Option<Selection>,
    status_message: Option<String>,
    effects: Vec<Effect>,
    quit: bool,
    highlighting: HighlightCache,
}

impl ReadingSession {
    #[must_use]
    pub fn new(document: Document) -> Self {
        Self::with_highlight_cache(document, None, HighlightCache::syntect())
    }

    pub(crate) fn with_source(document: Document, source: PathBuf) -> Self {
        Self::with_highlight_cache(document, Some(source), HighlightCache::syntect())
    }

    fn with_highlight_cache(
        document: Document,
        source: Option<PathBuf>,
        highlighting: HighlightCache,
    ) -> Self {
        let cursor = layout(&document, 100).first_position();
        let outline = outline_entries(&document);
        let outline_selection = (!outline.is_empty()).then_some(0);
        let fragment_targets = fragment_targets(&document);
        let block_count = document.blocks().len();
        Self {
            source,
            document,
            cursor,
            outline,
            outline_enabled: true,
            outline_selection,
            outline_viewport: 0,
            focus: PaneFocus::Document,
            jump_history: JumpHistory::default(),
            fragment_targets,
            viewport: 0,
            horizontal_offsets: vec![0; block_count],
            preferred_column: None,
            pending_count: None,
            pending_g: false,
            pending_control_w: false,
            search_prompt: None,
            search_matches: Vec::new(),
            search_highlights: BTreeSet::new(),
            search_leading_highlights: BTreeSet::new(),
            search_query: None,
            search_message: None,
            selection: None,
            status_message: None,
            effects: Vec::new(),
            quit: false,
            highlighting,
        }
    }

    pub fn command(&mut self, command: Command) {
        match command {
            Command::Quit => self.quit = true,
        }
    }

    pub fn key(&mut self, key: KeyEvent, width: u16, screen_height: u16) {
        if self.search_prompt.is_some() {
            self.search_prompt_key(key, width, screen_height);
            return;
        }

        let height = self.content_height(screen_height);
        if key.code == KeyCode::Esc {
            if self.selection.take().is_some() {
                self.clear_pending();
                return;
            }
            self.focus = PaneFocus::Document;
            self.ensure_outline_context_visible(self.content_height(screen_height));
            self.clear_pending();
            return;
        }

        if key.code == KeyCode::Tab {
            if self.focus == PaneFocus::Document {
                self.traverse_jump_history(JumpDirection::Forward, width, height);
            }
            self.clear_pending();
            return;
        }

        if key.code == KeyCode::Enter {
            if self.focus == PaneFocus::Outline {
                self.activate_outline_selection(width, screen_height);
            }
            self.clear_pending();
            return;
        }

        if self.pending_control_w {
            self.pending_control_w = false;
            match key.code {
                KeyCode::Char('h') if self.outline_is_visible(width) => {
                    self.focus = PaneFocus::Outline;
                }
                KeyCode::Char('l') => self.focus = PaneFocus::Document,
                _ => {}
            }
            self.ensure_outline_context_visible(self.content_height(screen_height));
            self.clear_pending();
            return;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL)
            && let KeyCode::Char(character) = key.code
        {
            if character == 'w' {
                self.pending_control_w = true;
                self.pending_count = None;
                self.pending_g = false;
                return;
            }
            self.control(character, width, height);
            return;
        }

        let KeyCode::Char(character) = key.code else {
            self.clear_pending();
            return;
        };

        if character == '/' {
            self.search_prompt = Some(SearchPrompt {
                query: String::new(),
                prior_focus: self.focus,
                prior_viewport: self.viewport,
                prior_outline_viewport: self.outline_viewport,
                prior_message: self.search_message.take(),
            });
            self.clear_pending();
            let rendered = self.rendered(width);
            self.ensure_cursor_visible(&rendered, self.content_height(screen_height));
            return;
        }

        if character.is_ascii_digit() {
            if self.focus == PaneFocus::Document && character == '0' && self.pending_count.is_none()
            {
                self.motion(width, height, Motion::RowStart, 1);
            } else if character != '0' || self.pending_count.is_some() {
                let digit = character.to_digit(10).expect("ASCII digit") as usize;
                self.pending_count = Some(
                    self.pending_count
                        .unwrap_or_default()
                        .saturating_mul(10)
                        .saturating_add(digit),
                );
            }
            return;
        }

        if self.focus == PaneFocus::Document && self.pending_g {
            self.pending_g = false;
            if character == 'g' {
                let count = self.pending_count.take();
                self.document_row(width, height, count, false);
                return;
            }
            if character == 'x' {
                self.pending_count = None;
                self.activate_under_cursor(width, height);
                return;
            }
        }

        if self.focus == PaneFocus::Document && character == 'g' {
            self.pending_g = true;
            return;
        }

        let supplied_count = self.pending_count.take();
        let count = supplied_count.unwrap_or(1).max(1);
        if character == 'o' {
            self.toggle_outline(width, screen_height);
            return;
        }
        if character == 'r' {
            self.request_reload();
            return;
        }
        if self.focus == PaneFocus::Outline {
            match character {
                'q' => self.command(Command::Quit),
                'j' => self.move_outline_selection(count, true, height),
                'k' => self.move_outline_selection(count, false, height),
                'h' => self.set_outline_branch_collapsed(true, height),
                'l' => self.set_outline_branch_collapsed(false, height),
                _ => self.clear_pending(),
            }
            return;
        }

        match character {
            'q' => self.command(Command::Quit),
            'v' if self.focus == PaneFocus::Document => {
                self.begin_selection(SelectionMode::Characterwise);
            }
            'V' if self.focus == PaneFocus::Document => {
                self.begin_selection(SelectionMode::Row);
            }
            'y' if self.focus == PaneFocus::Document && self.selection.is_some() => {
                self.yank_selection(width);
            }
            'h' => self.motion(width, height, Motion::Left, count),
            'j' => self.motion(width, height, Motion::Down, count),
            'k' => self.motion(width, height, Motion::Up, count),
            'l' => self.motion(width, height, Motion::Right, count),
            'w' => self.motion(width, height, Motion::WordForward, count),
            'b' => self.motion(width, height, Motion::WordBackward, count),
            '^' => self.motion(width, height, Motion::FirstNonBlank, count),
            '$' => self.motion(width, height, Motion::RowEnd, count),
            'G' => self.document_row(width, height, supplied_count, true),
            '{' => self.motion(width, height, Motion::ParagraphBackward, count),
            '}' => self.motion(width, height, Motion::ParagraphForward, count),
            'n' => self.navigate_search(true, width, height),
            'N' => self.navigate_search(false, width, height),
            _ => self.clear_pending(),
        }
    }

    fn request_reload(&mut self) {
        if let Some(path) = &self.source {
            self.effects.push(Effect::ReloadDocument(path.clone()));
            self.status_message = None;
        } else {
            self.status_message =
                Some("Reload unavailable: standard-input Documents cannot be reloaded".to_owned());
        }
        self.clear_pending();
    }

    pub fn report_reload_result(
        &mut self,
        result: Result<Document, SourceError>,
        width: u16,
        screen_height: u16,
    ) {
        match result {
            Ok(document) => {
                let context = ReloadContext::capture(self);
                self.document = document;
                self.outline = outline_entries(&self.document);
                self.cursor = context.position_in(&self.document, &self.outline);
                self.outline_selection = self.cursor.and_then(|cursor| {
                    self.outline
                        .iter()
                        .rposition(|entry| entry.position.block <= cursor.block)
                });
                if self.outline_selection.is_none() && !self.outline.is_empty() {
                    self.outline_selection = Some(0);
                }
                self.outline_viewport = 0;
                self.fragment_targets = fragment_targets(&self.document);
                self.viewport = 0;
                self.horizontal_offsets = vec![0; self.document.blocks().len()];
                self.preferred_column = None;
                self.jump_history = JumpHistory::default();
                self.search_prompt = None;
                self.search_matches.clear();
                self.search_highlights.clear();
                self.search_leading_highlights.clear();
                self.search_query = None;
                self.search_message = None;
                self.selection = None;
                self.highlighting.reset();
                self.status_message = Some(self.status_warning().map_or_else(
                    || "Reloaded".to_owned(),
                    |warning| format!("Reloaded: {warning}"),
                ));
                self.resize(width, screen_height);
            }
            Err(error) => {
                self.status_message = Some(format!("Reload failed: {error}"));
            }
        }
    }

    fn begin_selection(&mut self, mode: SelectionMode) {
        let Some(anchor) = self.cursor else {
            return;
        };
        self.selection = Some(Selection { mode, anchor });
        self.status_message = None;
        self.clear_pending();
    }

    fn yank_selection(&mut self, width: u16) {
        let Some(selection) = self.selection.take() else {
            return;
        };
        let Some(cursor) = self.cursor else {
            return;
        };
        let rendered = self.rendered(width);
        let text = copy::selected_text(
            &self.document,
            &rendered,
            selection.mode,
            selection.anchor,
            cursor,
        );
        self.effects.push(Effect::WriteClipboard(text));
        self.clear_pending();
    }

    pub fn report_clipboard_result(&mut self, result: ClipboardResult) {
        self.status_message = Some(match result {
            ClipboardResult::Copied(_) => "Copied".to_owned(),
            ClipboardResult::Failed(message) => {
                if message.is_empty() {
                    "Copy failed".to_owned()
                } else {
                    format!("Copy failed: {message}")
                }
            }
        });
    }

    pub fn report_browser_result(&mut self, result: BrowserResult) {
        match result {
            BrowserResult::Opened => {
                self.status_message = None;
            }
            BrowserResult::Failed(message) => {
                self.status_message = Some(if message.is_empty() {
                    "Browser failed".to_owned()
                } else {
                    format!("Browser failed: {message}")
                });
            }
        }
    }

    pub fn drain_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    #[must_use]
    pub fn selection_mode(&self) -> Option<SelectionMode> {
        self.selection.map(|selection| selection.mode)
    }

    #[must_use]
    pub fn selection_anchor(&self) -> Option<SemanticPosition> {
        self.selection.map(|selection| selection.anchor)
    }

    #[must_use]
    pub fn selection_contains(&self, position: SemanticPosition, width: u16) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };
        let Some(cursor) = self.cursor else {
            return false;
        };
        let rendered = self.rendered(width);
        selection_contains(selection, cursor, position, &rendered)
    }

    #[must_use]
    pub fn is_selected(&self, position: SemanticPosition, width: u16) -> bool {
        self.selection_contains(position, width)
    }

    #[must_use]
    pub fn has_quit(&self) -> bool {
        self.quit
    }

    #[must_use]
    pub fn document(&self) -> &Document {
        &self.document
    }

    #[must_use]
    pub fn cursor(&self) -> Option<SemanticPosition> {
        self.cursor
    }

    #[must_use]
    pub fn current_section(&self) -> Option<SemanticPosition> {
        let cursor = self.cursor?;
        self.outline
            .iter()
            .rev()
            .find(|entry| entry.position.block <= cursor.block)
            .map(|entry| entry.position)
    }

    #[must_use]
    pub fn outline_selection(&self) -> Option<SemanticPosition> {
        self.outline_selection
            .and_then(|selection| self.outline.get(selection))
            .map(|entry| entry.position)
    }

    #[must_use]
    pub fn focus(&self) -> PaneFocus {
        self.focus
    }

    #[must_use]
    pub fn viewport(&self) -> usize {
        self.viewport
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        if !self.outline_is_visible(width) {
            self.focus = PaneFocus::Document;
        }
        let height = self.content_height(height);
        self.ensure_outline_context_visible(height);
        if let Some(cursor) = self.cursor {
            self.ensure_horizontal_cursor_visible(width, cursor);
        }
        let rendered = self.rendered(width);
        self.ensure_cursor_visible(&rendered, height);
    }

    pub(crate) fn rendered(&self, width: u16) -> RenderedDocument {
        let width = self.pane_layout(width).document_width;
        layout_with_state(
            &self.document,
            width,
            &self.horizontal_offsets,
            self.highlighting.styles(),
        )
    }

    pub(crate) fn prepare_highlighting(&mut self, width: u16, height: u16) {
        let height = self.content_height(height);
        self.highlighting.collect();
        let rendered = self.rendered(width);
        let margin = usize::from(height.max(1));
        let first_row = self.viewport.saturating_sub(margin);
        let last_row = self
            .viewport
            .saturating_add(margin.saturating_mul(2))
            .min(rendered.rows().len());
        let mut blocks = rendered.rows()[first_row.min(last_row)..last_row]
            .iter()
            .map(crate::RenderedRow::block)
            .collect::<Vec<_>>();
        blocks.sort_unstable();
        blocks.dedup();

        for block_index in blocks {
            let block = &self.document.blocks()[block_index];
            if block.kind() == crate::BlockKind::Code
                && let Some(language) = block.language()
            {
                self.highlighting
                    .request(block_index, language, block.text());
            }
        }
    }

    pub(crate) fn highlighting_pending(&self) -> bool {
        self.highlighting.is_pending()
    }

    pub(crate) fn status_warning(&self) -> Option<&'static str> {
        self.document
            .warnings()
            .first()
            .map(|warning| warning.message())
    }

    pub(crate) fn status_text(&self) -> Option<String> {
        if let Some(prompt) = &self.search_prompt {
            Some(format!("/{}", prompt.query))
        } else if let Some(message) = &self.status_message {
            Some(message.clone())
        } else if let Some(message) = &self.search_message {
            Some(message.clone())
        } else if self.focus == PaneFocus::Outline {
            self.outline_selection
                .and_then(|selection| self.outline.get(selection))
                .map(|entry| entry.label.clone())
        } else if let Some(target) = self.link_target_under_cursor() {
            Some(target)
        } else {
            self.status_warning().map(str::to_owned)
        }
    }

    fn link_target_under_cursor(&self) -> Option<String> {
        let cursor = self.cursor?;
        link_target_at_position(&self.document, cursor)
    }

    pub(crate) fn content_height(&self, screen_height: u16) -> u16 {
        screen_height.saturating_sub(u16::from(self.status_text().is_some()))
    }

    pub(crate) fn outline(&self) -> &[OutlineEntry] {
        &self.outline
    }

    pub(crate) fn visible_outline_indices(&self) -> Vec<usize> {
        let current_path = self.current_section_path();
        let mut collapsed_ancestors = Vec::new();
        let mut visible = Vec::with_capacity(self.outline.len());

        for (index, entry) in self.outline.iter().enumerate() {
            let depth = entry.level.depth();
            while collapsed_ancestors
                .last()
                .is_some_and(|ancestor_depth| *ancestor_depth >= depth)
            {
                collapsed_ancestors.pop();
            }
            if collapsed_ancestors.is_empty() || current_path[index] {
                visible.push(index);
            }
            if entry.collapsed {
                collapsed_ancestors.push(depth);
            }
        }

        visible
    }

    pub(crate) fn outline_viewport(&self) -> usize {
        self.outline_viewport
    }

    pub(crate) fn is_search_match(&self, position: SemanticPosition) -> bool {
        self.search_highlights.contains(&position)
    }

    pub(crate) fn search_leading_query(&self, block: usize) -> Option<&str> {
        self.search_leading_highlights
            .contains(&block)
            .then_some(self.search_query.as_deref())
            .flatten()
    }

    pub(crate) fn pane_layout(&self, screen_width: u16) -> PaneLayout {
        if !self.outline_is_visible(screen_width) {
            return PaneLayout {
                outline_width: 0,
                document_x: 0,
                document_width: screen_width,
            };
        }

        let outline_width = (screen_width / 3).clamp(MIN_OUTLINE_WIDTH, MAX_OUTLINE_WIDTH);
        let document_x = outline_width
            .saturating_add(PANE_DIVIDER_WIDTH)
            .min(screen_width);
        PaneLayout {
            outline_width,
            document_x,
            document_width: screen_width.saturating_sub(document_x).max(1),
        }
    }

    fn outline_is_visible(&self, screen_width: u16) -> bool {
        !self.outline.is_empty()
            && self.outline_enabled
            && screen_width
                >= MIN_OUTLINE_WIDTH
                    .saturating_add(PANE_DIVIDER_WIDTH)
                    .saturating_add(MIN_DOCUMENT_PANE_WIDTH)
    }

    fn toggle_outline(&mut self, width: u16, screen_height: u16) {
        self.outline_enabled = !self.outline_enabled;
        if !self.outline_is_visible(width) {
            self.focus = PaneFocus::Document;
        }
        let height = self.content_height(screen_height);
        if let Some(cursor) = self.cursor {
            self.ensure_horizontal_cursor_visible(width, cursor);
        }
        let rendered = self.rendered(width);
        self.ensure_cursor_visible(&rendered, height);
        self.ensure_outline_context_visible(height);
    }

    fn control(&mut self, character: char, width: u16, height: u16) {
        if self.focus == PaneFocus::Outline {
            self.clear_pending();
            return;
        }
        let count = self.pending_count.take().unwrap_or(1).max(1);
        self.pending_g = false;
        let page = usize::from(height.max(1));
        let half_page_distance = count.saturating_mul((page / 2).max(1));
        let page_distance = count.saturating_mul(page);
        let scroll_distance = isize::try_from(count).unwrap_or(isize::MAX);
        match character {
            'u' => self.motion(width, height, Motion::Up, half_page_distance),
            'd' => self.motion(width, height, Motion::Down, half_page_distance),
            'b' => self.motion(width, height, Motion::Up, page_distance),
            'f' => self.motion(width, height, Motion::Down, page_distance),
            'e' => self.scroll(&self.rendered(width), height, scroll_distance),
            'y' => self.scroll(&self.rendered(width), height, -scroll_distance),
            'o' => self.traverse_jump_history(JumpDirection::Backward, width, height),
            'i' => self.traverse_jump_history(JumpDirection::Forward, width, height),
            _ => {}
        }
    }

    fn activate_outline_selection(&mut self, width: u16, screen_height: u16) {
        let Some(target) = self.outline_selection() else {
            return;
        };
        if let Some(cursor) = self.cursor {
            self.jump_history.record(cursor);
        }
        self.focus = PaneFocus::Document;
        let height = self.content_height(screen_height);
        self.move_cursor_to(target, width, height);
    }

    fn activate_under_cursor(&mut self, width: u16, height: u16) {
        let Some(cursor) = self.cursor else {
            return;
        };
        let Some(target) = link_target_at_position(&self.document, cursor) else {
            return;
        };
        self.activate_link_target(&target, width, height);
    }

    fn activate_link_target(&mut self, target: &str, width: u16, height: u16) {
        self.status_message = None;
        if let Some(fragment) = target.strip_prefix('#') {
            let key = fragment.to_lowercase();
            let Some(destination) = self.fragment_targets.get(&key).copied() else {
                self.status_message = Some(format!("Target not found: #{fragment}"));
                return;
            };
            if let Some(cursor) = self.cursor {
                self.jump_history.record(cursor);
            }
            self.move_cursor_to(destination, width, height);
            return;
        }

        if is_web_url(target) {
            self.effects.push(Effect::OpenBrowser(target.to_owned()));
            return;
        }

        self.status_message = Some(if looks_like_relative_path(target) {
            "Relative links cannot be opened".to_owned()
        } else {
            format!("Unsupported link scheme: {target}")
        });
    }

    fn traverse_jump_history(&mut self, direction: JumpDirection, width: u16, height: u16) {
        let Some(cursor) = self.cursor else {
            return;
        };
        let Some(target) = self.jump_history.traverse(cursor, direction) else {
            return;
        };
        self.move_cursor_to(target, width, height);
    }

    fn move_cursor_to(&mut self, target: SemanticPosition, width: u16, height: u16) {
        let had_status = self.status_text().is_some();
        self.cursor = Some(target);
        self.preferred_column = None;
        let height = self.height_after_cursor_change(height, had_status);
        self.ensure_horizontal_cursor_visible(width, target);
        let rendered = self.rendered(width);
        self.ensure_cursor_visible(&rendered, height);
        self.ensure_outline_context_visible(height);
    }

    fn height_after_cursor_change(&self, height: u16, had_status: bool) -> u16 {
        match (had_status, self.status_text().is_some()) {
            (false, true) => height.saturating_sub(1),
            (true, false) => height.saturating_add(1),
            _ => height,
        }
    }

    fn motion(&mut self, width: u16, height: u16, motion: Motion, count: usize) {
        let had_status = self.status_text().is_some();
        let rendered = self.rendered(width);
        let Some(cursor) = self.cursor else {
            return;
        };
        let Some(location) = rendered.cell_for_position(cursor) else {
            self.cursor = rendered.first_position();
            let height = self.height_after_cursor_change(height, had_status);
            self.ensure_cursor_visible(&rendered, height);
            self.ensure_outline_context_visible(height);
            return;
        };

        let target = match motion {
            Motion::Left | Motion::Right => {
                self.preferred_column = None;
                horizontal_target(&rendered, location, motion, count)
            }
            Motion::Up | Motion::Down => {
                let column = *self.preferred_column.get_or_insert(location.column);
                vertical_target(&rendered, location, column, motion, count)
            }
            Motion::WordForward | Motion::WordBackward => {
                self.preferred_column = None;
                word_target(&self.document, cursor, motion, count)
            }
            Motion::RowStart | Motion::FirstNonBlank => {
                self.preferred_column = None;
                let row = location
                    .row
                    .saturating_add(count.saturating_sub(1))
                    .min(rendered.rows().len().saturating_sub(1));
                rendered.rows()[row]
                    .cells()
                    .iter()
                    .find(|cell| cell.is_navigable())
                    .map(|cell| cell.position())
            }
            Motion::RowEnd => {
                self.preferred_column = None;
                let row = location
                    .row
                    .saturating_add(count.saturating_sub(1))
                    .min(rendered.rows().len().saturating_sub(1));
                rendered.rows()[row]
                    .cells()
                    .iter()
                    .rev()
                    .find(|cell| cell.is_navigable())
                    .map(|cell| cell.position())
            }
            Motion::ParagraphBackward | Motion::ParagraphForward => {
                self.preferred_column = None;
                paragraph_target(&rendered, cursor, motion, count)
            }
        };

        if let Some(target) = target {
            self.cursor = Some(target);
            self.ensure_horizontal_cursor_visible(width, target);
        }
        let height = self.height_after_cursor_change(height, had_status);
        let rendered = self.rendered(width);
        self.ensure_cursor_visible(&rendered, height);
        self.ensure_outline_context_visible(height);
    }

    fn document_row(&mut self, width: u16, height: u16, count: Option<usize>, end: bool) {
        let had_status = self.status_text().is_some();
        let rendered = self.rendered(width);
        self.preferred_column = None;
        self.cursor = match count {
            Some(count) if count > 0 && (end || count > 1) => rendered
                .rows()
                .get(count.saturating_sub(1))
                .and_then(|row| row.cells().first())
                .filter(|cell| cell.is_navigable())
                .map(|cell| cell.position())
                .or_else(|| rendered.last_position()),
            _ if end => rendered.last_position(),
            _ => rendered.first_position(),
        };
        let height = self.height_after_cursor_change(height, had_status);
        if let Some(cursor) = self.cursor {
            self.ensure_horizontal_cursor_visible(width, cursor);
        }
        let rendered = self.rendered(width);
        self.ensure_cursor_visible(&rendered, height);
        self.ensure_outline_context_visible(height);
    }

    fn scroll(&mut self, rendered: &RenderedDocument, height: u16, amount: isize) {
        let maximum = rendered
            .rows()
            .len()
            .saturating_sub(usize::from(height.max(1)));
        self.viewport = self.viewport.saturating_add_signed(amount).min(maximum);
    }

    fn ensure_cursor_visible(&mut self, rendered: &RenderedDocument, height: u16) {
        let height = usize::from(height.max(1));
        let maximum = rendered.rows().len().saturating_sub(height);
        self.viewport = self.viewport.min(maximum);
        let Some(row) = self
            .cursor
            .and_then(|cursor| rendered.row_for_position(cursor))
        else {
            return;
        };
        if row < self.viewport {
            self.viewport = row;
        } else if row >= self.viewport + height {
            self.viewport = row + 1 - height;
        }
    }

    fn ensure_horizontal_cursor_visible(&mut self, width: u16, cursor: SemanticPosition) {
        if !matches!(
            self.document.blocks()[cursor.block].kind(),
            crate::BlockKind::Code | crate::BlockKind::Table
        ) {
            return;
        }

        let width = self.pane_layout(width).document_width;
        let rendered = layout(&self.document, width);
        let Some(location) = rendered.cell_for_position(cursor) else {
            return;
        };
        let row = &rendered.rows()[location.row];
        let available_width = usize::from(width.max(1))
            .saturating_sub(row.column())
            .max(1);
        let cell_start = location.column.saturating_sub(row.column());
        let cell_end = cell_start + location.width;
        let offset = &mut self.horizontal_offsets[cursor.block];

        if cell_start < *offset {
            *offset = cell_start;
        } else if cell_end > offset.saturating_add(available_width) {
            let minimum_offset = cell_end.saturating_sub(available_width);
            let mut boundary = 0;
            for cell in row.cells() {
                if boundary >= minimum_offset {
                    break;
                }
                boundary += cell.width();
            }
            *offset = boundary.min(cell_start);
        }
    }

    fn clear_pending(&mut self) {
        self.pending_count = None;
        self.pending_g = false;
        self.pending_control_w = false;
    }

    fn search_prompt_key(&mut self, key: KeyEvent, width: u16, screen_height: u16) {
        match key.code {
            KeyCode::Esc => {
                let prompt = self.search_prompt.take().expect("Search Prompt is active");
                self.focus = if prompt.prior_focus == PaneFocus::Outline
                    && !self.outline_is_visible(width)
                {
                    PaneFocus::Document
                } else {
                    prompt.prior_focus
                };
                self.viewport = prompt.prior_viewport;
                self.outline_viewport = prompt.prior_outline_viewport;
                self.search_message = prompt.prior_message;
            }
            KeyCode::Backspace => {
                if let Some(prompt) = &mut self.search_prompt {
                    prompt.query.pop();
                }
            }
            KeyCode::Enter => {
                self.confirm_search(width, screen_height);
                return;
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                if let Some(prompt) = &mut self.search_prompt {
                    prompt.query.push(character);
                }
            }
            _ => {}
        }
        let rendered = self.rendered(width);
        self.ensure_cursor_visible(&rendered, self.content_height(screen_height));
        self.ensure_outline_context_visible(self.content_height(screen_height));
    }

    fn confirm_search(&mut self, width: u16, screen_height: u16) {
        let prompt = self.search_prompt.take().expect("Search Prompt is active");
        if prompt.query.is_empty() {
            self.focus = prompt.prior_focus;
            self.viewport = prompt.prior_viewport;
            self.outline_viewport = prompt.prior_outline_viewport;
            self.search_message = prompt.prior_message;
            return;
        }

        self.focus = PaneFocus::Document;
        self.search_matches = find_matches(&self.document, &prompt.query);
        self.search_query = Some(prompt.query.clone());
        self.search_highlights = self
            .search_matches
            .iter()
            .flat_map(|search_match| search_match.positions.iter().copied())
            .collect();
        self.search_leading_highlights = self
            .search_matches
            .iter()
            .flat_map(|search_match| search_match.leading_blocks.iter().copied())
            .collect();
        if self.search_matches.is_empty() {
            self.search_message = Some(format!("Pattern not found: {}", prompt.query));
            let rendered = self.rendered(width);
            self.ensure_cursor_visible(&rendered, self.content_height(screen_height));
            return;
        }

        self.search_message = None;
        self.navigate_search(true, width, self.content_height(screen_height));
    }

    fn navigate_search(&mut self, forward: bool, width: u16, height: u16) {
        let Some(cursor) = self.cursor else {
            return;
        };
        let target = if forward {
            self.search_matches
                .iter()
                .find(|search_match| search_match.start > cursor)
                .or_else(|| self.search_matches.first())
        } else {
            self.search_matches
                .iter()
                .rev()
                .find(|search_match| search_match.start < cursor)
                .or_else(|| self.search_matches.last())
        }
        .map(|search_match| search_match.start);
        if let Some(target) = target {
            self.move_cursor_to(target, width, height);
        }
    }

    fn move_outline_selection(&mut self, count: usize, forward: bool, height: u16) {
        let Some(selection) = self.outline_selection else {
            return;
        };
        let visible = self.visible_outline_indices();
        let Some(row) = visible.iter().position(|index| *index == selection) else {
            return;
        };
        let target = if forward {
            row.saturating_add(count)
                .min(visible.len().saturating_sub(1))
        } else {
            row.saturating_sub(count)
        };
        self.outline_selection = visible.get(target).copied();
        self.ensure_outline_context_visible(height);
    }

    fn set_outline_branch_collapsed(&mut self, collapsed: bool, height: u16) {
        let Some(selection) = self.outline_selection else {
            return;
        };
        if self.outline_is_branch(selection) {
            self.outline[selection].collapsed = collapsed;
        }
        self.ensure_outline_context_visible(height);
    }

    fn outline_is_branch(&self, index: usize) -> bool {
        self.outline
            .get(index + 1)
            .is_some_and(|next| next.level.depth() > self.outline[index].level.depth())
    }

    pub(crate) fn outline_branch_state(&self, index: usize) -> OutlineBranchState {
        if !self.outline_is_branch(index) {
            OutlineBranchState::Leaf
        } else if self.outline[index].collapsed {
            OutlineBranchState::Collapsed
        } else {
            OutlineBranchState::Expanded
        }
    }

    fn current_section_path(&self) -> Vec<bool> {
        let mut path = vec![false; self.outline.len()];
        let Some(mut index) = self.current_section().and_then(|section| {
            self.outline
                .iter()
                .position(|entry| entry.position == section)
        }) else {
            return path;
        };

        loop {
            path[index] = true;
            let depth = self.outline[index].level.depth();
            let Some(parent) = (0..index)
                .rev()
                .find(|candidate| self.outline[*candidate].level.depth() < depth)
            else {
                break;
            };
            index = parent;
        }
        path
    }

    fn ensure_outline_context_visible(&mut self, height: u16) {
        let visible = self.visible_outline_indices();
        if let Some(selection) = self.outline_selection
            && !visible.contains(&selection)
        {
            self.outline_selection = visible
                .iter()
                .rev()
                .find(|index| **index < selection)
                .copied()
                .or_else(|| visible.first().copied());
        }
        let target = match self.focus {
            PaneFocus::Outline => self.outline_selection,
            PaneFocus::Document => self.current_section().and_then(|section| {
                self.outline
                    .iter()
                    .position(|entry| entry.position == section)
            }),
        };
        let Some(target) = target else {
            self.outline_viewport = 0;
            return;
        };
        let Some(target) = visible.iter().position(|index| *index == target) else {
            self.outline_viewport = 0;
            return;
        };
        let height = usize::from(height.max(1));
        let maximum = visible.len().saturating_sub(height);
        self.outline_viewport = self.outline_viewport.min(maximum);
        if target < self.outline_viewport {
            self.outline_viewport = target;
        } else if target >= self.outline_viewport.saturating_add(height) {
            self.outline_viewport = target + 1 - height;
        }
    }
}

impl ReloadContext {
    fn capture(session: &ReadingSession) -> Self {
        let document_positions = logical_positions(&session.document);
        let section_index = session.current_section().and_then(|section| {
            session
                .outline
                .iter()
                .position(|entry| entry.position == section)
        });
        let heading_paths = heading_paths(&session.outline);
        let heading_path = section_index.map(|index| heading_paths[index].clone());
        let heading_path_occurrence = section_index
            .zip(heading_path.as_ref())
            .map(|(section, path)| {
                heading_paths[..section]
                    .iter()
                    .filter(|candidate| *candidate == path)
                    .count()
            })
            .unwrap_or_default();
        let section_positions = section_index
            .map(|index| positions_in_section(&document_positions, &session.outline, index))
            .unwrap_or_default();

        Self {
            heading_path,
            heading_path_occurrence,
            section_anchor: RelativeAnchor::capture(&section_positions, session.cursor),
            document_anchor: RelativeAnchor::capture(&document_positions, session.cursor),
        }
    }

    fn position_in(
        &self,
        document: &Document,
        outline: &[OutlineEntry],
    ) -> Option<SemanticPosition> {
        let document_positions = logical_positions(document);
        if let Some(path) = &self.heading_path {
            let heading_paths = heading_paths(outline);
            let section = heading_paths
                .iter()
                .enumerate()
                .filter(|(_, candidate)| *candidate == path)
                .nth(self.heading_path_occurrence)
                .map(|(index, _)| index);
            if let Some(section) = section {
                let positions = positions_in_section(&document_positions, outline, section);
                if let Some(position) = self.section_anchor.restore(&positions) {
                    return Some(position);
                }
            }
        }

        self.document_anchor
            .restore(&document_positions)
            .or_else(|| layout(document, 100).first_position())
    }
}

impl RelativeAnchor {
    fn capture(positions: &[SemanticPosition], cursor: Option<SemanticPosition>) -> Self {
        let ordinal = cursor
            .map(|cursor| {
                positions
                    .partition_point(|candidate| *candidate <= cursor)
                    .saturating_sub(1)
            })
            .unwrap_or_default();
        Self {
            ordinal,
            extent: positions.len(),
        }
    }

    fn restore(self, positions: &[SemanticPosition]) -> Option<SemanticPosition> {
        if positions.is_empty() {
            return None;
        }
        let ordinal = if self.extent <= 1 {
            0
        } else {
            self.ordinal
                .min(self.extent - 1)
                .saturating_mul(positions.len() - 1)
                .saturating_add((self.extent - 1) / 2)
                / (self.extent - 1)
        };
        positions.get(ordinal).copied()
    }
}

fn selection_contains(
    selection: Selection,
    cursor: SemanticPosition,
    position: SemanticPosition,
    rendered: &RenderedDocument,
) -> bool {
    match selection.mode {
        SelectionMode::Characterwise => {
            let (start, end) = copy::ordered_endpoints(selection.anchor, cursor);
            position >= start && position <= end
        }
        SelectionMode::Row => {
            let Some(anchor_row) = rendered.row_for_position(selection.anchor) else {
                return false;
            };
            let Some(cursor_row) = rendered.row_for_position(cursor) else {
                return false;
            };
            let Some(position_row) = rendered.row_for_position(position) else {
                return false;
            };
            let start_row = anchor_row.min(cursor_row);
            let end_row = anchor_row.max(cursor_row);
            (start_row..=end_row).contains(&position_row)
        }
    }
}

fn outline_label(block: &crate::Block) -> String {
    block
        .spans()
        .iter()
        .map(|span| {
            span.image().map_or_else(
                || span.text(),
                |image| {
                    if image.alt_text().is_empty() {
                        "(image)"
                    } else {
                        image.alt_text()
                    }
                },
            )
        })
        .collect()
}

fn outline_entries(document: &Document) -> Vec<OutlineEntry> {
    document
        .blocks()
        .iter()
        .enumerate()
        .filter_map(|(block, content)| {
            let BlockKind::Heading(level) = content.kind() else {
                return None;
            };
            Some(OutlineEntry {
                position: SemanticPosition { block, grapheme: 0 },
                level,
                label: outline_label(content),
                collapsed: false,
            })
        })
        .collect()
}

fn heading_paths(outline: &[OutlineEntry]) -> Vec<Vec<(HeadingLevel, String)>> {
    let mut current = Vec::<(HeadingLevel, String)>::new();
    let mut paths = Vec::with_capacity(outline.len());
    for entry in outline {
        while current
            .last()
            .is_some_and(|(level, _)| level.depth() >= entry.level.depth())
        {
            current.pop();
        }
        current.push((entry.level, entry.label.clone()));
        paths.push(current.clone());
    }
    paths
}

fn positions_in_section(
    positions: &[SemanticPosition],
    outline: &[OutlineEntry],
    index: usize,
) -> Vec<SemanticPosition> {
    let start = outline[index].position.block;
    let end = outline
        .get(index + 1)
        .map_or(usize::MAX, |entry| entry.position.block);
    positions
        .iter()
        .copied()
        .filter(|position| (start..end).contains(&position.block))
        .collect()
}

fn fragment_targets(document: &Document) -> BTreeMap<String, SemanticPosition> {
    let mut targets = BTreeMap::new();
    let mut heading_counts = HashMap::<String, usize>::new();

    for (block_index, block) in document.blocks().iter().enumerate() {
        if matches!(block.kind(), BlockKind::Heading(_)) {
            let base = github_heading_slug(block.text());
            let slug = unique_heading_slug(&base, &mut heading_counts);
            targets.entry(slug).or_insert(SemanticPosition {
                block: block_index,
                grapheme: 0,
            });
        }

        let mut grapheme = 0_usize;
        for span in block_spans(block) {
            let span_graphemes = span_grapheme_count(span);
            if let Some(target) = span.link_target() {
                if let Some(label) = target.strip_prefix("#fn-") {
                    let label = label.to_lowercase();
                    targets.entry(format!("fnref-{label}")).or_insert(SemanticPosition {
                        block: block_index,
                        grapheme,
                    });
                } else if let Some(label) = target.strip_prefix("#fnref-") {
                    let label = label.to_lowercase();
                    targets.entry(format!("fn-{label}")).or_insert(SemanticPosition {
                        block: block_index,
                        grapheme,
                    });
                }
            }
            grapheme += span_graphemes;
        }
    }

    targets
}

fn block_spans(block: &crate::Block) -> Vec<&crate::InlineSpan> {
    if let Some(table) = block.table() {
        table
            .rows()
            .iter()
            .flat_map(|row| row.cells())
            .flat_map(|cell| cell.spans())
            .collect()
    } else {
        block.spans().iter().collect()
    }
}

fn span_grapheme_count(span: &crate::InlineSpan) -> usize {
    if span.image().is_some() {
        1
    } else {
        span.text().graphemes(true).count()
    }
}

fn link_target_at_position(document: &Document, cursor: SemanticPosition) -> Option<String> {
    let block = document.blocks().get(cursor.block)?;
    if let Some(table) = block.table() {
        for cell in table.rows().iter().flat_map(|row| row.cells()) {
            let mut grapheme = cell.grapheme_offset();
            for span in cell.spans() {
                let count = span_grapheme_count(span);
                if cursor.grapheme >= grapheme && cursor.grapheme < grapheme + count {
                    return span.link_target().map(str::to_owned);
                }
                grapheme += count;
            }
        }
        return None;
    }

    let mut grapheme = 0_usize;
    for span in block.spans() {
        let count = span_grapheme_count(span);
        if cursor.grapheme >= grapheme && cursor.grapheme < grapheme + count {
            return span.link_target().map(str::to_owned);
        }
        grapheme += count;
    }
    None
}

fn github_heading_slug(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    for character in text.to_lowercase().chars() {
        if character.is_whitespace() {
            slug.push('-');
            continue;
        }
        if github_slug_keeps(character) {
            slug.push(character);
        }
    }
    slug
}

fn github_slug_keeps(character: char) -> bool {
    match character {
        '\u{2000}'..='\u{206F}' | '\u{2E00}'..='\u{2E7F}' => false,
        '\'' | '!' | '"' | '#' | '$' | '%' | '&' | '(' | ')' | '*' | '+' | ',' | '.' | '/'
        | ':' | ';' | '<' | '=' | '>' | '?' | '@' | '[' | '\\' | ']' | '^' | '`' | '{' | '|'
        | '}' | '~' => false,
        _ => true,
    }
}

fn unique_heading_slug(base: &str, counts: &mut HashMap<String, usize>) -> String {
    let mut slug = base.to_owned();
    let original = base.to_owned();
    while counts.contains_key(&slug) {
        let next = counts.get(&original).copied().unwrap_or(0).saturating_add(1);
        counts.insert(original.clone(), next);
        slug = format!("{original}-{next}");
    }
    counts.insert(slug.clone(), 0);
    slug
}

fn looks_like_relative_path(target: &str) -> bool {
    target.starts_with("./")
        || target.starts_with("../")
        || target.starts_with('/')
        || (!target.contains(':') && !target.starts_with('#'))
}

fn is_web_url(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Motion {
    Left,
    Down,
    Up,
    Right,
    WordForward,
    WordBackward,
    RowStart,
    FirstNonBlank,
    RowEnd,
    ParagraphBackward,
    ParagraphForward,
}

fn horizontal_target(
    rendered: &RenderedDocument,
    location: CellLocation,
    motion: Motion,
    count: usize,
) -> Option<SemanticPosition> {
    let cells = rendered
        .rows()
        .iter()
        .flat_map(|row| row.cells())
        .filter(|cell| cell.is_navigable())
        .collect::<Vec<_>>();
    let current = cells
        .iter()
        .position(|cell| cell.position() == location.position)?;
    let target = match motion {
        Motion::Left => current.saturating_sub(count),
        Motion::Right => current
            .saturating_add(count)
            .min(cells.len().saturating_sub(1)),
        _ => unreachable!(),
    };
    cells.get(target).map(|cell| cell.position())
}

fn vertical_target(
    rendered: &RenderedDocument,
    location: CellLocation,
    column: usize,
    motion: Motion,
    count: usize,
) -> Option<SemanticPosition> {
    let mut row = location.row;
    let mut target = location.position;

    for _ in 0..count {
        let next = match motion {
            Motion::Up => (0..row).rev().find_map(|candidate| {
                rendered
                    .nearest_position(candidate, column)
                    .map(|position| (candidate, position))
            }),
            Motion::Down => (row + 1..rendered.rows().len()).find_map(|candidate| {
                rendered
                    .nearest_position(candidate, column)
                    .map(|position| (candidate, position))
            }),
            _ => unreachable!(),
        };
        let Some((next_row, next_target)) = next else {
            break;
        };
        row = next_row;
        target = next_target;
    }

    Some(target)
}

fn word_target(
    document: &Document,
    cursor: SemanticPosition,
    motion: Motion,
    count: usize,
) -> Option<SemanticPosition> {
    let graphemes = semantic_graphemes(document);
    let mut index = graphemes
        .iter()
        .position(|(position, _)| *position == cursor)?;

    for _ in 0..count {
        index = match motion {
            Motion::WordForward => next_word(&graphemes, index),
            Motion::WordBackward => previous_word(&graphemes, index),
            _ => unreachable!(),
        };
    }
    Some(graphemes[index].0)
}

fn semantic_graphemes(document: &Document) -> Vec<(SemanticPosition, &str)> {
    document
        .blocks()
        .iter()
        .enumerate()
        .flat_map(|(block, content)| {
            content
                .text()
                .graphemes(true)
                .enumerate()
                .map(move |(grapheme, text)| (SemanticPosition { block, grapheme }, text))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WordClass {
    Whitespace,
    Keyword,
    Punctuation,
}

fn word_class(grapheme: &str) -> WordClass {
    if grapheme.chars().all(char::is_whitespace) {
        WordClass::Whitespace
    } else if grapheme
        .chars()
        .any(|character| character.is_alphanumeric() || character == '_')
    {
        WordClass::Keyword
    } else {
        WordClass::Punctuation
    }
}

fn next_word(graphemes: &[(SemanticPosition, &str)], current: usize) -> usize {
    let mut index = current;
    let class = word_class(graphemes[index].1);

    while class != WordClass::Whitespace
        && index + 1 < graphemes.len()
        && graphemes[index].0.block == graphemes[index + 1].0.block
        && word_class(graphemes[index + 1].1) == class
    {
        index += 1;
    }
    if index + 1 < graphemes.len() {
        index += 1;
    }
    while index + 1 < graphemes.len() && word_class(graphemes[index].1) == WordClass::Whitespace {
        index += 1;
    }
    while index > 0 && word_class(graphemes[index].1) == WordClass::Whitespace {
        index -= 1;
    }
    index
}

fn previous_word(graphemes: &[(SemanticPosition, &str)], current: usize) -> usize {
    let mut index = current.saturating_sub(1);
    while index > 0 && word_class(graphemes[index].1) == WordClass::Whitespace {
        index -= 1;
    }
    let class = word_class(graphemes[index].1);
    while index > 0
        && graphemes[index].0.block == graphemes[index - 1].0.block
        && word_class(graphemes[index - 1].1) == class
    {
        index -= 1;
    }
    index
}

fn paragraph_target(
    rendered: &RenderedDocument,
    cursor: SemanticPosition,
    motion: Motion,
    count: usize,
) -> Option<SemanticPosition> {
    let starts = rendered
        .rows()
        .iter()
        .flat_map(|row| row.cells())
        .filter(|cell| cell.is_navigable())
        .map(|cell| cell.position())
        .fold(Vec::new(), |mut starts, position| {
            if starts
                .last()
                .is_none_or(|last: &SemanticPosition| last.block != position.block)
            {
                starts.push(position);
            }
            starts
        });
    let current = starts
        .iter()
        .position(|position| position.block == cursor.block)?;
    let target = match motion {
        Motion::ParagraphBackward => current.saturating_sub(count),
        Motion::ParagraphForward => current
            .saturating_add(count)
            .min(starts.len().saturating_sub(1)),
        _ => unreachable!(),
    };
    starts.get(target).copied()
}

pub struct Harness {
    session: ReadingSession,
    terminal: Terminal<TestBackend>,
    clipboard: FakeClipboard,
    browser: FakeBrowser,
    effect_log: Vec<Effect>,
}

impl Harness {
    #[must_use]
    pub fn new(document: Document, width: u16, height: u16) -> Self {
        Self::with_session(ReadingSession::new(document), width, height)
    }

    #[must_use]
    pub fn with_highlighter(
        document: Document,
        width: u16,
        height: u16,
        highlighter: impl CodeHighlighter,
    ) -> Self {
        Self::with_session(
            ReadingSession::with_highlight_cache(
                document,
                None,
                HighlightCache::with_highlighter(highlighter),
            ),
            width,
            height,
        )
    }

    fn with_session(mut session: ReadingSession, width: u16, height: u16) -> Self {
        let backend = TestBackend::new(width, height);
        let terminal = Terminal::new(backend).expect("TestBackend is infallible");
        session.resize(width, height);
        let mut harness = Self {
            session,
            terminal,
            clipboard: FakeClipboard::succeeding(),
            browser: FakeBrowser::succeeding(),
            effect_log: Vec::new(),
        };
        harness.draw();
        harness
    }

    pub fn set_clipboard_result(&mut self, result: ClipboardResult) {
        self.clipboard.result = result;
    }

    pub fn set_browser_result(&mut self, result: BrowserResult) {
        self.browser.result = result;
    }

    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effect_log)
    }

    fn apply_effects(&mut self) {
        for effect in self.session.drain_effects() {
            match &effect {
                Effect::WriteClipboard(text) => {
                    let result = self.clipboard.write_text(text);
                    self.session.report_clipboard_result(result);
                }
                Effect::OpenBrowser(url) => {
                    let result = self.browser.open_url(url);
                    self.session.report_browser_result(result);
                }
                Effect::ReloadDocument(path) => {
                    let area = self.terminal.backend().buffer().area;
                    crate::reload::apply(&mut self.session, path, area.width, area.height);
                }
            }
            self.effect_log.push(effect);
        }
    }

    pub fn open(path: impl AsRef<Path>, width: u16, height: u16) -> Result<Self, SourceError> {
        let path = path.as_ref().to_owned();
        Ok(Self::with_session(
            ReadingSession::with_source(load_document(&path)?, path),
            width,
            height,
        ))
    }

    pub fn open_with_highlighter(
        path: impl AsRef<Path>,
        width: u16,
        height: u16,
        highlighter: impl CodeHighlighter,
    ) -> Result<Self, SourceError> {
        let path = path.as_ref().to_owned();
        Ok(Self::with_session(
            ReadingSession::with_highlight_cache(
                load_document(&path)?,
                Some(path),
                HighlightCache::with_highlighter(highlighter),
            ),
            width,
            height,
        ))
    }

    pub fn command(&mut self, command: Command) {
        self.session.command(command);
        self.draw();
    }

    pub fn keys(&mut self, keys: &str) {
        for character in keys.chars() {
            self.key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
        }
    }

    pub fn control(&mut self, character: char) {
        self.key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::CONTROL,
        ));
    }

    pub fn key(&mut self, key: KeyEvent) {
        let area = self.terminal.backend().buffer().area;
        self.session.key(key, area.width, area.height);
        self.apply_effects();
        self.draw();
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.terminal.backend_mut().resize(width, height);
        self.terminal
            .resize(Rect::new(0, 0, width, height))
            .expect("TestBackend is infallible");
        self.session.resize(width, height);
        self.draw();
    }

    #[must_use]
    pub fn has_quit(&self) -> bool {
        self.session.has_quit()
    }

    #[must_use]
    pub fn cursor(&self) -> Option<SemanticPosition> {
        self.session.cursor()
    }

    #[must_use]
    pub fn current_section(&self) -> Option<SemanticPosition> {
        self.session.current_section()
    }

    #[must_use]
    pub fn outline_selection(&self) -> Option<SemanticPosition> {
        self.session.outline_selection()
    }

    #[must_use]
    pub fn focus(&self) -> PaneFocus {
        self.session.focus()
    }

    #[must_use]
    pub fn selection_mode(&self) -> Option<SelectionMode> {
        self.session.selection_mode()
    }

    #[must_use]
    pub fn selection_anchor(&self) -> Option<SemanticPosition> {
        self.session.selection_anchor()
    }

    #[must_use]
    pub fn selection_contains(&self, position: SemanticPosition) -> bool {
        let width = self.terminal.backend().buffer().area.width;
        self.session.selection_contains(position, width)
    }

    #[must_use]
    pub fn document(&self) -> &Document {
        self.session.document()
    }

    #[must_use]
    pub fn viewport(&self) -> usize {
        self.session.viewport()
    }

    #[must_use]
    pub fn cursor_cell(&self) -> Option<CellLocation> {
        self.screen_cell(self.session.cursor()?)
    }

    #[must_use]
    pub fn cursor_is_highlighted(&self) -> bool {
        self.cursor()
            .and_then(|cursor| self.modifier_at(cursor))
            .is_some_and(|modifier| modifier.contains(Modifier::REVERSED))
    }

    #[must_use]
    pub fn modifier_at(&self, position: SemanticPosition) -> Option<Modifier> {
        let location = self.screen_cell(position)?;
        let column = u16::try_from(location.column).ok()?;
        let row = u16::try_from(location.row).ok()?;
        Some(self.terminal.backend().buffer()[(column, row)].modifier)
    }

    #[must_use]
    pub fn screen_modifier(&self, column: u16, row: u16) -> Option<Modifier> {
        let buffer = self.terminal.backend().buffer();
        (column < buffer.area.width && row < buffer.area.height)
            .then(|| buffer[(column, row)].modifier)
    }

    #[must_use]
    pub fn outline_modifier_at(&self, position: SemanticPosition) -> Option<Modifier> {
        let index = self
            .session
            .outline()
            .iter()
            .position(|entry| entry.position == position)?;
        let row = self
            .session
            .visible_outline_indices()
            .iter()
            .position(|candidate| *candidate == index)?;
        let row = row.checked_sub(self.session.outline_viewport())?;
        let row = u16::try_from(row).ok()?;
        let buffer = self.terminal.backend().buffer();
        if row >= self.session.content_height(buffer.area.height) {
            return None;
        }
        Some(buffer[(0, row)].modifier)
    }

    #[must_use]
    pub fn foreground_at(&self, position: SemanticPosition) -> Option<Color> {
        let location = self.screen_cell(position)?;
        let column = u16::try_from(location.column).ok()?;
        let row = u16::try_from(location.row).ok()?;
        Some(self.terminal.backend().buffer()[(column, row)].fg)
    }

    #[must_use]
    pub fn highlight_at(&self, position: SemanticPosition) -> Option<crate::HighlightStyle> {
        let area = self.terminal.backend().buffer().area;
        self.session
            .rendered(area.width)
            .rows()
            .iter()
            .flat_map(|row| row.cells())
            .find(|cell| cell.position() == position)
            .and_then(|cell| cell.style().highlight())
    }

    pub fn settle_highlighting(&mut self) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while self.session.highlighting_pending() && std::time::Instant::now() < deadline {
            std::thread::yield_now();
            self.draw();
        }
        assert!(
            !self.session.highlighting_pending(),
            "highlighting did not settle within ten seconds"
        );
        self.draw();
    }

    fn screen_cell(&self, position: SemanticPosition) -> Option<CellLocation> {
        let area = self.terminal.backend().buffer().area;
        let panes = self.session.pane_layout(area.width);
        let mut location = self
            .session
            .rendered(area.width)
            .cell_for_position(position)?;
        let screen_row = location.row.checked_sub(self.session.viewport())?;
        location.column = location
            .column
            .saturating_add(usize::from(panes.document_x));
        if screen_row >= usize::from(area.height) || location.column >= usize::from(area.width) {
            return None;
        }
        location.row = screen_row;
        Some(location)
    }

    #[must_use]
    pub fn frame(&self) -> String {
        let buffer = self.terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn draw(&mut self) {
        let area = self.terminal.backend().buffer().area;
        self.session.prepare_highlighting(area.width, area.height);
        self.terminal
            .draw(|frame| ui::render(frame, &self.session))
            .expect("TestBackend is infallible");
    }
}
