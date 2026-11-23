// SPDX-License-Identifier: GPL-3.0-only
//! Native color emoji from a system color font, with bounded texture caching.
//! Original text and scalar cursor positions are unchanged, including ZWJ clusters.
use egui::{Color32, Context, TextureHandle};
use std::{
    ops::Range,
    sync::{Arc, OnceLock},
};
use swash::{
    FontRef,
    scale::{Render, ScaleContext, Source, StrikeWith},
    shape::ShapeContext,
};
use unicode_segmentation::UnicodeSegmentation;
static FONT: OnceLock<Arc<Vec<u8>>> = OnceLock::new();
pub fn install_font(bytes: Vec<u8>) {
    if FontRef::from_index(&bytes, 0).is_some() {
        let _ = FONT.set(Arc::new(bytes));
    }
}
#[derive(Clone)]
struct Entry {
    text: String,
    texture: Option<TextureHandle>,
}
#[derive(Clone, Default)]
struct Cache(Vec<Entry>);
#[derive(Clone)]
struct Run {
    bytes: Range<usize>,
    scalars: Range<usize>,
    texture: TextureHandle,
}
fn candidate(text: &str) -> bool {
    text.chars().any(
        |c| matches!(c as u32,0x1f000..=0x1faff|0x2600..=0x27ff|0x2300..=0x23ff|0x2b00..=0x2bff),
    ) || text.contains('\u{20e3}')
}
fn texture(ctx: &Context, text: &str) -> Option<TextureHandle> {
    let bytes = FONT.get()?;
    let id = egui::Id::new("color-emoji-cache");
    if let Some(hit) = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Cache>(id)
            .0
            .iter()
            .find(|entry| entry.text == text)
            .cloned()
    }) {
        return hit.texture;
    }
    let result = (|| {
        if text.len() > 96 {
            return None;
        }
        let font = FontRef::from_index(bytes, 0)?;
        let mut shaping = ShapeContext::new();
        let mut shaper = shaping.builder(font).size(64.0).build();
        shaper.add_str(text);
        let mut glyphs = Vec::new();
        shaper.shape_with(|cluster| {
            for glyph in cluster.glyphs {
                if glyph.id != 0 {
                    glyphs.push(glyph.id);
                }
            }
        });
        if glyphs.len() != 1 {
            return None;
        }
        let mut context = ScaleContext::new();
        let mut scaler = context.builder(font).size(64.0).build();
        let image = Render::new(&[
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
        ])
        .render(&mut scaler, glyphs[0])?;
        let w = image.placement.width as usize;
        let h = image.placement.height as usize;
        if w == 0 || h == 0 || w > 256 || h > 256 || image.data.len() != w * h * 4 {
            return None;
        }
        Some(ctx.load_texture(
            "color-emoji",
            egui::ColorImage::from_rgba_unmultiplied([w, h], &image.data),
            egui::TextureOptions::LINEAR,
        ))
    })();
    ctx.data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<Cache>(id);
        if cache.0.len() >= 256 {
            cache.0.remove(0);
        }
        cache.0.push(Entry {
            text: text.into(),
            texture: result.clone(),
        });
    });
    result
}
#[derive(Clone)]
struct RunEntry {
    text: String,
    runs: Arc<Vec<Run>>,
}
#[derive(Clone, Default)]
struct RunCache {
    entries: Vec<RunEntry>,
    bytes: usize,
}
fn empty_runs() -> Arc<Vec<Run>> {
    static EMPTY: OnceLock<Arc<Vec<Run>>> = OnceLock::new();
    Arc::clone(EMPTY.get_or_init(|| Arc::new(Vec::new())))
}
fn runs(ctx: &Context, text: &str) -> Arc<Vec<Run>> {
    if FONT.get().is_none() || text.len() > 512 * 1024 || text.is_ascii() {
        return empty_runs();
    }
    let id = egui::Id::new("emoji-run-cache");
    if let Some(hit) = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<RunCache>(id)
            .entries
            .iter()
            .find(|entry| entry.text == text)
            .map(|entry| Arc::clone(&entry.runs))
    }) {
        return hit;
    }
    let mut result = Vec::new();
    let mut scalar = 0;
    for (byte, grapheme) in text.grapheme_indices(true) {
        let end = scalar + grapheme.chars().count();
        if candidate(grapheme)
            && let Some(texture) = texture(ctx, grapheme)
        {
            result.push(Run {
                bytes: byte..byte + grapheme.len(),
                scalars: scalar..end,
                texture,
            });
        }
        scalar = end;
    }
    let result = Arc::new(result);
    ctx.data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<RunCache>(id);
        while !cache.entries.is_empty()
            && (cache.entries.len() >= 8 || cache.bytes + text.len() > 1024 * 1024)
        {
            let removed = cache.entries.remove(0);
            cache.bytes -= removed.text.len();
        }
        cache.bytes += text.len();
        cache.entries.push(RunEntry {
            text: text.into(),
            runs: Arc::clone(&result),
        });
    });
    result
}
pub fn decorate_job(ctx: &Context, job: &mut egui::text::LayoutJob) {
    let runs = runs(ctx, &job.text);
    if runs.is_empty() {
        return;
    }
    let mut sections = Vec::new();
    for section in &job.sections {
        let mut position = section.byte_range.start;
        while position < section.byte_range.end {
            let at = runs.partition_point(|run| run.bytes.end <= position);
            let hit = runs.get(at).filter(|run| run.bytes.contains(&position));
            let end = if let Some(run) = hit {
                run.bytes.end.min(section.byte_range.end)
            } else {
                runs.get(at).map_or(section.byte_range.end, |run| {
                    run.bytes.start.min(section.byte_range.end)
                })
            };
            if let Some(run) = hit {
                let first_end = position
                    + job.text[position..]
                        .chars()
                        .next()
                        .map_or(0, char::len_utf8);
                let mut format = section.format.clone();
                let base = format.font_id.size;
                format.color = Color32::TRANSPARENT;
                format.extra_letter_spacing = 0.0;
                format.line_height = Some(base * 1.4);
                if position == run.bytes.start {
                    let glyph_width = ctx
                        .fonts_mut(|fonts| {
                            fonts
                                .layout_no_wrap(
                                    job.text[position..first_end].into(),
                                    format.font_id.clone(),
                                    Color32::WHITE,
                                )
                                .size()
                                .x
                        })
                        .max(1.0);
                    format.font_id.size *= base / glyph_width;
                } else {
                    format.font_id.size = 0.001;
                }
                sections.push(egui::text::LayoutSection {
                    leading_space: if position == section.byte_range.start {
                        section.leading_space
                    } else {
                        0.0
                    },
                    byte_range: position..first_end,
                    format: format.clone(),
                });
                if first_end < end {
                    format.font_id.size = 0.001;
                    sections.push(egui::text::LayoutSection {
                        leading_space: 0.0,
                        byte_range: first_end..end,
                        format,
                    });
                }
            } else {
                let mut plain = section.clone();
                plain.byte_range = position..end;
                if position != section.byte_range.start {
                    plain.leading_space = 0.0;
                }
                sections.push(plain);
            }
            position = end;
        }
    }
    job.sections = sections;
}
pub fn paint(ui: &egui::Ui, galley: &egui::Galley, position: egui::Pos2, text: &str, size: f32) {
    let runs = runs(ui.ctx(), text);
    if runs.is_empty() {
        return;
    }
    let first = galley
        .cursor_from_pos(egui::vec2(0.0, ui.clip_rect().top() - position.y))
        .index;
    let last = galley
        .cursor_from_pos(egui::vec2(
            ui.clip_rect().right() - position.x,
            ui.clip_rect().bottom() - position.y,
        ))
        .index;
    let start = runs.partition_point(|run| run.scalars.end < first);
    let end = runs.partition_point(|run| run.scalars.start <= last);
    for run in &runs[start.min(end)..end] {
        let start = galley
            .pos_from_cursor(egui::text::CCursor::new(run.scalars.start))
            .translate(position.to_vec2());
        let end = galley
            .pos_from_cursor(egui::text::CCursor::new(run.scalars.end))
            .translate(position.to_vec2());
        let width = (end.left() - start.left()).max(size * 0.8).min(size * 1.5);
        let [w, h] = run.texture.size();
        let height = width * h as f32 / w as f32;
        let rect = egui::Rect::from_min_size(
            egui::pos2(start.left(), start.center().y - height / 2.0),
            egui::vec2(width, height),
        );
        if ui.is_rect_visible(rect) {
            ui.painter().image(
                run.texture.id(),
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }
}
