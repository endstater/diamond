// SPDX-License-Identifier: GPL-3.0-only
//! Native egui editors over a semantic document, UTF-8 source, or explicit hex.

use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use diamond_document::{Document, MarkKind};
use egui::text::LayoutJob;
use egui::{Color32, FontId, Key, Modifiers, TextBuffer, TextFormat};
use thiserror::Error;

mod emoji;
mod live;
mod math;
mod mermaid;
mod property_block;
mod source_history;
mod source_syntax;
mod syntax;
pub use emoji::install_font as install_emoji_font;
pub fn syntax_licenses() -> &'static str {
    static LICENSES: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    LICENSES.get_or_init(|| two_face::acknowledgement::listing().to_md())
}
mod math_normalize;
mod media;
pub use media::Loader as ImageLoader;
pub fn invalidate_font_layouts(ctx: &egui::Context) {
    ctx.data_mut(|data| {
        let key = egui::Id::new("font-layout-epoch");
        let next = data.get_temp::<u64>(key).unwrap_or(0).wrapping_add(1);
        data.insert_temp(key, next);
    });
}
pub(crate) fn font_epoch(ctx: &egui::Context) -> u64 {
    ctx.data(|data| data.get_temp::<u64>(egui::Id::new("font-layout-epoch")))
        .unwrap_or(0)
}
pub fn decode_preview_png(bytes: &[u8]) -> Result<egui::ColorImage, String> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("Preview pages must be PNG".into());
    }
    media::decode(bytes)
}
mod search;
pub mod svg;

static NEXT_EDITOR_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorMode {
    Formatted,
    Image,
    Source,
    Hex,
}

#[derive(Debug, Error)]
pub enum EditorError {
    #[error("The buffer is not valid UTF-8; use hexadecimal mode.")]
    InvalidUtf8,
    #[error("Live Markdown is available only for .md files.")]
    FormattingNotAllowed,
    #[error("Invalid hexadecimal buffer: {0}")]
    InvalidHex(String),
    #[error("{0}")]
    Document(String),
}

#[derive(Clone, Debug, Default)]
pub struct EditorResponse {
    pub open_link: Option<String>,
    pub changed: bool,
    /// True until native IME composition commits or is cancelled.
    pub composing: bool,
    /// An error produced by this frame; the desktop displays a timed notification.
    pub error: Option<String>,
}

pub struct EditorBuffer {
    images: media::Images,
    syntax_language: String,
    source_layout: syntax::LayoutCache,
    source_history: source_history::History,
    language: diamond_core::Language,
    id: u64,
    mode: EditorMode,
    document: Option<Document>,
    source: String,
    hex: String,
    initially_binary: bool,
    original_bytes: Vec<u8>,
    hex_initialized: bool,
    image_file: Option<diamond_document::Image>,
    image_read_error: Option<String>,
    formatting_allowed: bool,
    live: live::LiveState,
    raw_composition: Option<(EditorMode, String)>,
    reset_raw_native: bool,
    selection: Range<usize>,
    navigation_offset: usize,
    show_find: bool,
    find: String,
    replacement: String,
    search: search::SearchState,
    generation: u64,
    font_family: egui::FontFamily,
    word_wrap: bool,
    line_numbers: bool,
    line_number_cache: (u64, usize),
}

impl EditorBuffer {
    /// Select a safe default from the real file extension. Only `.md` files
    /// expose the live Markdown mode; binary content always starts in hex.
    pub fn for_path(path: &Path, bytes: &[u8]) -> Self {
        let markdown = path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"));
        let mut initial = if is_image_path(path) {
            Self::from_bytes_inner(&[], false)
        } else {
            Self::from_bytes_inner(bytes, markdown)
        };
        if is_image_path(path) {
            initial.original_bytes = bytes.to_vec();
            initial.initially_binary = true;
            initial.document = None;
        }
        let mut buffer = initial.with_formatting_allowed(
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("md")),
        );
        buffer.syntax_language = path
            .extension()
            .and_then(|ext| ext.to_str())
            .or_else(|| path.file_name().and_then(|name| name.to_str()))
            .unwrap_or("txt")
            .to_owned();
        if is_image_path(path) {
            buffer.mode = EditorMode::Image;
            buffer.image_file = Some(diamond_document::Image {
                target: "diamond-open-image".into(),
                title: String::new(),
                alt: String::new(),
            });
            let bytes = std::sync::Arc::new(bytes.to_vec());
            buffer
                .images
                .set_loader(std::sync::Arc::new(move |_| Ok(bytes.as_ref().clone())));
        }
        buffer
    }

    pub fn with_formatting_allowed(mut self, allowed: bool) -> Self {
        self.formatting_allowed = allowed;
        if !allowed && self.mode == EditorMode::Formatted {
            self.source = self
                .document
                .as_ref()
                .map_or_else(String::new, Document::to_markdown);
            self.mode = EditorMode::Source;
        }
        self
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self::from_bytes_inner(bytes, true)
    }

    fn from_bytes_inner(bytes: &[u8], formatting_allowed: bool) -> Self {
        let decoded = std::str::from_utf8(bytes).ok();
        Self {
            images: media::Images::default(),
            syntax_language: "txt".into(),
            source_layout: Default::default(),
            source_history: Default::default(),
            language: diamond_core::Language::En,
            id: NEXT_EDITOR_ID.fetch_add(1, Ordering::Relaxed),
            mode: if decoded.is_some() {
                if formatting_allowed {
                    EditorMode::Formatted
                } else {
                    EditorMode::Source
                }
            } else {
                EditorMode::Hex
            },
            document: decoded.filter(|_| formatting_allowed).map(Document::parse),
            source: decoded.unwrap_or_default().to_owned(),
            hex: String::new(),
            original_bytes: if decoded.is_none() {
                bytes.to_vec()
            } else {
                Vec::new()
            },
            hex_initialized: false,
            image_file: None,
            image_read_error: None,
            initially_binary: decoded.is_none(),
            formatting_allowed,
            live: live::LiveState::default(),
            raw_composition: None,
            reset_raw_native: false,
            selection: 0..0,
            navigation_offset: 0,
            show_find: false,
            find: String::new(),
            replacement: String::new(),
            search: search::SearchState::default(),
            generation: 0,
            font_family: egui::FontFamily::Proportional,
            word_wrap: true,
            line_numbers: true,
            line_number_cache: (u64::MAX, 1),
        }
    }

    pub fn jump_to_source(&mut self, offset: usize) {
        let length = match self.mode {
            EditorMode::Formatted => self
                .document
                .as_ref()
                .map_or(0, |document| document.to_markdown().chars().count()),
            EditorMode::Source => self.source.chars().count(),
            EditorMode::Hex => self.hex.chars().count(),
            EditorMode::Image => 0,
        };
        let offset = offset.min(length);
        self.search.jump = Some(offset..offset);
        self.search.jump_focus = true;
    }
    pub fn preview_offset(&self) -> usize {
        if self.mode == EditorMode::Formatted {
            self.live.preview_offset()
        } else {
            self.navigation_offset
        }
    }
    pub fn cursor_offset(&self) -> usize {
        if self.mode == EditorMode::Formatted {
            self.live.cursor_offset()
        } else {
            self.selection.end
        }
    }
    pub fn revision_key(&self) -> (u64, u64) {
        (self.id, self.generation)
    }

    pub fn set_unreadable_image(&mut self, error: String) {
        self.image_read_error = Some(error.clone());
        self.images
            .set_loader(std::sync::Arc::new(move |_| Err(error.clone())));
    }
    pub fn set_image_loader(&mut self, loader: ImageLoader) {
        if self.image_file.is_none() {
            self.images.set_loader(loader);
        }
    }

    pub fn retarget_path(&mut self, path: &Path) {
        self.syntax_language = path
            .extension()
            .and_then(|ext| ext.to_str())
            .or_else(|| path.file_name().and_then(|name| name.to_str()))
            .unwrap_or("txt")
            .into();
        let allowed = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
        if !allowed && self.mode == EditorMode::Formatted {
            let _ = self.set_mode(EditorMode::Source);
        }
        self.formatting_allowed = allowed;
    }

    pub fn set_language(&mut self, language: diamond_core::Language) {
        self.language = language;
    }

    pub fn set_view_options(&mut self, family: egui::FontFamily, wrap: bool, line_numbers: bool) {
        self.font_family = family;
        self.word_wrap = wrap;
        self.line_numbers = line_numbers;
    }

    pub fn open_find(&mut self) {
        self.show_find = true;
        self.search.focus = true;
    }

    fn refresh_search(&mut self) {
        if !self.show_find || !self.search.needs_refresh(&self.find, self.generation) {
            return;
        }
        let text = match self.mode {
            EditorMode::Formatted => self
                .document
                .as_ref()
                .map_or_else(String::new, Document::to_markdown),
            EditorMode::Source => self.source.clone(),
            EditorMode::Hex => self.hex.clone(),
            EditorMode::Image => String::new(),
        };
        self.search.refresh(&self.find, &text, self.generation);
    }

    pub fn bytes(&self) -> Result<Vec<u8>, EditorError> {
        match self.mode {
            EditorMode::Formatted => Ok(self.document.as_ref().map_or_else(Vec::new, |document| {
                document.source_map_shared().source.as_bytes().to_vec()
            })),
            EditorMode::Source => Ok(self.source.as_bytes().to_vec()),
            EditorMode::Image => Ok(self.original_bytes.clone()),
            EditorMode::Hex => {
                if self.hex_initialized || !self.hex.is_empty() {
                    decode_hex(&self.hex)
                } else {
                    Ok(self.original_bytes.clone())
                }
            }
        }
    }

    pub fn matches_bytes(&self, bytes: &[u8]) -> bool {
        match self.mode {
            EditorMode::Formatted => self
                .document
                .as_ref()
                .is_some_and(|document| document.source_map_shared().source.as_bytes() == bytes),
            EditorMode::Source => self.source.as_bytes() == bytes,
            EditorMode::Image => self.original_bytes.as_slice() == bytes,
            EditorMode::Hex => {
                if self.hex_initialized || !self.hex.is_empty() {
                    decode_hex(&self.hex).is_ok_and(|decoded| decoded == bytes)
                } else {
                    self.original_bytes.as_slice() == bytes
                }
            }
        }
    }

    pub fn mode(&self) -> EditorMode {
        self.mode
    }

    pub fn is_composing(&self) -> bool {
        self.live.is_composing() || self.raw_composition.is_some()
    }

    /// Discard only an uncommitted IME candidate when leaving a tab. Accepted
    /// content and the semantic document's undo history remain intact.
    pub fn cancel_composition(&mut self) -> bool {
        let mut cancelled = false;
        if let Some(document) = self.document.as_mut() {
            cancelled |= self.live.cancel_composition(document);
        }
        if let Some((mode, before)) = self.raw_composition.take() {
            match mode {
                EditorMode::Source => {
                    self.source = before;
                    self.source_history.cancel_composition();
                }
                EditorMode::Hex => self.hex = before,
                EditorMode::Formatted | EditorMode::Image => {}
            }
            self.reset_raw_native = true;
            cancelled = true;
        }
        if cancelled {
            self.generation = self.generation.wrapping_add(1);
        }
        cancelled
    }

    pub fn is_binary(&self) -> bool {
        self.bytes().map_or(self.initially_binary, |bytes| {
            std::str::from_utf8(&bytes).is_err()
        })
    }

    pub fn document(&self) -> Option<&Document> {
        self.document.as_ref()
    }

    /// Current serialized UTF-8 content, including Markdown syntax.
    pub fn source_text(&self) -> Option<String> {
        String::from_utf8(self.bytes().ok()?).ok()
    }

    pub fn set_mode(&mut self, mode: EditorMode) -> Result<(), EditorError> {
        if mode == EditorMode::Formatted && !self.formatting_allowed {
            return Err(EditorError::FormattingNotAllowed);
        }
        if self.image_read_error.is_some() && mode != EditorMode::Image {
            return Err(EditorError::Document(
                "Image exceeds the read limit or cannot be read".into(),
            ));
        }
        if self.mode == mode {
            return Ok(());
        }
        if self.is_composing() {
            return Err(EditorError::Document(
                "Finish text composition before switching modes.".into(),
            ));
        }
        let bytes = self.bytes()?;
        match mode {
            EditorMode::Image => {
                if self.image_file.is_none() {
                    return Err(EditorError::Document("This file is not an image".into()));
                }
                self.original_bytes = bytes.clone();
                let bytes = std::sync::Arc::new(bytes);
                self.images
                    .set_loader(std::sync::Arc::new(move |_| Ok(bytes.as_ref().clone())));
            }
            EditorMode::Hex => {
                self.hex = encode_hex(&bytes);
                self.hex_initialized = true;
            }
            EditorMode::Formatted | EditorMode::Source => {
                let text = String::from_utf8(bytes).map_err(|_| EditorError::InvalidUtf8)?;
                if mode == EditorMode::Formatted
                    && self
                        .document
                        .as_ref()
                        .is_none_or(|document| document.to_markdown() != text)
                {
                    self.document = Some(Document::parse(&text));
                }
                self.source = text;
            }
        }
        self.source_history = Default::default();
        self.mode = mode;
        self.generation = self.generation.wrapping_add(1);
        self.live.reset();
        self.reset_raw_native = true;
        self.selection = 0..0;
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        if self.is_composing() {
            return false;
        }
        if self.mode == EditorMode::Source {
            if let Some(selection) = self.source_history.undo(&mut self.source) {
                self.selection = selection;
                self.reset_raw_native = true;
                self.generation = self.generation.wrapping_add(1);
                return true;
            }
            return false;
        }
        let changed = self.mode == EditorMode::Formatted
            && self.document.as_mut().is_some_and(Document::undo);
        if changed {
            self.generation = self.generation.wrapping_add(1)
        }
        changed
    }

    pub fn redo(&mut self) -> bool {
        if self.is_composing() {
            return false;
        }
        if self.mode == EditorMode::Source {
            if let Some(selection) = self.source_history.redo(&mut self.source) {
                self.selection = selection;
                self.reset_raw_native = true;
                self.generation = self.generation.wrapping_add(1);
                return true;
            }
            return false;
        }
        let changed = self.mode == EditorMode::Formatted
            && self.document.as_mut().is_some_and(Document::redo);
        if changed {
            self.generation = self.generation.wrapping_add(1)
        }
        changed
    }

    pub fn toggle_mark(&mut self, kind: MarkKind) -> Result<(), EditorError> {
        let document = self.document.as_mut().ok_or(EditorError::InvalidUtf8)?;
        if self.mode != EditorMode::Formatted {
            return Err(EditorError::Document(
                "Select formatted mode to apply formatting.".into(),
            ));
        }
        let result = document
            .toggle_mark(self.selection.clone(), kind)
            .map_err(|error| EditorError::Document(error.to_string()));
        if result.is_ok() {
            self.generation = self.generation.wrapping_add(1);
        }
        result
    }

    pub fn find_all(&self, needle: &str) -> Vec<Range<usize>> {
        if needle.is_empty() {
            return Vec::new();
        }
        let text = self.visible_text();
        text.match_indices(needle)
            .map(|(start, matched)| {
                let scalar_start = text[..start].chars().count();
                scalar_start..scalar_start + matched.chars().count()
            })
            .collect()
    }

    /// Literal, case-sensitive replacement. Errors leave the buffer unchanged.
    pub fn replace_all(&mut self, needle: &str, replacement: &str) -> Result<usize, EditorError> {
        if self.is_composing() {
            return Err(EditorError::Document(
                "Finish text composition before replacing text.".into(),
            ));
        }
        let ranges = self.find_all(needle);
        if ranges.is_empty() {
            return Ok(0);
        }
        match self.mode {
            EditorMode::Formatted => {
                let mut next = self
                    .document
                    .as_ref()
                    .ok_or(EditorError::InvalidUtf8)?
                    .clone();
                for range in ranges.iter().rev() {
                    next.replace_range(range.clone(), replacement)
                        .map_err(|error| EditorError::Document(error.to_string()))?;
                }
                self.document = Some(next);
            }
            EditorMode::Image => return Ok(0),
            EditorMode::Source => {
                let next = self.source.replace(needle, replacement);
                let edits = source_history::difference(&self.source, &next);
                self.source_history
                    .record(edits, self.selection.clone(), self.selection.clone());
                self.source = next;
            }
            EditorMode::Hex => self.hex = self.hex.replace(needle, replacement),
        }
        self.generation = self.generation.wrapping_add(1);
        Ok(ranges.len())
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> EditorResponse {
        self.ui_with_options(ui, 16.0)
    }

    pub fn ui_with_options(&mut self, ui: &mut egui::Ui, font_size: f32) -> EditorResponse {
        let lang = self.language;
        let tr = |key| lang.text(key);
        self.images.begin_frame(ui.ctx());
        let font_size = font_size.clamp(10.0, 36.0);
        let mut response = EditorResponse::default();
        if ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::F)) {
            self.open_find();
        }
        let exit_find =
            self.show_find && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
        if exit_find {
            self.show_find = false;
            self.search.jump = None;
        }
        let mut new_mode = None;
        ui.horizontal(|ui| {
            if self.formatting_allowed
                && ui
                    .selectable_label(self.mode == EditorMode::Formatted, tr("Live"))
                    .clicked()
            {
                new_mode = Some(EditorMode::Formatted);
            }
            for (mode, label) in [
                (
                    if self.image_file.is_some() {
                        EditorMode::Image
                    } else {
                        EditorMode::Source
                    },
                    if self.image_file.is_some() {
                        tr("Image")
                    } else {
                        tr("Source")
                    },
                ),
                (EditorMode::Hex, tr("Hex")),
            ] {
                if ui.selectable_label(self.mode == mode, label).clicked() {
                    new_mode = Some(mode);
                }
            }
            if svg::button(
                ui,
                tr("Поиск · Ctrl+F"),
                include_bytes!("../../../assets/lucide/search.svg"),
            )
            .clicked()
            {
                self.open_find();
            }
        });
        if let Some(mode) = new_mode {
            match self.set_mode(mode) {
                Ok(()) => {}
                Err(error) => self.record_error(error.to_string(), &mut response),
            }
        }
        self.refresh_search();
        if self.show_find {
            self.find_ui(ui, &mut response);
        }
        self.refresh_search();
        ui.separator();

        if self.mode == EditorMode::Image {
            if let Some(image) = &self.image_file {
                let area = ui.available_rect_before_wrap();
                let edge = area.width().min(area.height()).max(1.0);
                let rect = egui::Rect::from_center_size(area.center(), egui::Vec2::splat(edge));
                self.images.show(ui, rect, image, self.language);
                ui.allocate_rect(area, egui::Sense::hover());
            }
            return response;
        }
        if self.mode == EditorMode::Hex && !self.hex_initialized {
            self.hex = encode_hex(&self.original_bytes);
            self.hex_initialized = true;
        }
        let edit_id = ui.make_persistent_id(("diamond-editor", self.id, self.mode as u8));
        if exit_find {
            ui.memory_mut(|memory| memory.request_focus(edit_id));
        }
        if self.mode != EditorMode::Formatted && self.reset_raw_native {
            let mut native = egui::text_edit::TextEditState::default();
            native.cursor.set_char_range(Some(egui::text::CCursorRange {
                primary: egui::text::CCursor::new(self.selection.end),
                secondary: egui::text::CCursor::new(self.selection.start),
                h_pos: None,
            }));
            native.store(ui.ctx(), edit_id);
            self.reset_raw_native = false;
        }
        let matches = if self.show_find {
            self.search.matches.clone()
        } else {
            Vec::new()
        };
        let active_match = if self.show_find {
            self.search.matches.get(self.search.current).cloned()
        } else {
            None
        };
        let jump = self.search.jump.take();
        let jump_focus = std::mem::take(&mut self.search.jump_focus);
        if jump.is_some() && jump_focus {
            ui.memory_mut(|memory| memory.request_focus(edit_id));
        }
        if self.mode != EditorMode::Formatted {
            if let Some(range) = &jump {
                let mut native =
                    egui::text_edit::TextEditState::load(ui.ctx(), edit_id).unwrap_or_default();
                native.cursor.set_char_range(Some(egui::text::CCursorRange {
                    primary: egui::text::CCursor::new(range.end),
                    secondary: egui::text::CCursor::new(range.start),
                    h_pos: None,
                }));
                native.store(ui.ctx(), edit_id);
            }
        }
        let accent = ui.visuals().hyperlink_color;
        let viewport_height = ui.available_height().max(font_size * 2.0);
        let family = self.font_family.clone();
        let raw_ime = if self.mode != EditorMode::Formatted {
            let events = ui.input(|input| {
                let mut flags = (false, false, false);
                for event in &input.events {
                    if let egui::Event::Ime(event) = event {
                        match event {
                            egui::ImeEvent::Preedit(_) => flags.0 = true,
                            egui::ImeEvent::Commit(_) => flags.1 = true,
                            egui::ImeEvent::Disabled => flags.2 = true,
                            _ => {}
                        }
                    }
                }
                flags
            });
            if events.0
                && self.raw_composition.is_none()
                && ui.memory(|memory| memory.has_focus(edit_id))
            {
                let before = if self.mode == EditorMode::Source {
                    self.source.clone()
                } else {
                    self.hex.clone()
                };
                self.raw_composition = Some((self.mode, before));
                if self.mode == EditorMode::Source {
                    self.source_history
                        .begin_composition(self.selection.clone());
                }
            }
            events
        } else {
            (false, false, false)
        };
        let mut raw_has_focus = false;
        if self.mode == EditorMode::Source
            && self.raw_composition.is_none()
            && !raw_ime.0
            && ui.memory(|memory| memory.has_focus(edit_id))
        {
            let redo = ui.input_mut(|input| {
                input.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
                    || input.consume_key(Modifiers::COMMAND, Key::Y)
            });
            let undo = ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::Z));
            if redo {
                response.changed |= self.redo();
            } else if undo {
                response.changed |= self.undo();
            }
            if self.reset_raw_native {
                let mut native =
                    egui::text_edit::TextEditState::load(ui.ctx(), edit_id).unwrap_or_default();
                native.cursor.set_char_range(Some(egui::text::CCursorRange {
                    primary: egui::text::CCursor::new(self.selection.end),
                    secondary: egui::text::CCursor::new(self.selection.start),
                    h_pos: None,
                }));
                native.clear_undoer();
                native.store(ui.ctx(), edit_id);
                self.reset_raw_native = false;
            }
        }
        let mut source_edits = Vec::new();
        let source_before_selection = egui::text_edit::TextEditState::load(ui.ctx(), edit_id)
            .and_then(|native| native.cursor.char_range())
            .map_or_else(
                || self.selection.clone(),
                |cursor| cursor.secondary.index..cursor.primary.index,
            );

        if self.mode == EditorMode::Source
            && self.formatting_allowed
            && self.raw_composition.is_none()
            && !raw_ime.0
            && ui.input(|input| {
                input.focused && input.modifiers == Modifiers::NONE && input.key_pressed(Key::Enter)
            })
            && ui.memory(|memory| memory.has_focus(edit_id))
            && let Some(mut native) = egui::text_edit::TextEditState::load(ui.ctx(), edit_id)
            && let Some(cursor) = native.cursor.char_range()
            && cursor.primary.index == cursor.secondary.index
            && let Some(continuation) =
                diamond_document::source_line_continuation(&self.source, cursor.primary.index)
            && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter))
        {
            let to_byte = |position| {
                self.source
                    .char_indices()
                    .map(|(byte, _)| byte)
                    .chain(std::iter::once(self.source.len()))
                    .nth(position)
                    .unwrap_or(self.source.len())
            };
            let bytes = to_byte(continuation.range.start)..to_byte(continuation.range.end);
            let target = continuation.range.start + continuation.text.chars().count();
            source_history::replace(
                &mut self.source,
                bytes,
                &continuation.text,
                &mut source_edits,
            );
            native.cursor.set_char_range(Some(egui::text::CCursorRange {
                primary: egui::text::CCursor::new(target),
                secondary: egui::text::CCursor::new(target),
                h_pos: None,
            }));
            native.store(ui.ctx(), edit_id);
            response.changed = true;
        }

        match self.mode {
            EditorMode::Image => {}
            EditorMode::Formatted => {
                if let Some(document) = self.document.as_mut() {
                    let options = live::ViewOptions {
                        language: self.language,
                        family: family.clone(),
                        wrap: self.word_wrap,
                        matches: &matches,
                        active: active_match.as_ref(),
                        jump: jump.clone(),
                        accent,
                        dark: ui.visuals().dark_mode,
                    };
                    let live_response = live::show(
                        ui,
                        document,
                        &mut self.live,
                        edit_id,
                        font_size,
                        &options,
                        &mut self.images,
                    );
                    response.changed |= live_response.changed;
                    response.composing = live_response.composing;
                    if live_response.open_link.is_some() {
                        response.open_link = live_response.open_link;
                    }
                    if let Some(error) = live_response.error {
                        self.record_error(error, &mut response);
                    }
                }
            }
            EditorMode::Source => {
                if self.line_numbers && self.line_number_cache.0 != self.generation {
                    self.line_number_cache = (self.generation, self.source.split('\n').count());
                }
                let gutter_width = if self.line_numbers {
                    self.line_number_cache.1.to_string().len() as f32 * font_size * 0.65 + 16.0
                } else {
                    0.0
                };
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .max_height(viewport_height)
                    .id_salt(("source-scroll", self.id))
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            let gutter = if self.line_numbers {
                                Some(
                                    ui.allocate_exact_size(
                                        egui::vec2(gutter_width, 0.0),
                                        egui::Sense::hover(),
                                    )
                                    .0,
                                )
                            } else {
                                None
                            };
                            let cache = &mut self.source_layout;
                            let syntax_language = &self.syntax_language;
                            let wrap = self.word_wrap;
                            let mut layouter =
                                |ui: &egui::Ui, buffer: &dyn TextBuffer, width: f32| {
                                    cache.layout(
                                        ui,
                                        buffer.as_str(),
                                        &syntax::LayoutOptions {
                                            language: syntax_language,
                                            size: font_size,
                                            width,
                                            wrap,
                                            family: &family,
                                            matches: &matches,
                                            active: active_match.as_ref(),
                                            accent,
                                        },
                                    )
                                };
                            if let Some(mut native) =
                                egui::text_edit::TextEditState::load(ui.ctx(), edit_id)
                            {
                                native.clear_undoer();
                                native.store(ui.ctx(), edit_id);
                            }
                            let mut output = {
                                let mut adapter = source_history::Buffer {
                                    source: &mut self.source,
                                    edits: &mut source_edits,
                                };
                                egui::TextEdit::multiline(&mut adapter)
                                    .id(edit_id)
                                    .font(FontId::new(font_size, family.clone()))
                                    .code_editor()
                                    .desired_width(ui.available_width().max(400.0))
                                    .desired_rows(30)
                                    .min_size(egui::vec2(0.0, viewport_height))
                                    .frame(false)
                                    .layouter(&mut layouter)
                                    .show(ui)
                            };
                            // Native snapshots are transient within this widget call;
                            // persistent Source history contains only edit fragments.
                            output.state.clear_undoer();
                            output.state.clone().store(ui.ctx(), edit_id);
                            emoji::paint(
                                ui,
                                &output.galley,
                                output.galley_pos,
                                &self.source,
                                font_size,
                            );
                            if let Some(cursor) = output.cursor_range {
                                if self.selection.end != cursor.primary.index {
                                    self.navigation_offset = cursor.primary.index;
                                }
                                self.selection = cursor.secondary.index..cursor.primary.index;
                            }
                            if ui.input(|input| {
                                input.raw_scroll_delta.y != 0.0
                                    && input.pointer.hover_pos().is_some_and(|position| {
                                        ui.clip_rect().contains(position)
                                            && output.response.rect.contains(position)
                                    })
                            }) {
                                self.navigation_offset = output
                                    .galley
                                    .cursor_from_pos(
                                        egui::pos2(
                                            output.galley_pos.x,
                                            ui.clip_rect().top() + font_size,
                                        ) - output.galley_pos,
                                    )
                                    .index;
                            }
                            response.changed |= output.response.changed();
                            raw_has_focus = output.response.has_focus();
                            if let Some(gutter) = gutter {
                                paint_line_numbers(ui, &output, gutter, font_size);
                            }
                            if let Some(range) = &jump {
                                let rect = output
                                    .galley
                                    .pos_from_cursor(egui::text::CCursor::new(range.start))
                                    .translate(output.galley_pos.to_vec2());
                                ui.scroll_to_rect(rect, Some(egui::Align::Center));
                            }
                        });
                    });
            }
            EditorMode::Hex => {
                ui.label(tr(
                    "Hexadecimal bytes · pairs 00–FF · whitespace is ignored",
                ));
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .max_height(viewport_height)
                    .id_salt(("hex-scroll", self.id))
                    .show(ui, |ui| {
                        let mut layouter = |ui: &egui::Ui, buffer: &dyn TextBuffer, _width: f32| {
                            let mut job = LayoutJob::default();
                            job.wrap.max_width = f32::INFINITY;
                            job.append(
                                buffer.as_str(),
                                0.0,
                                TextFormat::simple(
                                    FontId::monospace(font_size),
                                    ui.visuals().text_color(),
                                ),
                            );
                            ui.fonts_mut(|fonts| {
                                fonts.layout_job(search::highlight(
                                    job,
                                    &matches,
                                    active_match.as_ref(),
                                    accent,
                                ))
                            })
                        };
                        let output = egui::TextEdit::multiline(&mut self.hex)
                            .id(edit_id)
                            .font(FontId::monospace(font_size))
                            .code_editor()
                            .desired_width(ui.available_width())
                            .desired_rows(30)
                            .min_size(egui::vec2(0.0, viewport_height))
                            .frame(false)
                            .layouter(&mut layouter)
                            .show(ui);
                        response.changed |= output.response.changed();
                        raw_has_focus = output.response.has_focus();
                        if let Some(range) = &jump {
                            let rect = output
                                .galley
                                .pos_from_cursor(egui::text::CCursor::new(range.start))
                                .translate(output.galley_pos.to_vec2());
                            ui.scroll_to_rect(rect, Some(egui::Align::Center));
                        }
                    });
                if let Err(error) = decode_hex(&self.hex) {
                    ui.colored_label(Color32::from_gray(175), error.to_string());
                }
            }
        }
        if self.mode == EditorMode::Source {
            self.source_history.record(
                source_edits,
                source_before_selection,
                self.selection.clone(),
            );
        }
        if self.mode != EditorMode::Formatted {
            if raw_ime.1 {
                if self.mode == EditorMode::Source {
                    self.source_history
                        .commit_composition(self.selection.clone());
                }
                self.raw_composition = None;
            } else if self.raw_composition.is_some() && (raw_ime.2 || !raw_has_focus) {
                response.changed |= self.cancel_composition();
            }
            response.composing = self.is_composing();
        }
        if response.changed {
            self.generation = self.generation.wrapping_add(1);
        }
        response
    }

    fn visible_text(&self) -> String {
        match self.mode {
            EditorMode::Formatted => self
                .document
                .as_ref()
                .map_or_else(String::new, Document::text),
            EditorMode::Source => self.source.clone(),
            EditorMode::Hex => self.hex.clone(),
            EditorMode::Image => String::new(),
        }
    }

    fn record_error(&mut self, error: String, response: &mut EditorResponse) {
        response.error = Some(error);
    }

    fn find_ui(&mut self, ui: &mut egui::Ui, response: &mut EditorResponse) {
        let lang = self.language;
        let tr = |key| lang.text(key);
        let mut replace = false;
        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .corner_radius(10)
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                let query_id = egui::Id::new(("find-query", self.id));
                if self.search.focus {
                    ui.memory_mut(|memory| memory.request_focus(query_id));
                    self.search.focus = false;
                }
                let button_font = egui::TextStyle::Button.resolve(ui.style());
                let toggle_width = ui
                    .fonts_mut(|fonts| {
                        fonts
                            .layout_no_wrap("Aa".into(), button_font, ui.visuals().text_color())
                            .size()
                            .x
                    })
                    .max(20.0)
                    + ui.spacing().button_padding.x * 2.0;
                let spacing = ui.spacing().item_spacing.x;
                let controls_width = toggle_width * 2.0 + 32.0 * 3.0 + spacing * 4.0;
                let input_width = (ui.available_width() - controls_width - spacing).max(40.0);
                egui::ScrollArea::horizontal()
                    .id_salt(("find-controls-scroll", self.id))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        egui::Grid::new(("find-rows", self.id))
                            .num_columns(2)
                            .spacing([spacing, 8.0])
                            .show(ui, |ui| {
                                // Consume Enter before the single-line field can surrender focus.
                                if ui.memory(|memory| memory.has_focus(query_id)) {
                                    if ui.input_mut(|input| {
                                        input.consume_key(Modifiers::SHIFT, Key::Enter)
                                    }) {
                                        self.search.navigate(false);
                                    } else if ui.input_mut(|input| {
                                        input.consume_key(Modifiers::NONE, Key::Enter)
                                    }) {
                                        self.search.navigate(true);
                                    }
                                }
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.find)
                                        .hint_text(tr("Найти…"))
                                        .id(query_id)
                                        .desired_width((input_width - 20.0).max(1.0))
                                        .min_size(egui::vec2(0.0, 36.0))
                                        .margin(egui::Margin::symmetric(10, 8)),
                                );
                                ui.allocate_ui_with_layout(
                                    egui::vec2(controls_width, 36.0),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .add_sized(
                                                [toggle_width, 36.0],
                                                egui::Button::new("Aa")
                                                    .selected(self.search.options.case_sensitive),
                                            )
                                            .on_hover_text(tr("Учитывать регистр"))
                                            .clicked()
                                        {
                                            self.search.options.case_sensitive =
                                                !self.search.options.case_sensitive;
                                            self.search.focus = true;
                                        }
                                        if ui
                                            .add_sized(
                                                [toggle_width, 36.0],
                                                egui::Button::new(".*")
                                                    .selected(self.search.options.regex),
                                            )
                                            .on_hover_text(tr("Регулярное выражение"))
                                            .clicked()
                                        {
                                            self.search.options.regex = !self.search.options.regex;
                                            self.search.focus = true;
                                        }
                                        if svg::button(
                                            ui,
                                            tr("Предыдущее совпадение"),
                                            include_bytes!("../../../assets/lucide/arrow-up.svg"),
                                        )
                                        .clicked()
                                        {
                                            self.search.navigate(false);
                                            self.search.focus = true;
                                        }
                                        if svg::button(
                                            ui,
                                            tr("Следующее совпадение"),
                                            include_bytes!("../../../assets/lucide/arrow-down.svg"),
                                        )
                                        .clicked()
                                        {
                                            self.search.navigate(true);
                                            self.search.focus = true;
                                        }
                                        if svg::button(
                                            ui,
                                            tr("Закрыть поиск · Esc"),
                                            include_bytes!("../../../assets/lucide/x.svg"),
                                        )
                                        .clicked()
                                        {
                                            self.show_find = false;
                                        }
                                    },
                                );
                                ui.end_row();
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.replacement)
                                        .hint_text(tr("Заменить на…"))
                                        .desired_width((input_width - 20.0).max(1.0))
                                        .min_size(egui::vec2(0.0, 36.0))
                                        .margin(egui::Margin::symmetric(10, 8)),
                                );
                                replace = ui
                                    .add_enabled_ui(
                                        !self.is_composing() && self.search.error.is_none(),
                                        |ui| {
                                            ui.add_sized(
                                                [controls_width, 36.0],
                                                egui::Button::new(tr("Заменить все")),
                                            )
                                        },
                                    )
                                    .inner
                                    .clicked();
                                ui.end_row();
                            });
                    });
                let count = self.search.matches.len();
                ui.label(format!(
                    "{}/{}",
                    if count == 0 {
                        0
                    } else {
                        self.search.current + 1
                    },
                    count
                ));
                if let Some(error) = &self.search.error {
                    ui.label(error);
                }
            });
        if replace {
            let text = match self.mode {
                EditorMode::Formatted => self
                    .document
                    .as_ref()
                    .map_or_else(String::new, Document::to_markdown),
                EditorMode::Source => self.source.clone(),
                EditorMode::Hex => self.hex.clone(),
                EditorMode::Image => String::new(),
            };
            if let Some(next) = self.search.replace(&text, &self.replacement) {
                if next != text {
                    let result = match self.mode {
                        EditorMode::Formatted => self
                            .document
                            .as_mut()
                            .ok_or(EditorError::InvalidUtf8)
                            .and_then(|document| {
                                document
                                    .replace_source_range(0..text.chars().count(), &next)
                                    .map_err(|error| EditorError::Document(error.to_string()))
                            }),
                        EditorMode::Image => Ok(()),
                        EditorMode::Source => {
                            let edits = source_history::difference(&self.source, &next);
                            self.source_history.record(
                                edits,
                                self.selection.clone(),
                                self.selection.clone(),
                            );
                            self.source = next;
                            Ok(())
                        }
                        EditorMode::Hex => {
                            self.hex = next;
                            Ok(())
                        }
                    };
                    match result {
                        Ok(()) => {
                            response.changed = true;
                            self.generation = self.generation.wrapping_add(1);
                        }
                        Err(error) => self.record_error(error.to_string(), response),
                    }
                }
            }
        }
    }
}

fn paint_line_numbers(
    ui: &egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    gutter: egui::Rect,
    size: f32,
) {
    let clip = ui.clip_rect();
    let mut line = 1;
    let mut start = true;
    for row in &output.galley.rows {
        let rect = row.rect().translate(output.galley_pos.to_vec2());
        if rect.top() > clip.bottom() {
            break;
        }
        if start && rect.bottom() >= clip.top() {
            ui.painter().text(
                egui::pos2(gutter.right() - 8.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                line.to_string(),
                FontId::monospace(size),
                ui.visuals().weak_text_color(),
            );
        }
        start = row.ends_with_newline;
        if start {
            line += 1;
        }
    }
}

pub fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "tif" | "tiff" | "svg"
            )
        })
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut result = String::new();
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            result.push(if index % 16 == 0 { '\n' } else { ' ' });
        }
        use std::fmt::Write;
        let _ = write!(result, "{byte:02X}");
    }
    result
}

fn decode_hex(text: &str) -> Result<Vec<u8>, EditorError> {
    let digits: Vec<_> = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    if digits.len() % 2 != 0 {
        return Err(EditorError::InvalidHex(
            "an incomplete byte needs two digits".into(),
        ));
    }
    digits
        .chunks_exact(2)
        .enumerate()
        .map(|(index, pair)| {
            let hi = pair[0].to_digit(16);
            let lo = pair[1].to_digit(16);
            match (hi, lo) {
                (Some(hi), Some(lo)) if pair[0].is_ascii() && pair[1].is_ascii() => {
                    Ok(((hi << 4) | lo) as u8)
                }
                _ => Err(EditorError::InvalidHex(format!(
                    "invalid byte at position {}",
                    index + 1
                ))),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_round_trip_preserves_every_byte() {
        let bytes: Vec<u8> = (0..=255).collect();
        let buffer = EditorBuffer::from_bytes(&bytes);
        assert_eq!(buffer.mode(), EditorMode::Hex);
        assert_eq!(buffer.bytes().unwrap(), bytes);
    }

    #[test]
    fn invalid_hex_rejects_save_and_mode_switch_without_losing_edit() {
        let mut buffer = EditorBuffer::from_bytes(&[0xff, 0x00]);
        buffer.hex = "FF 0".into();
        assert!(buffer.bytes().is_err());
        assert!(buffer.set_mode(EditorMode::Source).is_err());
        assert_eq!(buffer.mode(), EditorMode::Hex);
        assert_eq!(buffer.hex, "FF 0");
    }

    #[test]
    fn binary_cannot_be_decoded_lossily() {
        let mut buffer = EditorBuffer::from_bytes(&[0xff, 0xfe, 0x61]);
        assert!(buffer.set_mode(EditorMode::Formatted).is_err());
        assert_eq!(buffer.bytes().unwrap(), [0xff, 0xfe, 0x61]);
    }

    #[test]
    fn utf8_mode_round_trip_preserves_markdown_bytes() {
        let input = "# Привет\r\n\r\n**мир**  \r\n";
        let mut buffer = EditorBuffer::from_bytes(input.as_bytes());
        for mode in [EditorMode::Source, EditorMode::Hex, EditorMode::Formatted] {
            buffer.set_mode(mode).unwrap();
            assert_eq!(buffer.bytes().unwrap(), input.as_bytes());
        }
    }

    #[test]
    fn source_find_reports_scalar_ranges_and_replaces_unicode() {
        let mut buffer = EditorBuffer::from_bytes("я🙂я".as_bytes());
        buffer.set_mode(EditorMode::Source).unwrap();
        assert_eq!(buffer.find_all("я"), [0..1, 2..3]);
        assert_eq!(buffer.replace_all("я", "ёж").unwrap(), 2);
        assert_eq!(buffer.bytes().unwrap(), "ёж🙂ёж".as_bytes());
    }

    #[test]
    fn lexical_layout_preserves_all_unicode_and_punctuation() {
        let source = "fn имя() { let x = \"🙂\\\"\"; // café\n}\n";
        assert_eq!(
            syntax::layout(&egui::Context::default(), source, "rs", 16.0, true).text,
            source
        );
    }

    #[test]
    fn formatting_uses_semantics_and_preserves_model_undo() {
        let mut buffer = EditorBuffer::from_bytes("Привет мир\n".as_bytes());
        buffer.selection = 7..10;
        buffer.toggle_mark(MarkKind::Bold).unwrap();
        assert_eq!(buffer.bytes().unwrap(), "Привет **мир**\n".as_bytes());
        assert!(buffer.undo());
        assert_eq!(buffer.bytes().unwrap(), "Привет мир\n".as_bytes());
        assert!(buffer.redo());
        assert_eq!(buffer.document().unwrap().text(), "Привет мир");
    }

    #[test]
    fn replace_all_failure_in_raw_block_does_not_partially_change_other_blocks() {
        let source = "[link](x) good\n\ngood\n";
        let mut buffer = EditorBuffer::from_bytes(source.as_bytes());
        assert!(buffer.replace_all("good", "changed").is_err());
        assert_eq!(buffer.bytes().unwrap(), source.as_bytes());
    }

    #[test]
    fn only_markdown_paths_allow_formatted_mode() {
        for path in [
            "code.rs",
            "plain.txt",
            "config.json",
            "no_extension",
            "note.markdown",
        ] {
            let mut buffer = EditorBuffer::for_path(Path::new(path), b"# This is text\n");
            assert_eq!(buffer.mode(), EditorMode::Source);
            assert!(matches!(
                buffer.set_mode(EditorMode::Formatted),
                Err(EditorError::FormattingNotAllowed)
            ));
            assert_eq!(buffer.bytes().unwrap(), b"# This is text\n");
        }
        for path in ["note.md", "note.MD"] {
            let buffer = EditorBuffer::for_path(Path::new(path), b"# Heading\n");
            assert_eq!(buffer.mode(), EditorMode::Formatted);
        }
    }

    #[test]
    fn non_markdown_cannot_regain_live_mode_after_hex_roundtrip() {
        let mut buffer = EditorBuffer::for_path(Path::new("code.rs"), b"fn main() {}\n");
        buffer.set_mode(EditorMode::Hex).unwrap();
        buffer.set_mode(EditorMode::Source).unwrap();
        assert!(buffer.set_mode(EditorMode::Formatted).is_err());
        assert_eq!(buffer.bytes().unwrap(), b"fn main() {}\n");
    }

    #[test]
    fn cancelling_source_ime_restores_accepted_text() {
        let mut buffer = EditorBuffer::for_path(Path::new("text.txt"), b"accepted");
        buffer.raw_composition = Some((EditorMode::Source, buffer.source.clone()));
        buffer.source.push_str("候補");
        assert!(buffer.is_composing());
        assert!(buffer.set_mode(EditorMode::Hex).is_err());
        assert!(buffer.cancel_composition());
        assert!(!buffer.is_composing());
        assert_eq!(buffer.bytes().unwrap(), b"accepted");
    }
}

#[cfg(test)]
mod image_tests {
    use super::*;
    #[test]
    fn images_start_in_native_view_and_hex_is_only_built_on_demand() {
        let bytes = b"\x89PNG\r\n\x1a\ninvalid image";
        let mut buffer = EditorBuffer::for_path(Path::new("photo.png"), bytes);
        assert_eq!(buffer.mode(), EditorMode::Image);
        assert!(buffer.hex.is_empty());
        assert_eq!(buffer.bytes().unwrap(), bytes);
        buffer.set_mode(EditorMode::Hex).unwrap();
        assert!(!buffer.hex.is_empty());
        assert_eq!(buffer.bytes().unwrap(), bytes);
        buffer.set_mode(EditorMode::Image).unwrap();
        assert_eq!(buffer.bytes().unwrap(), bytes);
    }
}

#[cfg(test)]
mod find_focus_tests {
    use super::*;
    #[test]
    fn successive_query_letters_and_enter_keep_query_focus_in_all_text_modes() {
        for mode in [EditorMode::Formatted, EditorMode::Source, EditorMode::Hex] {
            let ctx = egui::Context::default();
            let mut buffer = EditorBuffer::from_bytes(b"abc abc\n");
            buffer.set_mode(mode).unwrap();
            let before = buffer.bytes().unwrap();
            buffer.open_find();
            let query = egui::Id::new(("find-query", buffer.id));
            let letters = if mode == EditorMode::Hex {
                ["6", "1"]
            } else {
                ["a", "b"]
            };
            let mut wanted = String::new();
            for frame in 0..4 {
                let mut input = egui::RawInput {
                    focused: true,
                    time: Some(frame as f64),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 650.0),
                    )),
                    ..Default::default()
                };
                if frame == 1 || frame == 2 {
                    let letter = letters[frame - 1];
                    input.events.push(egui::Event::Text(letter.into()));
                    wanted.push_str(letter);
                }
                if frame == 3 {
                    input.events.push(egui::Event::Key {
                        key: Key::Enter,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    });
                }
                let _ = ctx.run(input, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        buffer.ui(ui);
                    });
                });
                assert_eq!(
                    ctx.memory(|memory| memory.focused()),
                    Some(query),
                    "mode {mode:?}, frame {frame}"
                );
                assert_eq!(buffer.find, wanted);
                assert_eq!(buffer.bytes().unwrap(), before);
            }
        }
    }
}
