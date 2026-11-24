// SPDX-License-Identifier: GPL-3.0-only
//! Read-only frontmatter rendering inside the native document flow.
//! Clicking the block reveals its YAML; tags alone navigate to search.
use diamond_core::Language;
use diamond_document::frontmatter::{Frontmatter, PropertyValue};
use egui::{Color32, FontId, Galley, Rect, Stroke, Ui, Vec2};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Default)]
pub(super) struct RenderCache(RefCell<Option<(Style, Rc<Layout>)>>);
#[derive(Clone, PartialEq)]
struct Style {
    font: FontId,
    width: u32,
    epoch: u64,
    scale: u32,
    text: Color32,
    muted: Color32,
    accent: Color32,
}
pub(super) struct Layout {
    pub height: f32,
    rows: Vec<Row>,
    column: f32,
}
struct Row {
    top: f32,
    label: Arc<Galley>,
    value: Option<Arc<Galley>>,
    tags: Vec<Tag>,
}
struct Tag {
    text: String,
    galley: Arc<Galley>,
    rect: Rect,
}

fn galley(ui: &Ui, text: String, font: FontId, color: Color32, width: f32) -> Arc<Galley> {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = width.max(1.0);
    job.append(&text, 0.0, egui::TextFormat::simple(font, color));
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

impl RenderCache {
    pub fn cached(&self) -> Option<Rc<Layout>> {
        self.0
            .borrow()
            .as_ref()
            .map(|(_, layout)| Rc::clone(layout))
    }
    pub fn layout(&self, ui: &Ui, header: &Frontmatter, font: FontId, width: f32) -> Rc<Layout> {
        let style = Style {
            font: font.clone(),
            width: width.to_bits(),
            epoch: super::font_epoch(ui.ctx()),
            scale: ui.ctx().pixels_per_point().to_bits(),
            text: ui.visuals().text_color(),
            muted: ui.visuals().weak_text_color(),
            accent: ui.visuals().hyperlink_color,
        };
        if let Some((_, layout)) = self.0.borrow().as_ref().filter(|(key, _)| *key == style) {
            return Rc::clone(layout);
        }
        let inner = (width - 24.0).max(1.0);
        let column = (inner * 0.28).clamp(80.0, 180.0).min(inner * 0.5);
        let value_width = (inner - column - 12.0).max(1.0);
        let row_height = (font.size * 1.65).max(26.0);
        let mut top = 10.0;
        let mut rows = Vec::new();
        for property in &header.properties {
            let icon = match &property.value {
                PropertyValue::Number(_) => "01",
                PropertyValue::Date(_) => "[]",
                PropertyValue::Bool(_) => "[x]",
                PropertyValue::List(_) => "#",
                _ => "Aa",
            };
            let label = galley(
                ui,
                format!("{icon}  {}", property.name),
                FontId::new(font.size * 0.9, font.family.clone()),
                style.muted,
                column,
            );
            let mut height = row_height.max(label.size().y);
            let mut tags = Vec::new();
            let value = if property.name == "tags"
                && matches!(
                    property.value,
                    PropertyValue::Text(_) | PropertyValue::List(_)
                ) {
                let mut x = 0.0;
                let mut y = 0.0;
                let mut line_height = row_height;
                for text in header.tags() {
                    let caption = galley(
                        ui,
                        text.clone(),
                        font.clone(),
                        style.accent,
                        (value_width - 16.0).max(1.0),
                    );
                    let size = caption.size() + egui::vec2(16.0, 6.0);
                    if x > 0.0 && x + size.x > value_width {
                        x = 0.0;
                        y += line_height + 4.0;
                        line_height = row_height;
                    }
                    let rect = Rect::from_min_size(egui::pos2(x, y), size);
                    x += size.x + 6.0;
                    line_height = line_height.max(size.y);
                    tags.push(Tag {
                        text,
                        galley: caption,
                        rect,
                    });
                }
                height = height.max(y + line_height);
                if tags.is_empty() {
                    Some(galley(
                        ui,
                        "—".into(),
                        font.clone(),
                        style.muted,
                        value_width,
                    ))
                } else {
                    None
                }
            } else {
                let text = property.value.text();
                let value = galley(
                    ui,
                    if text.is_empty() { "—".into() } else { text },
                    font.clone(),
                    style.text,
                    value_width,
                );
                height = height.max(value.size().y);
                Some(value)
            };
            rows.push(Row {
                top,
                label,
                value,
                tags,
            });
            top += height + 8.0;
        }
        if rows.is_empty() {
            rows.push(Row {
                top,
                label: galley(ui, "{}".into(), font, style.muted, inner),
                value: None,
                tags: Vec::new(),
            });
            top += row_height;
        }
        let layout = Rc::new(Layout {
            height: top + 6.0,
            rows,
            column,
        });
        *self.0.borrow_mut() = Some((style, Rc::clone(&layout)));
        layout
    }

    pub fn paint(
        &self,
        ui: &mut Ui,
        rect: Rect,
        layout: &Layout,
        id: egui::Id,
        language: Language,
    ) -> (bool, Option<String>) {
        if !ui.is_rect_visible(rect) {
            return (false, None);
        }
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        painter.rect_filled(rect, 12.0, ui.visuals().faint_bg_color);
        painter.rect_stroke(
            rect,
            12.0,
            Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
            egui::StrokeKind::Inside,
        );
        let response = ui.interact(rect, id.with("property-block"), egui::Sense::click());
        let mut tag_hovered = false;
        let mut open_tag = None;
        let pointer = ui.input(|input| input.pointer.hover_pos());
        let value_left = rect.left() + 12.0 + layout.column + 12.0;
        for (row_index, row) in layout.rows.iter().enumerate() {
            let origin = egui::pos2(rect.left() + 12.0, rect.top() + row.top);
            painter.galley(
                origin,
                Arc::clone(&row.label),
                ui.visuals().weak_text_color(),
            );
            if let Some(value) = &row.value {
                painter.galley(
                    egui::pos2(value_left, origin.y),
                    Arc::clone(value),
                    ui.visuals().text_color(),
                );
            }
            for (tag_index, tag) in row.tags.iter().enumerate() {
                let tag_rect = tag.rect.translate(Vec2::new(value_left, origin.y));
                if !ui.is_rect_visible(tag_rect) {
                    continue;
                }
                let accent = ui.visuals().hyperlink_color;
                painter.rect_filled(tag_rect, 12.0, accent.gamma_multiply(0.14));
                painter.galley(
                    tag_rect.min + egui::vec2(8.0, 3.0),
                    Arc::clone(&tag.galley),
                    accent,
                );
                let tag_response = ui
                    .interact(
                        tag_rect,
                        id.with(("property-tag", row_index, tag_index)),
                        egui::Sense::click(),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(language.text("Search this tag"));
                tag_hovered |= pointer.is_some_and(|point| tag_rect.contains(point));
                if tag_response.clicked() {
                    open_tag = Some(tag.text.clone());
                }
            }
        }
        if !tag_hovered {
            response
                .clone()
                .on_hover_text(language.text("Edit YAML: click"));
        }
        (response.clicked() && !tag_hovered, open_tag)
    }
}
