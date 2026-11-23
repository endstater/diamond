// SPDX-License-Identifier: GPL-3.0-only
//! One editable projection with real source syntax revealed in the caret block.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use diamond_document::{BlockKind, Document, Mark, MarkKind};
use egui::text::{CCursor, CCursorRange, LayoutJob};
use egui::{Color32, FontId, Key, Modifiers, Stroke, TextBuffer, TextFormat};

pub(super) struct ViewOptions<'a> {
    pub language: diamond_core::Language,
    pub family: egui::FontFamily,
    pub wrap: bool,
    pub matches: &'a [Range<usize>],
    pub active: Option<&'a Range<usize>>,
    pub jump: Option<Range<usize>>,
    pub accent: Color32,
    pub dark: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SourceSelection {
    primary: usize,
    secondary: usize,
}

#[derive(Default)]
pub(super) struct LiveState {
    selection: SourceSelection,
    navigation_offset: usize,
    reveal: Option<Range<usize>>,
    last_projection: String,
    composing: bool,
    composition_checkpoint: Option<Document>,
    composition_selection: SourceSelection,
    reset_native_state: bool,
    pending_jump: Option<usize>,
}

impl LiveState {
    pub fn preview_offset(&self) -> usize {
        self.navigation_offset
    }
    pub fn cursor_offset(&self) -> usize {
        self.selection.primary
    }
    pub fn is_composing(&self) -> bool {
        self.composing
    }

    pub fn cancel_composition(&mut self, document: &mut Document) -> bool {
        let was_composing = self.composing;
        if let Some(before) = self.composition_checkpoint.take() {
            *document = before;
            self.selection = self.composition_selection;
        }
        self.composing = false;
        self.reveal = None;
        self.reset_native_state |= was_composing;
        was_composing
    }

    pub fn reset(&mut self) {
        *self = Self {
            reset_native_state: true,
            ..Self::default()
        };
    }
}

#[derive(Default)]
pub(super) struct LiveResponse {
    pub changed: bool,
    pub composing: bool,
    pub open_link: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone)]
struct ProjectedBlock {
    view_range: Range<usize>,
    kind: BlockKind,
    marks: Vec<Mark>,
    links: Vec<diamond_document::Link>,
    index: usize,
    indent: usize,
    quote_depth: usize,
    math: Vec<diamond_document::MathSpan>,
    footnotes: Vec<diamond_document::FootnoteRef>,
    content_start: usize,
    header: Option<Range<usize>>,
    gap: Option<Range<usize>>,
    source_range: Range<usize>,
    text: String,
    language: String,
    order: u64,
    table: Option<diamond_document::Table>,
    image: Option<diamond_document::Image>,
    image_height: f32,
    revealed: bool,
    before: Vec<usize>,
    after: Vec<usize>,
}

#[derive(Clone)]
struct Projection {
    text: String,
    source: String,
    /// Every displayed scalar boundary has an explicit source scalar anchor.
    to_source: Vec<usize>,
    blocks: Vec<ProjectedBlock>,
}

impl Projection {
    fn build(document: &Document, reveal: Option<&Range<usize>>) -> Self {
        let map = document.source_map();
        let chars: Vec<_> = map.source.chars().collect();
        let mut result = Self {
            text: String::new(),
            source: map.source.clone(),
            to_source: vec![0],
            blocks: Vec::new(),
        };
        let mut consumed = 0;
        for (index, block_map) in map.blocks.iter().enumerate() {
            let Some(block) = document.blocks().get(index) else {
                continue;
            };
            let range = &block_map.source_range;
            result.append_source(&chars, consumed..range.start);
            let view_start = result.scalar_len();
            let mut header = None;
            let mut gap = None;
            if block.kind == BlockKind::Code || block.kind == BlockKind::Footnote {
                if !result.text.ends_with("\n\n") {
                    let start = result.scalar_len();
                    result.append_virtual("\n", range.start);
                    gap = Some(start..result.scalar_len());
                }
                let start = result.scalar_len();
                let title = if let Some(label) = &block.footnote_label {
                    format!("[{}] {label}", block.footnote_number.unwrap_or(0))
                } else {
                    block
                        .language()
                        .split_whitespace()
                        .next()
                        .unwrap_or("text")
                        .to_owned()
                };
                result.append_virtual(&format!("{title}\n"), range.start);
                header = Some(start..result.scalar_len());
            }
            let content_start = result.scalar_len();
            let revealed = !block_map.is_exact
                || block.kind == BlockKind::Raw
                || reveal.is_some_and(|active| ranges_touch(active, range));
            if !revealed && block.table.is_some() {
                result.append_virtual(" \n", range.start);
                if let Some(last) = result.to_source.last_mut() {
                    *last = range.end;
                }
            } else if revealed {
                result.append_source(&chars, range.clone());
            } else {
                let boundaries = &block_map.semantic_to_source_before;
                let right = &block_map.semantic_to_source_after;
                let visible: Vec<_> = block.text.chars().collect();
                // The mapper explicitly declines ambiguous entity expansions.
                // Keep source editable instead of guessing destructive offsets.
                if boundaries.len() != visible.len() + 1 || right.len() != boundaries.len() {
                    result.append_source(&chars, range.clone());
                } else {
                    if let Some(last) = result.to_source.last_mut() {
                        *last = right[0];
                    }
                    for (local, character) in visible.iter().enumerate() {
                        result.text.push(*character);
                        result.to_source.push(boundaries[local + 1]);
                    }
                    // Parser spans include the final line ending; it is layout,
                    // not Markdown syntax, so keep a mapped visible newline.
                    let ending_start =
                        if chars.get(range.end.wrapping_sub(2)..range.end) == Some(&['\r', '\n']) {
                            range.end - 2
                        } else if range.end > range.start && chars[range.end - 1] == '\n' {
                            range.end - 1
                        } else {
                            range.end
                        };
                    if ending_start < range.end && !block.text.ends_with('\n') {
                        result.append_source(&chars, ending_start..range.end);
                    }
                }
            }
            let view_end = result.scalar_len();
            if block.kind == BlockKind::Code {
                result.append_virtual("\n", range.end);
            }
            result.blocks.push(ProjectedBlock {
                view_range: view_start..view_end,
                kind: block.kind.clone(),
                marks: block.marks.clone(),
                links: block.links.clone(),
                index,
                indent: block.indent,
                quote_depth: block.quote_depth,
                math: block.math.clone(),
                footnotes: block.footnotes.clone(),
                content_start,
                header,
                gap,
                source_range: range.clone(),
                text: block.text.clone(),
                language: block.language().into(),
                order: block.order(),
                table: block.table.clone(),
                image: block.image.clone(),
                image_height: 88.0,
                revealed,
                before: block_map.semantic_to_source_before.clone(),
                after: block_map.semantic_to_source_after.clone(),
            });
            consumed = range.end;
        }
        result.append_source(&chars, consumed..chars.len());
        result
    }

    fn scalar_len(&self) -> usize {
        self.to_source.len() - 1
    }

    fn append_virtual(&mut self, text: &str, source: usize) {
        for c in text.chars() {
            self.text.push(c);
            self.to_source.push(source);
        }
    }

    fn append_source(&mut self, source: &[char], range: Range<usize>) {
        if range.start > range.end || range.end > source.len() {
            return;
        }
        if let Some(last) = self.to_source.last_mut() {
            *last = range.start;
        }
        let mut position = range.start;
        while position < range.end {
            let crlf = source[position] == '\r'
                && source.get(position + 1) == Some(&'\n')
                && position + 1 < range.end;
            self.text.push(if crlf { '\n' } else { source[position] });
            position += if crlf { 2 } else { 1 };
            self.to_source.push(position);
        }
    }

    fn source_at(&self, view: usize) -> usize {
        self.to_source[view.min(self.scalar_len())]
    }

    fn view_at(&self, source: usize) -> usize {
        if let Some(block) = self
            .blocks
            .iter()
            .find(|block| block.header.is_some() && block.source_range.start == source)
        {
            return block.content_start;
        }
        let upper = self
            .to_source
            .partition_point(|position| *position < source);
        if upper == 0 {
            return 0;
        }
        if upper >= self.to_source.len() {
            return self.scalar_len();
        }
        if source - self.to_source[upper - 1] <= self.to_source[upper] - source {
            upper - 1
        } else {
            upper
        }
    }

    fn layout(
        &self,
        ui: &egui::Ui,
        font_size: f32,
        width: f32,
        color: Color32,
        options: &ViewOptions<'_>,
    ) -> LayoutJob {
        let mut job = LayoutJob::default();
        job.wrap.max_width = if options.wrap { width } else { f32::INFINITY };
        let code_spans: Vec<_> = self
            .blocks
            .iter()
            .map(|block| {
                if block.kind == BlockKind::Code {
                    Some(super::syntax::spans(
                        ui.ctx(),
                        &block.text,
                        &block.language,
                        options.dark,
                    ))
                } else {
                    None
                }
            })
            .collect();
        let mut block_index = 0;
        for (view, character) in self.text.chars().enumerate() {
            while self
                .blocks
                .get(block_index)
                .is_some_and(|block| view >= block.view_range.end)
            {
                block_index += 1;
            }
            let mut leading_space = 0.0;
            let mut format =
                TextFormat::simple(FontId::new(font_size, options.family.clone()), color);
            if let Some(block) = self
                .blocks
                .get(block_index)
                .filter(|block| block.view_range.contains(&view))
            {
                let semantic = if block.revealed {
                    let source = self.source_at(view);
                    let next = block.after.partition_point(|at| *at <= source);
                    next.checked_sub(1).filter(|index| {
                        *index + 1 < block.before.len() && source < block.before[*index + 1]
                    })
                } else {
                    view.checked_sub(block.content_start)
                };
                match block.kind {
                    BlockKind::Heading(level) => {
                        format.font_id.size *= match level {
                            1 => 1.8,
                            2 => 1.5,
                            _ => 1.2,
                        }
                    }
                    BlockKind::Quote => {
                        format.color = Color32::from_gray(if options.dark { 155 } else { 100 });
                    }
                    BlockKind::Code => {
                        format.font_id = FontId::monospace(font_size * 0.94);
                        format.background = Color32::from_gray(if options.dark { 28 } else { 237 });
                    }
                    BlockKind::Task(done) => {
                        format.line_height = Some((font_size * 1.5).max(26.0));
                        if done {
                            format.strikethrough = Stroke::new(1.0, Color32::from_gray(135));
                            format.color = Color32::from_gray(if options.dark { 140 } else { 105 });
                        }
                    }
                    _ => {}
                }
                if !block.revealed {
                    if view == block.content_start {
                        leading_space += container_indent(block, font_size);
                    }
                    if block.image.is_some() {
                        format.line_height = Some(block.image_height + font_size * 1.8);
                        format.valign = egui::Align::Max;
                    }
                }
                if block.revealed && semantic.is_none() && character != '\n' {
                    format.color = Color32::from_gray(if options.dark { 115 } else { 135 });
                }
                if let Some(position) = semantic {
                    if let Some(runs) = &code_spans[block_index] {
                        let at = runs.partition_point(|run| run.scalars.end <= position);
                        if let Some(run) =
                            runs.get(at).filter(|run| run.scalars.contains(&position))
                        {
                            format.color = run.color;
                            format.italics = run.italic;
                            if run.bold {
                                format.extra_letter_spacing = 0.25;
                            }
                        }
                    }
                    if block
                        .links
                        .iter()
                        .any(|link| link.range.contains(&position))
                    {
                        format.color = options.accent;
                        format.underline = Stroke::new(1.0, options.accent.gamma_multiply(0.6));
                    }
                    for mark in &block.marks {
                        if mark.range.contains(&position) {
                            match mark.kind {
                                MarkKind::Bold => {
                                    format.color =
                                        Color32::from_gray(if options.dark { 240 } else { 12 });
                                    format.extra_letter_spacing = 0.35;
                                }
                                MarkKind::Italic => format.italics = true,
                                MarkKind::Strike => {
                                    format.strikethrough = Stroke::new(1.0, Color32::from_gray(150))
                                }
                                MarkKind::Code => {
                                    format.font_id = FontId::monospace(font_size * 0.94);
                                    format.background =
                                        Color32::from_gray(if options.dark { 36 } else { 225 });
                                }
                            }
                        }
                    }
                }
            }
            if let Some(block) = self.blocks.get(block_index) {
                if block
                    .gap
                    .as_ref()
                    .is_some_and(|range| range.contains(&view))
                {
                    format.font_id.size = 1.0;
                    format.line_height = Some(12.0);
                    format.color = Color32::TRANSPARENT;
                    format.background = Color32::TRANSPARENT;
                } else if block
                    .header
                    .as_ref()
                    .is_some_and(|range| range.contains(&view))
                {
                    format = TextFormat::simple(
                        FontId::new(font_size * 0.76, options.family.clone()),
                        ui.visuals().weak_text_color(),
                    );
                    format.line_height = Some(font_size * 1.65);
                } else if !block.revealed
                    && let Some(position) = view.checked_sub(block.content_start)
                {
                    if let Some(reference) = block
                        .footnotes
                        .iter()
                        .find(|reference| reference.range.contains(&position))
                    {
                        let width = ui.fonts_mut(|fonts| {
                            fonts
                                .layout_no_wrap(
                                    reference.number.to_string(),
                                    FontId::monospace(font_size * 0.72),
                                    options.accent,
                                )
                                .size()
                                .x
                        });
                        format.font_id.size = 1.0;
                        format.extra_letter_spacing = width + 3.0;
                        format.line_height = Some(font_size * 1.3);
                        format.color = Color32::TRANSPARENT;
                    }
                    let at = block
                        .math
                        .partition_point(|math| math.range.end <= position);
                    if let Some(math) = block
                        .math
                        .get(at)
                        .filter(|math| math.range.contains(&position))
                    {
                        let formula = super::math::formula(ui.ctx(), &math.latex, math.display);
                        let size = formula.size * (font_size / 16.0);
                        let size = fit_formula(size, width - 8.0);
                        let unit = ui
                            .fonts_mut(|fonts| {
                                fonts
                                    .layout_no_wrap(
                                        "◇".into(),
                                        FontId::monospace(16.0),
                                        Color32::WHITE,
                                    )
                                    .size()
                                    .x
                            })
                            .max(1.0);
                        format.font_id = FontId::monospace(size.x.max(1.0) * 16.0 / unit);
                        format.color = Color32::TRANSPARENT;
                        format.extra_letter_spacing = 0.0;
                        format.line_height = Some(size.y.max(font_size * 1.3) + 6.0);
                        if block.kind == BlockKind::Math {
                            leading_space += ((width - size.x) / 2.0).max(0.0);
                        }
                    }
                }
            }
            let source = self.source_at(view);
            let matched = options.matches.partition_point(|range| range.end <= source);
            if options
                .matches
                .get(matched)
                .is_some_and(|range| range.contains(&source))
            {
                let selected = options.active.is_some_and(|range| range.contains(&source));
                format.background =
                    options
                        .accent
                        .gamma_multiply(if selected { 0.48 } else { 0.23 });
            }
            let mut encoded = [0; 4];
            if let Some(block) = self
                .blocks
                .get(block_index)
                .filter(|block| !block.revealed && block.table.is_some())
            {
                let rows = block.table.as_ref().map_or(1, |table| {
                    table
                        .cells
                        .iter()
                        .map(|cell| cell.row + 1)
                        .max()
                        .unwrap_or(1)
                });
                format.font_id.size = 1.0;
                format.color = Color32::TRANSPARENT;
                leading_space = 0.0;
                format.extra_letter_spacing = 0.0;
                format.line_height = Some(rows as f32 * font_size * 1.9 + 28.0);
            }
            job.append(character.encode_utf8(&mut encoded), leading_space, format);
        }
        job
    }
}

struct TableLayout {
    columns: Vec<f32>,
}
impl TableLayout {
    fn width(&self) -> f32 {
        self.columns.iter().sum()
    }
}
fn table_layout(
    ui: &egui::Ui,
    block: &ProjectedBlock,
    size: f32,
    family: &egui::FontFamily,
) -> Option<TableLayout> {
    if block.revealed {
        return None;
    }
    let table = block.table.as_ref()?;
    let mut columns = vec![64.0_f32; table.columns.len()];
    let glyph_width = |text: &str, font_size: f32| {
        ui.fonts_mut(|fonts| {
            fonts
                .layout_no_wrap(
                    text.to_owned(),
                    FontId::new(font_size, family.clone()),
                    Color32::WHITE,
                )
                .size()
                .x
        })
    };
    let widths: Vec<_> = table
        .cells
        .iter()
        .map(|cell| {
            let mut width = glyph_width(&cell.text, size);
            let first = block
                .math
                .partition_point(|math| math.range.end <= cell.range.start);
            for math in block.math[first..]
                .iter()
                .take_while(|math| math.range.start < cell.range.end)
            {
                width += super::math::formula(ui.ctx(), &math.latex, math.display)
                    .size
                    .x
                    * (size / 16.0)
                    + 4.0
                    - glyph_width("◇", size);
            }
            let first = block
                .footnotes
                .partition_point(|reference| reference.range.end <= cell.range.start);
            for reference in block.footnotes[first..]
                .iter()
                .take_while(|reference| reference.range.start < cell.range.end)
            {
                let reference_width = ui.fonts_mut(|fonts| {
                    fonts
                        .layout_no_wrap(
                            reference.number.to_string(),
                            FontId::monospace(size * 0.72),
                            Color32::WHITE,
                        )
                        .size()
                        .x
                });
                width += reference_width + 3.0 - glyph_width("⁎", size);
            }
            width
        })
        .collect();
    for (cell, width) in table.cells.iter().zip(&widths) {
        if let Some(column) = columns.get_mut(cell.column) {
            *column = column.max(*width + 24.0);
        }
    }
    Some(TableLayout { columns })
}

fn ranges_touch(active: &Range<usize>, block: &Range<usize>) -> bool {
    if active.is_empty() {
        block.start <= active.start && active.start <= block.end
    } else {
        active.start < block.end && block.start < active.end
    }
}

fn caret_block(document: &Document, source: usize) -> Option<Range<usize>> {
    let map = document.source_map();
    let length = map.source.chars().count();
    map.blocks
        .iter()
        .zip(document.blocks())
        .find(|(block, semantic)| {
            if source < block.source_range.start {
                return false;
            }
            let fragment = &map.source[block.source_byte_range.clone()];
            // The caret after a trailing newline belongs to the empty next line,
            // including EOF. Never fall back to the preceding block in a gap.
            let open_code = semantic.kind == BlockKind::Code && open_fence(fragment);
            source < block.source_range.end
                || (source == length
                    && source == block.source_range.end
                    && ((!fragment.ends_with('\n') && !fragment.is_empty()) || open_code))
        })
        .map(|(block, _)| block.source_range.clone())
}

fn open_fence(source: &str) -> bool {
    let mut lines = source.lines();
    let first = lines.next().unwrap_or("").trim_start();
    let Some(marker) = first.chars().next().filter(|c| matches!(c, '`' | '~')) else {
        return false;
    };
    let count = first.chars().take_while(|c| *c == marker).count();
    count >= 3
        && !lines.any(|line| {
            let line = line.trim();
            line.chars().take_while(|c| *c == marker).count() >= count
                && line.chars().all(|c| c == marker)
        })
}

fn translate(position: usize, removed: &Range<usize>, inserted: usize, after: bool) -> usize {
    if position < removed.start || (position == removed.start && !after) {
        position
    } else if position > removed.end {
        position - (removed.end - removed.start) + inserted
    } else {
        removed.start + if after { inserted } else { 0 }
    }
}

struct LiveBuffer<'a> {
    document: &'a mut Document,
    projection: Rc<RefCell<Projection>>,
    text: String,
    reveal: Option<Range<usize>>,
    error: Option<String>,
    pending_insert: Option<usize>,
    forced_selection: Option<SourceSelection>,
    changed: bool,
    checkpoint: Option<Document>,
}

impl LiveBuffer<'_> {
    fn edit(&mut self, range: Range<usize>, replacement: &str) -> bool {
        if self.error.is_some() {
            return false;
        }
        if self.checkpoint.is_none() {
            self.checkpoint = Some(self.document.clone());
        }
        let old_source = self.document.to_markdown();
        let replacement = if old_source.contains("\r\n") {
            replacement.replace("\r\n", "\n").replace('\n', "\r\n")
        } else {
            replacement.replace("\r\n", "\n")
        };
        if let Err(error) = self
            .document
            .replace_source_range(range.clone(), &replacement)
        {
            self.error = Some(error.to_string());
            return false;
        }
        let inserted = replacement.chars().count();
        self.reveal = self.reveal.as_ref().map(|active| {
            translate(active.start, &range, inserted, false)
                ..translate(active.end, &range, inserted, true)
        });
        let target = range.start + inserted;
        if let Some(block) = caret_block(self.document, target) {
            self.reveal = Some(self.reveal.as_ref().map_or(block.clone(), |current| {
                current.start.min(block.start)..current.end.max(block.end)
            }));
        }
        let mut next = Projection::build(self.document, self.reveal.as_ref());
        for block in &mut next.blocks {
            if let Some(image) = &block.image {
                if let Some(previous) = self.projection.borrow().blocks.iter().find(|old| {
                    old.image
                        .as_ref()
                        .is_some_and(|old| old.target == image.target)
                }) {
                    block.image_height = previous.image_height;
                }
            }
        }
        self.text.clone_from(&next.text);
        *self.projection.borrow_mut() = next;
        self.forced_selection = Some(SourceSelection {
            primary: target,
            secondary: target,
        });
        self.changed |= old_source != self.document.to_markdown();
        true
    }
}

impl TextBuffer for LiveBuffer<'_> {
    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<LiveBuffer<'static>>()
    }
    fn is_mutable(&self) -> bool {
        true
    }
    fn as_str(&self) -> &str {
        &self.text
    }

    fn insert_text(&mut self, text: &str, char_index: usize) -> usize {
        if self.error.is_some() {
            return 0;
        }
        let source = self
            .pending_insert
            .take()
            .unwrap_or_else(|| self.projection.borrow().source_at(char_index));
        if !self.edit(source..source, text) {
            return 0;
        }
        let target = self.forced_selection.unwrap_or_default().primary;
        self.projection
            .borrow()
            .view_at(target)
            .saturating_sub(char_index)
    }

    fn delete_char_range(&mut self, range: Range<usize>) {
        if self.error.is_some() {
            return;
        }
        let source_range = {
            let projection = self.projection.borrow();
            if range.start == 0 && range.end == projection.scalar_len() {
                0..projection.source.chars().count()
            } else {
                projection.source_at(range.start)..projection.source_at(range.end)
            }
        };
        let source_start = source_range.start;
        if self.edit(source_range, "") {
            self.pending_insert = Some(source_start);
        }
    }

    fn replace_with(&mut self, text: &str) {
        let length = self.document.to_markdown().chars().count();
        self.edit(0..length, text);
    }
}

/// Reconstruct one source transaction, rather than keeping TextEdit's separate
/// delete/insert callbacks (or each intermediate IME candidate) in history.
fn coalesce(mut before: Document, after: &Document) -> Result<Document, String> {
    let old = before.to_markdown();
    let new = after.to_markdown();
    if old == new {
        return Ok(before);
    }
    let old_chars: Vec<_> = old.chars().collect();
    let new_chars: Vec<_> = new.chars().collect();
    let prefix = old_chars
        .iter()
        .zip(&new_chars)
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = old_chars[prefix..]
        .iter()
        .rev()
        .zip(new_chars[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let replacement: String = new_chars[prefix..new_chars.len() - suffix].iter().collect();
    before
        .replace_source_range(prefix..old_chars.len() - suffix, &replacement)
        .map_err(|error| error.to_string())?;
    Ok(before)
}

pub(super) fn show(
    ui: &mut egui::Ui,
    document: &mut Document,
    state: &mut LiveState,
    id: egui::Id,
    font_size: f32,
    options: &ViewOptions<'_>,
    images: &mut super::media::Images,
) -> LiveResponse {
    let mut response = LiveResponse::default();
    let jump = state
        .pending_jump
        .take()
        .map(|source| source..source)
        .or_else(|| options.jump.clone());
    if jump.is_some() {
        ui.memory_mut(|memory| memory.request_focus(id));
    }
    if !state.composing
        && let Some(range) = &jump
    {
        state.selection = SourceSelection {
            primary: range.end,
            secondary: range.start,
        };
        state.reset_native_state = true;
    }
    let focused = ui.input(|input| input.focused) && ui.memory(|memory| memory.has_focus(id));
    let (preedit, commit, disabled) = ui.input(|input| {
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
    if focused && preedit && !state.composing {
        state.composition_checkpoint = Some(document.clone());
        state.composition_selection = state.selection;
        state.composing = true;
    }
    if focused && !state.composing {
        if ui.input_mut(|input| {
            input.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
                || input.consume_key(Modifiers::COMMAND, Key::Y)
        }) {
            response.changed |= document.redo();
        } else if ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::Z)) {
            response.changed |= document.undo();
        }
    }
    if focused
        && !state.composing
        && state.selection.primary == state.selection.secondary
        && ui.input(|input| input.key_pressed(Key::Enter) && input.modifiers == Modifiers::NONE)
        && let Some(continuation) = document.line_continuation(state.selection.primary)
        && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter))
    {
        let target = continuation.range.start + continuation.text.chars().count();
        match document.replace_source_range(continuation.range, &continuation.text) {
            Ok(()) => {
                state.selection = SourceSelection {
                    primary: target,
                    secondary: target,
                };
                state.reset_native_state = true;
                response.changed = true;
            }
            Err(error) => response.error = Some(error.to_string()),
        }
    }
    if !state.composing {
        state.reveal = if let Some(range) = &jump {
            caret_block(document, range.start)
        } else if focused {
            caret_block(document, state.selection.primary)
        } else {
            options
                .active
                .and_then(|range| caret_block(document, range.start))
        };
    }
    let mut initial = Projection::build(document, state.reveal.as_ref());
    for block in &mut initial.blocks {
        if let Some(image) = &block.image {
            block.image_height =
                images.height(&image.target, (ui.available_width() - 28.0).max(1.0));
        }
    }

    if !state.composing && (initial.text != state.last_projection || state.reset_native_state) {
        let mut native = if state.reset_native_state {
            egui::text_edit::TextEditState::default()
        } else {
            egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default()
        };
        native.cursor.set_char_range(Some(CCursorRange {
            primary: CCursor::new(initial.view_at(state.selection.primary)),
            secondary: CCursor::new(initial.view_at(state.selection.secondary)),
            h_pos: None,
        }));
        native.clear_undoer();
        native.store(ui.ctx(), id);
        state.reset_native_state = false;
    }
    let before_selection = state.selection;
    let projection = Rc::new(RefCell::new(initial));
    let layout_projection = Rc::clone(&projection);
    let mut adapter = LiveBuffer {
        text: projection.borrow().text.clone(),
        projection: Rc::clone(&projection),
        document,
        reveal: state.reveal.clone(),
        error: None,
        pending_insert: None,
        forced_selection: None,
        changed: false,
        checkpoint: None,
    };
    let mut layouter = |ui: &egui::Ui, _buffer: &dyn TextBuffer, width: f32| {
        let mut job = layout_projection.borrow().layout(
            ui,
            font_size,
            width,
            ui.visuals().text_color(),
            options,
        );
        super::emoji::decorate_job(ui.ctx(), &mut job);
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    };
    let mut table_action = None;
    let mut table_jump = None;
    let mut toggled = None;
    let mut has_focus = focused;
    let height = ui.available_height().max(font_size * 2.0);
    egui::ScrollArea::new([!options.wrap, true])
        .auto_shrink([false, false])
        .max_height(height)
        .id_salt(id.with("live-scroll"))
        .show(ui, |ui| {
            let background = ui.painter().add(egui::Shape::Noop);
            let output = egui::TextEdit::multiline(&mut adapter)
                .id(id)
                .font(FontId::new(font_size, options.family.clone()))
                .desired_width(ui.available_width())
                .desired_rows(30)
                .min_size(egui::vec2(0.0, height))
                .frame(false)
                .margin(egui::Margin {
                    left: 36,
                    right: 4,
                    top: 4,
                    bottom: 4,
                })
                .layouter(&mut layouter)
                .show(ui);
            has_focus = output.response.has_focus();
            if let Some(cursor) = output.cursor_range {
                let map = projection.borrow();
                state.selection = SourceSelection {
                    primary: map.source_at(cursor.primary.index),
                    secondary: map.source_at(cursor.secondary.index),
                };
            }
            if state.selection != before_selection {
                state.navigation_offset = state.selection.primary;
            }
            if ui.input(|input| {
                input.raw_scroll_delta.y != 0.0
                    && input.pointer.hover_pos().is_some_and(|position| {
                        ui.clip_rect().contains(position) && output.response.rect.contains(position)
                    })
            }) {
                let view = output
                    .galley
                    .cursor_from_pos(
                        egui::pos2(output.galley_pos.x, ui.clip_rect().top() + font_size)
                            - output.galley_pos,
                    )
                    .index;
                state.navigation_offset = projection.borrow().source_at(view);
            }
            if let Some(range) = &jump {
                let view = projection.borrow().view_at(range.start);
                let rect = output
                    .galley
                    .pos_from_cursor(CCursor::new(view))
                    .translate(output.galley_pos.to_vec2());
                ui.scroll_to_rect(rect, Some(egui::Align::Center));
            }
            super::emoji::paint(
                ui,
                &output.galley,
                output.galley_pos,
                &projection.borrow().text,
                font_size,
            );
            paint_structures(
                ui,
                &output,
                &projection.borrow(),
                font_size,
                options,
                background,
                images,
            );
            table_action = paint_tables(
                ui,
                &output,
                &projection.borrow(),
                font_size,
                options,
                &mut table_jump,
                &mut response.open_link,
            );
            toggled = paint_decorations(
                ui,
                &output,
                &projection.borrow(),
                font_size,
                options.language,
            );
            if let Some(pointer) = ui
                .input(|input| input.pointer.hover_pos())
                .filter(|point| output.response.rect.contains(*point))
            {
                let view = output
                    .galley
                    .cursor_from_pos(pointer - output.galley_pos)
                    .index;
                let map = projection.borrow();
                if let Some(block) = map
                    .blocks
                    .iter()
                    .find(|block| block.view_range.contains(&view))
                {
                    let source = map.source_at(view);
                    let position = if block.revealed {
                        block
                            .after
                            .partition_point(|at| *at <= source)
                            .checked_sub(1)
                            .filter(|index| {
                                *index + 1 < block.before.len() && source < block.before[*index + 1]
                            })
                    } else {
                        view.checked_sub(block.content_start)
                    };
                    if let Some(link) = position.and_then(|position| {
                        block
                            .links
                            .iter()
                            .find(|link| link.range.contains(&position))
                    }) {
                        output
                            .response
                            .clone()
                            .on_hover_text(options.language.text("Open link: Ctrl+click"));
                        if ui.input(|input| input.modifiers.command) {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            if output.response.clicked() {
                                response.open_link = Some(link.target.clone());
                            }
                        }
                    }
                }
            }
        });
    if let Some(source) = table_jump {
        state.pending_jump = Some(source);
        ui.ctx().request_repaint();
    }
    if let Some(label) = response
        .open_link
        .as_deref()
        .and_then(|target| target.strip_prefix("diamond-footnote:"))
        .map(str::to_owned)
    {
        let map = adapter.document.source_map();
        if let Some((span, _)) = map
            .blocks
            .iter()
            .zip(adapter.document.blocks())
            .find(|(_, block)| block.footnote_label.as_deref() == Some(label.as_str()))
        {
            state.pending_jump = Some(span.source_range.start);
            ui.ctx().request_repaint();
        } else {
            response.error = Some(format!("Undefined footnote: {label}"));
        }
        response.open_link = None;
    }
    if let Some(selection) = adapter.forced_selection {
        state.selection = selection;
    }
    state.reveal = adapter.reveal;
    state.last_projection.clone_from(&adapter.text);
    if let Some(error) = adapter.error {
        if let Some(before) = adapter.checkpoint.take() {
            *adapter.document = before;
        }
        state.selection = before_selection;
        state.reset_native_state = true;
        response.error = Some(error);
    } else if adapter.changed {
        let before = adapter.checkpoint.take().expect("changed edit checkpoint");
        match coalesce(before, adapter.document) {
            Ok(merged) => {
                *adapter.document = merged;
                response.changed = true;
            }
            Err(error) => response.error = Some(error),
        }
    }
    if let Some((index, row, column, action)) =
        table_action.filter(|_| !state.composing && !adapter.changed)
    {
        match adapter.document.edit_table(index, row, column, action) {
            Ok(()) => {
                response.changed = true;
                state.reset_native_state = true;
            }
            Err(error) => response.error = Some(error),
        }
    }
    if let Some(index) = toggled.filter(|_| !state.composing && !adapter.changed) {
        state.selection = before_selection;
        match adapter.document.toggle_task(index) {
            Ok(()) => {
                response.changed = true;
                state.reset_native_state = true;
            }
            Err(error) => response.error = Some(error.to_string()),
        }
    }
    if commit && state.composing {
        if let Some(checkpoint) = state.composition_checkpoint.take() {
            match coalesce(checkpoint, adapter.document) {
                Ok(merged) => *adapter.document = merged,
                Err(error) => response.error = Some(error),
            }
        }
        state.composing = false;
        state.reset_native_state = true;
    } else if state.composing && (disabled || !has_focus) {
        state.cancel_composition(adapter.document);
        response.changed = true;
    }
    // Force source-anchored selection after each transaction. In particular a
    // revealed prefix must never move the caret by the hidden marker count.
    if !state.composing && (adapter.changed || response.changed) {
        let current = Projection::build(adapter.document, state.reveal.as_ref());
        if let Some(mut native) = egui::text_edit::TextEditState::load(ui.ctx(), id) {
            native.cursor.set_char_range(Some(CCursorRange {
                primary: CCursor::new(current.view_at(state.selection.primary)),
                secondary: CCursor::new(current.view_at(state.selection.secondary)),
                h_pos: None,
            }));
            native.clear_undoer();
            native.store(ui.ctx(), id);
        }
    }
    let wanted = if has_focus {
        caret_block(adapter.document, state.selection.primary)
    } else {
        options
            .active
            .and_then(|range| caret_block(adapter.document, range.start))
    };
    if !state.composing && wanted != state.reveal {
        ui.ctx().request_repaint();
    }
    response.composing = state.composing;
    response
}

fn paint_structures(
    ui: &mut egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    projection: &Projection,
    size: f32,
    options: &ViewOptions<'_>,
    background: egui::layers::ShapeIdx,
    images: &mut super::media::Images,
) {
    let mut shapes = Vec::new();
    for block in &projection.blocks {
        let start = output
            .galley
            .pos_from_cursor(CCursor::new(block.view_range.start))
            .translate(output.galley_pos.to_vec2());
        let end = output
            .galley
            .pos_from_cursor(CCursor::new(
                block
                    .view_range
                    .end
                    .saturating_sub(1)
                    .max(block.view_range.start),
            ))
            .translate(output.galley_pos.to_vec2());
        if block.kind == BlockKind::Code {
            let start = output
                .galley
                .pos_from_cursor(CCursor::new(
                    block
                        .header
                        .as_ref()
                        .map_or(block.content_start, |range| range.start),
                ))
                .translate(output.galley_pos.to_vec2());
            let rect = egui::Rect::from_min_max(
                egui::pos2(output.galley_pos.x - 10.0, start.top() - 3.0),
                egui::pos2(output.response.rect.right() - 4.0, end.bottom() + 3.0),
            );
            shapes.push(egui::Shape::rect_filled(
                rect,
                10.0,
                Color32::from_gray(if options.dark { 26 } else { 237 }),
            ));
            shapes.push(egui::Shape::rect_stroke(
                rect,
                10.0,
                Stroke::new(1.0, Color32::from_gray(if options.dark { 45 } else { 210 })),
                egui::StrokeKind::Inside,
            ));
        }
        if block.revealed {
            continue;
        }
        for reference in &block.footnotes {
            let caret = output
                .galley
                .pos_from_cursor(CCursor::new(block.content_start + reference.range.start))
                .translate(output.galley_pos.to_vec2());
            ui.painter().text(
                egui::pos2(caret.left() + 1.0, caret.top()),
                egui::Align2::LEFT_TOP,
                reference.number.to_string(),
                FontId::monospace(size * 0.72),
                options.accent,
            );
        }
        for math in &block.math {
            let caret = output
                .galley
                .pos_from_cursor(CCursor::new(block.content_start + math.range.start))
                .translate(output.galley_pos.to_vec2());
            let formula = super::math::formula(ui.ctx(), &math.latex, math.display);
            let dimensions = fit_formula(
                formula.size * (size / 16.0),
                (output.response.rect.right() - output.galley_pos.x - 12.0).max(1.0),
            );
            let rect = egui::Rect::from_min_size(
                egui::pos2(caret.left() + 2.0, caret.center().y - dimensions.y / 2.0),
                dimensions,
            );
            super::math::paint(ui, &formula, rect, ui.visuals().text_color());
        }
        if let Some(image) = &block.image {
            let rect = egui::Rect::from_min_size(
                egui::pos2(output.galley_pos.x, start.top() + 3.0),
                egui::vec2(
                    (output.response.rect.right() - output.galley_pos.x - 8.0).max(1.0),
                    block.image_height,
                ),
            );
            images.show(ui, rect, image, options.language);
        }
    }
    ui.painter().set(background, egui::Shape::Vec(shapes));
}

fn container_indent(block: &ProjectedBlock, size: f32) -> f32 {
    block.indent as f32 * size * 0.55
        + block.quote_depth as f32 * 14.0
        + if block.quote_depth > 0
            && matches!(
                block.kind,
                BlockKind::Bullet | BlockKind::Ordered | BlockKind::Task(_)
            )
        {
            20.0
        } else {
            0.0
        }
}

fn paint_decorations(
    ui: &mut egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    projection: &Projection,
    font_size: f32,
    language: diamond_core::Language,
) -> Option<usize> {
    let mut toggled = None;
    let color = ui.visuals().weak_text_color();
    for block in &projection.blocks {
        if block.revealed && !matches!(block.kind, BlockKind::Task(_)) {
            continue;
        }
        let caret = output
            .galley
            .pos_from_cursor(CCursor::new(block.content_start))
            .translate(output.galley_pos.to_vec2());
        let indent = container_indent(block, font_size);
        let body_x = output.galley_pos.x + indent;
        let center = egui::pos2(body_x - 18.0, caret.center().y);
        if !block.revealed && block.quote_depth > 0 {
            let end = output
                .galley
                .pos_from_cursor(CCursor::new(
                    block
                        .view_range
                        .end
                        .saturating_sub(1)
                        .max(block.content_start),
                ))
                .translate(output.galley_pos.to_vec2());
            for level in 0..block.quote_depth {
                let marker_space = if matches!(
                    block.kind,
                    BlockKind::Bullet | BlockKind::Ordered | BlockKind::Task(_)
                ) {
                    30.0
                } else {
                    10.0
                };
                let x = body_x - marker_space - level as f32 * 14.0;
                ui.painter().line_segment(
                    [egui::pos2(x, caret.top()), egui::pos2(x, end.bottom())],
                    Stroke::new(2.0, color),
                );
            }
        }
        match block.kind {
            BlockKind::Bullet => {
                ui.painter().circle_filled(center, 2.5, color);
            }
            BlockKind::Ordered => {
                ui.painter().text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    format!("{}.", block.order),
                    FontId::monospace(font_size * 0.8),
                    color,
                );
            }
            BlockKind::Task(done) => {
                let hit = egui::Rect::from_center_size(
                    center,
                    egui::vec2(30.0, caret.height().clamp(22.0, 30.0)),
                );
                let response = ui
                    .interact(
                        hit,
                        output.response.id.with(("task", block.index)),
                        egui::Sense::click(),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if response.is_pointer_button_down_on() || response.clicked() {
                    response.request_focus();
                }
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Checkbox,
                        ui.is_enabled(),
                        language.text(if done { "Completed task" } else { "Task" }),
                    )
                });
                if !block.revealed || response.hovered() || response.is_pointer_button_down_on() {
                    let tint = if done || response.hovered() {
                        ui.visuals().hyperlink_color
                    } else {
                        color
                    };
                    if done {
                        ui.painter().circle_filled(center, 8.5, tint);
                        let ink = if u16::from(tint.r()) + u16::from(tint.g()) + u16::from(tint.b())
                            > 380
                        {
                            Color32::from_gray(20)
                        } else {
                            Color32::WHITE
                        };
                        ui.painter().line_segment(
                            [
                                center + egui::vec2(-4.0, 0.0),
                                center + egui::vec2(-1.0, 3.0),
                            ],
                            Stroke::new(1.8, ink),
                        );
                        ui.painter().line_segment(
                            [
                                center + egui::vec2(-1.0, 3.0),
                                center + egui::vec2(4.5, -3.5),
                            ],
                            Stroke::new(1.8, ink),
                        );
                    } else {
                        ui.painter()
                            .circle_stroke(center, 8.5, Stroke::new(1.5, tint));
                    }
                }
                if response.clicked() {
                    toggled = Some(block.index);
                }
            }
            BlockKind::Rule => {
                ui.painter().line_segment(
                    [
                        egui::pos2(caret.left(), center.y),
                        egui::pos2(output.response.rect.right() - 8.0, center.y),
                    ],
                    Stroke::new(1.0, color),
                );
            }
            _ => {}
        }
    }
    toggled
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(document: &mut Document, reveal: Option<Range<usize>>) -> LiveBuffer<'_> {
        let projection = Projection::build(document, reveal.as_ref());
        LiveBuffer {
            text: projection.text.clone(),
            document,
            projection: Rc::new(RefCell::new(projection)),
            reveal,
            error: None,
            pending_insert: None,
            forced_selection: None,
            changed: false,
            checkpoint: None,
        }
    }

    #[test]
    fn code_heading_reserves_a_real_row_before_the_code_body() {
        let source = "Before\n\n```rust\nfn main() {}\n```\n";
        let document = Document::parse(source);
        let projection = Projection::build(&document, None);
        let code = projection
            .blocks
            .iter()
            .find(|block| block.kind == BlockKind::Code)
            .unwrap();
        assert!(code.header.is_some());
        assert!(code.content_start > code.header.as_ref().unwrap().start);
        assert!(projection.text.contains("rust\nfn main() {}"));
        let revealed = Projection::build(&document, Some(&code.source_range));
        assert_eq!(
            revealed.view_at(code.source_range.start),
            revealed.blocks[1].content_start
        );
        assert_eq!(document.to_markdown(), source);
    }

    #[test]
    fn task_pointer_press_release_is_not_stolen_by_the_text_editor() {
        let context = egui::Context::default();
        let id = egui::Id::new("task-click-regression");
        let mut document = Document::parse("- [ ] task\n");
        let mut state = LiveState::default();
        let mut images = super::super::media::Images::default();
        let options = ViewOptions {
            language: diamond_core::Language::En,
            family: egui::FontFamily::Proportional,
            wrap: true,
            matches: &[],
            active: None,
            jump: None,
            accent: Color32::LIGHT_GRAY,
            dark: true,
        };
        let point = egui::pos2(26.0, 25.0);
        let frames = vec![
            vec![],
            vec![egui::Event::PointerMoved(point)],
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            }],
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
        ];
        for (frame, events) in frames.into_iter().enumerate() {
            let _ = context.run(
                egui::RawInput {
                    events,
                    time: Some(frame as f64),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(640.0, 480.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        show(
                            ui,
                            &mut document,
                            &mut state,
                            id,
                            17.0,
                            &options,
                            &mut images,
                        );
                    });
                },
            );
        }
        assert_eq!(document.blocks()[0].kind, BlockKind::Task(true));
        assert!(document.undo());
        assert_eq!(document.to_markdown(), "- [ ] task\n");
    }

    #[test]
    fn blank_line_caret_does_not_reveal_the_preceding_markdown() {
        for source in ["# Heading\n", "- [ ] task\n", "1. item\n\n", "**bold**\n\n"] {
            let document = Document::parse(source);
            let caret = source.chars().count();
            assert_eq!(caret_block(&document, caret), None);
            let view = Projection::build(&document, caret_block(&document, caret).as_ref());
            assert!(!view.blocks[0].revealed);
        }
        let unfinished = Document::parse("```rust\n");
        assert!(caret_block(&unfinished, 8).is_some());
    }

    #[test]
    fn unfocused_window_reaches_idle_with_text_widget_focus_retained() {
        let context = egui::Context::default();
        let id = egui::Id::new("focus-regression");
        let mut document = Document::parse("# Heading\n\n- [ ] task\n");
        let mut state = LiveState::default();
        let mut images = super::super::media::Images::default();
        let options = ViewOptions {
            language: diamond_core::Language::En,
            family: egui::FontFamily::Proportional,
            wrap: true,
            matches: &[],
            active: None,
            jump: None,
            accent: Color32::LIGHT_GRAY,
            dark: true,
        };
        let mut delay = std::time::Duration::ZERO;
        for frame in 0..8 {
            let input = egui::RawInput {
                focused: false,
                time: Some(frame as f64),
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 480.0),
                )),
                ..Default::default()
            };
            let output = context.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.memory_mut(|memory| memory.request_focus(id));
                    show(
                        ui,
                        &mut document,
                        &mut state,
                        id,
                        17.0,
                        &options,
                        &mut images,
                    );
                });
            });
            delay = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
        }
        assert!(
            delay > std::time::Duration::ZERO,
            "unfocused editor must not continuously repaint"
        );
        assert_eq!(state.reveal, None);
    }

    #[test]
    fn list_links_and_rules_project_without_source_delimiters() {
        let source = "- [ ] [сайт](https://example.org) и [[Note|笔记]]\n\n---\n";
        let document = Document::parse(source);
        let projection = Projection::build(&document, None);
        assert!(projection.text.starts_with("сайт и 笔记\n"));
        assert!(!projection.text.contains("[["));
        assert!(!projection.text.contains("---"));
        assert_eq!(projection.blocks[0].links.len(), 2);
        assert_eq!(projection.blocks[1].kind, BlockKind::Rule);
        assert!(
            projection
                .to_source
                .windows(2)
                .all(|pair| pair[0] <= pair[1])
        );
        assert_eq!(document.to_markdown(), source);
    }

    #[test]
    fn inactive_blocks_hide_markers_and_reveal_real_source_at_the_caret() {
        let document = Document::parse("# Привет\n\n**мир**\n");
        let hidden = Projection::build(&document, None);
        assert_eq!(hidden.text, "Привет\n\nмир\n");
        let active = Projection::build(&document, Some(&(0..9)));
        assert_eq!(active.text, "# Привет\n\nмир\n");
    }

    #[test]
    fn unicode_caret_stays_on_same_content_after_reveal() {
        let document = Document::parse("# Я🙂\n\n**世界**\n");
        let hidden = Projection::build(&document, None);
        let source = hidden.source_at(2);
        let active = Projection::build(&document, Some(&(0..6)));
        assert_eq!(active.source_at(active.view_at(source)), source);
        assert_eq!(active.view_at(source), 4);
    }

    #[test]
    fn deleting_revealed_heading_marker_changes_semantics_and_is_undoable() {
        let mut document = Document::parse("# Title\n");
        let mut editor = buffer(&mut document, Some(0..8));
        editor.delete_char_range(0..2);
        assert_eq!(editor.document.blocks()[0].kind, BlockKind::Paragraph);
        assert_eq!(editor.document.to_markdown(), "Title\n");
        assert!(editor.document.undo());
        assert_eq!(editor.document.to_markdown(), "# Title\n");
    }

    #[test]
    fn typed_markdown_reparses_as_structure() {
        let mut document = Document::parse("Title");
        let mut editor = buffer(&mut document, Some(0..5));
        assert_eq!(editor.insert_text("# ", 0), 2);
        assert_eq!(editor.document.blocks()[0].kind, BlockKind::Heading(1));
        assert_eq!(editor.document.to_markdown(), "# Title");
    }

    #[test]
    fn crlf_maps_one_visible_newline_to_two_source_scalars() {
        let mut document = Document::parse("# Я\r\n");
        let mut editor = buffer(&mut document, Some(0..5));
        assert_eq!(editor.as_str(), "# Я\n");
        assert_eq!(editor.insert_text("a\nb", 3), 3);
        assert_eq!(editor.document.to_markdown(), "# Яa\r\nb\r\n");
    }

    #[test]
    fn replacement_callbacks_coalesce_to_one_undo_transaction() {
        let mut document = Document::parse("**Привет**\n");
        let before = document.clone();
        let mut editor = buffer(&mut document, Some(0..11));
        editor.delete_char_range(2..8);
        editor.insert_text("世界", 2);
        let mut merged = coalesce(before, editor.document).unwrap();
        assert_eq!(merged.to_markdown(), "**世界**\n");
        assert!(merged.undo());
        assert_eq!(merged.to_markdown(), "**Привет**\n");
    }

    #[test]
    fn cancelled_ime_restores_precomposition_document_and_history() {
        let mut document = Document::parse("Я");
        let mut state = LiveState {
            composing: true,
            composition_checkpoint: Some(document.clone()),
            ..LiveState::default()
        };
        document.replace_source_range(1..1, "候補").unwrap();
        assert!(state.cancel_composition(&mut document));
        assert_eq!(document.to_markdown(), "Я");
        assert!(!document.undo());
        assert!(!state.is_composing());
    }
}

fn fit_formula(size: egui::Vec2, width: f32) -> egui::Vec2 {
    size * (width.max(1.0) / size.x.max(1.0)).min(1.0)
}

fn paint_tables(
    ui: &mut egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    projection: &Projection,
    size: f32,
    options: &ViewOptions<'_>,
    jump: &mut Option<usize>,
    open_link: &mut Option<String>,
) -> Option<(usize, usize, usize, diamond_document::TableAction)> {
    use diamond_document::{CellAlignment, TableAction};
    let mut command = None;
    for block in projection
        .blocks
        .iter()
        .filter(|block| !block.revealed && block.table.is_some())
    {
        let Some(table) = &block.table else { continue };
        let Some(layout) = table_layout(ui, block, size, &options.family) else {
            continue;
        };
        let start = output
            .galley
            .pos_from_cursor(CCursor::new(block.content_start))
            .translate(output.galley_pos.to_vec2());
        let rows = table
            .cells
            .iter()
            .map(|cell| cell.row + 1)
            .max()
            .unwrap_or(1);
        let height = rows as f32 * size * 1.9 + 24.0;
        let rect = egui::Rect::from_min_size(
            egui::pos2(output.galley_pos.x, start.top()),
            egui::vec2(
                (output.response.rect.right() - output.galley_pos.x - 8.0).max(1.0),
                height,
            ),
        );
        if !ui.is_rect_visible(rect) {
            continue;
        }
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.set_clip_rect(rect.intersect(ui.clip_rect()));
            egui::ScrollArea::horizontal()
                .max_width(rect.width())
                .id_salt(("table-scroll", output.response.id, block.index))
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    let (area, _) = ui.allocate_exact_size(
                        egui::vec2(layout.width(), rows as f32 * size * 1.9),
                        egui::Sense::hover(),
                    );
                    for cell in &table.cells {
                        let left =
                            area.left() + layout.columns.iter().take(cell.column).sum::<f32>();
                        let cell_rect = egui::Rect::from_min_size(
                            egui::pos2(left, area.top() + cell.row as f32 * size * 1.9),
                            egui::vec2(layout.columns[cell.column], size * 1.9),
                        );
                        if !ui.is_rect_visible(cell_rect) {
                            continue;
                        }
                        let painter = ui
                            .painter()
                            .with_clip_rect(cell_rect.intersect(ui.clip_rect()));
                        painter.rect_filled(
                            cell_rect,
                            0.0,
                            if cell.row == 0 {
                                ui.visuals().faint_bg_color
                            } else {
                                ui.visuals().extreme_bg_color
                            },
                        );
                        painter.rect_stroke(
                            cell_rect,
                            0.0,
                            Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
                            egui::StrokeKind::Inside,
                        );
                        let mut job = LayoutJob::default();
                        for (offset, c) in cell.text.chars().enumerate() {
                            let position = cell.range.start + offset;
                            let mut format = TextFormat::simple(
                                FontId::new(size, options.family.clone()),
                                ui.visuals().text_color(),
                            );
                            for mark in block
                                .marks
                                .iter()
                                .filter(|mark| mark.range.contains(&position))
                            {
                                match mark.kind {
                                    MarkKind::Bold => format.extra_letter_spacing = 0.3,
                                    MarkKind::Italic => format.italics = true,
                                    MarkKind::Strike => {
                                        format.strikethrough = Stroke::new(1.0, format.color)
                                    }
                                    MarkKind::Code => {
                                        format.font_id = FontId::monospace(size);
                                        format.background = ui.visuals().code_bg_color;
                                    }
                                }
                            }
                            if block
                                .links
                                .iter()
                                .any(|link| link.range.contains(&position))
                            {
                                format.color = options.accent;
                                format.underline = Stroke::new(1.0, options.accent);
                            }
                            if let Some(math) = block
                                .math
                                .iter()
                                .find(|math| math.range.contains(&position))
                            {
                                let dimensions = fit_formula(
                                    super::math::formula(ui.ctx(), &math.latex, math.display).size
                                        * (size / 16.0),
                                    cell_rect.width() - 20.0,
                                );
                                let unit = ui
                                    .fonts_mut(|fonts| {
                                        fonts
                                            .layout_no_wrap(
                                                "◇".into(),
                                                FontId::monospace(16.0),
                                                Color32::WHITE,
                                            )
                                            .size()
                                            .x
                                    })
                                    .max(1.0);
                                format.font_id = FontId::monospace(dimensions.x * 16.0 / unit);
                                format.color = Color32::TRANSPARENT;
                                format.extra_letter_spacing = 0.0;
                                format.line_height = Some(size * 1.9);
                            }
                            job.append(&c.to_string(), 0.0, format);
                        }
                        super::emoji::decorate_job(ui.ctx(), &mut job);
                        let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                        let x = match table.columns[cell.column] {
                            CellAlignment::Left => cell_rect.left() + 10.0,
                            CellAlignment::Center => cell_rect.center().x - galley.size().x / 2.0,
                            CellAlignment::Right => cell_rect.right() - galley.size().x - 10.0,
                        };
                        let position = egui::pos2(x, cell_rect.center().y - galley.size().y / 2.0);
                        painter.galley(position, galley.clone(), ui.visuals().text_color());
                        super::emoji::paint(ui, &galley, position, &cell.text, size);
                        for math in block.math.iter().filter(|math| {
                            cell.range.start <= math.range.start && math.range.end <= cell.range.end
                        }) {
                            let caret = galley
                                .pos_from_cursor(CCursor::new(math.range.start - cell.range.start))
                                .translate(position.to_vec2());
                            let formula = super::math::formula(ui.ctx(), &math.latex, math.display);
                            let dimensions =
                                fit_formula(formula.size * (size / 16.0), cell_rect.width() - 20.0);
                            super::math::paint(
                                ui,
                                &formula,
                                egui::Rect::from_min_size(
                                    egui::pos2(
                                        caret.left(),
                                        cell_rect.center().y - dimensions.y / 2.0,
                                    ),
                                    dimensions,
                                ),
                                ui.visuals().text_color(),
                            );
                        }
                        let response = ui.interact(
                            cell_rect,
                            output.response.id.with((
                                "table-cell",
                                block.index,
                                cell.row,
                                cell.column,
                            )),
                            egui::Sense::click(),
                        );
                        if response.double_clicked() {
                            *jump = block.after.get(cell.range.start).copied();
                        }
                        if response.clicked()
                            && ui.input(|input| input.modifiers.command)
                            && let Some(link) = block.links.iter().find(|link| {
                                link.range.start < cell.range.end
                                    && cell.range.start < link.range.end
                            })
                        {
                            *open_link = Some(link.target.clone());
                        }
                        response.context_menu(|ui| {
                            let tr = |key| options.language.text(key);
                            if ui.button(tr("Edit cell source")).clicked() {
                                *jump = block.after.get(cell.range.start).copied();
                                ui.close();
                            }
                            for (title, action) in [
                                ("Insert row above", TableAction::InsertRowBefore),
                                ("Insert row below", TableAction::InsertRowAfter),
                                ("Delete row", TableAction::DeleteRow),
                                ("Insert column before", TableAction::InsertColumnBefore),
                                ("Insert column after", TableAction::InsertColumnAfter),
                                ("Delete column", TableAction::DeleteColumn),
                                ("Align left", TableAction::Align(CellAlignment::Left)),
                                ("Align center", TableAction::Align(CellAlignment::Center)),
                                ("Align right", TableAction::Align(CellAlignment::Right)),
                            ] {
                                if ui
                                    .add_enabled(
                                        !(cell.row == 0
                                            && matches!(action, TableAction::DeleteRow)),
                                        egui::Button::new(tr(title)),
                                    )
                                    .clicked()
                                {
                                    command = Some((block.index, cell.row, cell.column, action));
                                    ui.close();
                                }
                            }
                        });
                    }
                });
        });
    }
    command
}
