// SPDX-License-Identifier: GPL-3.0-only
//! A native split preview. Rendering and compilation stay in the plugin worker.
use diamond_plugin::{SourceAnchor, ViewDescriptor, ViewEvent, ViewFrame};
use std::time::Instant;
pub struct Preview {
    pub view: ViewDescriptor,
    pub document_id: String,
    pub sent: Option<(u64, u64)>,
    pub observed: Option<(u64, u64)>,
    pub changed_at: Instant,
    pub event: Option<ViewEvent>,
    pub sync: bool,
    first_page: usize,
    page_count: usize,
    scale: f32,
    pages: Vec<egui::TextureHandle>,
    anchors: Vec<SourceAnchor>,
    diagnostics: Vec<diamond_plugin::Diagnostic>,
    cursor: Option<usize>,
    last_cursor: Option<usize>,
}
impl Preview {
    pub fn new(view: ViewDescriptor) -> Self {
        Self {
            view,
            document_id: String::new(),
            sent: None,
            observed: None,
            changed_at: Instant::now(),
            event: None,
            sync: true,
            first_page: 0,
            page_count: 0,
            scale: 1.0,
            pages: Vec::new(),
            anchors: Vec::new(),
            diagnostics: Vec::new(),
            cursor: None,
            last_cursor: None,
        }
    }
    pub fn accepts(&self, path: &std::path::Path) -> bool {
        path.extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| {
                self.view
                    .extensions
                    .iter()
                    .any(|item| item.eq_ignore_ascii_case(extension))
            })
    }
    pub fn clear_document(&mut self, id: String) {
        self.document_id = id;
        self.sent = None;
        self.observed = None;
        self.first_page = 0;
        self.page_count = 0;
        self.pages.clear();
        self.anchors.clear();
        self.diagnostics.clear();
        self.last_cursor = None;
    }
    pub fn receive(&mut self, ctx: &egui::Context, frame: ViewFrame, pages: Vec<egui::ColorImage>) {
        // Diagnostics-only failure frames retain the last successful preview.
        if !pages.is_empty() {
            self.first_page = frame.first_page;
            self.page_count = frame.page_count.unwrap_or(pages.len());
            self.pages = pages
                .into_iter()
                .map(|image| ctx.load_texture("plugin-page", image, egui::TextureOptions::LINEAR))
                .collect();
            self.anchors = frame.anchors;
        }
        self.diagnostics = frame.diagnostics;
        self.cursor = frame.cursor;
    }
    fn offset_at(&self, page: usize, x: f32, y: f32) -> Option<usize> {
        self.anchors
            .iter()
            .filter(|anchor| anchor.page == page)
            .min_by(|a, b| {
                ((a.y - y).abs() + (a.x - x).abs() * 0.1)
                    .total_cmp(&((b.y - y).abs() + (b.x - x).abs() * 0.1))
            })
            .map(|anchor| anchor.offset)
    }
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        source_cursor: usize,
        language: diamond_core::Language,
    ) -> (bool, Option<usize>) {
        let mut close = false;
        let mut jump = self.cursor.take();
        ui.horizontal(|ui| {
            ui.strong(&self.view.title);
            ui.checkbox(&mut self.sync, language.text("Sync scrolling"));
            close = ui.button("×").clicked();
        });
        if self.page_count > self.pages.len() {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(self.first_page > 0, egui::Button::new("←"))
                    .clicked()
                {
                    self.event = Some(ViewEvent::RequestPages {
                        start: self.first_page.saturating_sub(4),
                        count: 4,
                        scale: self.scale,
                    });
                }
                ui.label(format!(
                    "{}–{} / {}",
                    self.first_page + 1,
                    self.first_page + self.pages.len(),
                    self.page_count
                ));
                if ui
                    .add_enabled(
                        self.first_page + self.pages.len() < self.page_count,
                        egui::Button::new("→"),
                    )
                    .clicked()
                {
                    self.event = Some(ViewEvent::RequestPages {
                        start: self.first_page + self.pages.len(),
                        count: 4,
                        scale: self.scale,
                    });
                }
            });
        }
        for diagnostic in &self.diagnostics {
            if ui
                .add(egui::Button::new(&diagnostic.message).wrap())
                .clicked()
            {
                jump = Some(diagnostic.start);
            }
        }
        let anchor = if self.sync && self.last_cursor != Some(source_cursor) {
            self.anchors
                .iter()
                .min_by_key(|anchor| anchor.offset.abs_diff(source_cursor))
                .cloned()
        } else {
            None
        };
        if self.last_cursor != Some(source_cursor) {
            self.last_cursor = Some(source_cursor);
            self.event = Some(ViewEvent::Cursor {
                offset: source_cursor,
            });
        }
        let mut interaction = None;
        let scroll = egui::ScrollArea::vertical()
            .id_salt(("plugin-preview", &self.view.id))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.pages.is_empty() {
                    ui.label(language.text("Waiting for preview…"));
                }
                for (local_page, texture) in self.pages.iter().enumerate() {
                    let page = self.first_page + local_page;
                    let [width, height] = texture.size();
                    let w = ui.available_width().min(width as f32).max(1.0);
                    let h = w * height as f32 / width as f32;
                    let response = ui.add(
                        egui::Image::new((texture.id(), egui::vec2(w, h)))
                            .sense(egui::Sense::click()),
                    );
                    if let Some(anchor) = anchor.as_ref().filter(|anchor| anchor.page == page) {
                        ui.scroll_to_rect(
                            egui::Rect::from_min_size(
                                egui::pos2(
                                    response.rect.left() + anchor.x * w,
                                    response.rect.top() + anchor.y * h,
                                ),
                                egui::Vec2::splat(1.0),
                            ),
                            Some(egui::Align::Center),
                        );
                    }
                    if response.clicked()
                        && let Some(pointer) = response.interact_pointer_pos()
                    {
                        interaction = Some((
                            page,
                            (pointer.x - response.rect.left()) / w,
                            (pointer.y - response.rect.top()) / h,
                            true,
                        ));
                    }
                    if self.sync
                        && response.hovered()
                        && ui.input(|input| input.raw_scroll_delta.y != 0.0)
                    {
                        let y =
                            ((ui.clip_rect().center().y - response.rect.top()) / h).clamp(0.0, 1.0);
                        interaction = Some((page, 0.0, y, false));
                    }
                    ui.add_space(12.0);
                }
            });
        let _ = scroll;
        if let Some((page, x, y, click)) = interaction {
            if self.sync || click {
                jump = self.offset_at(page, x, y);
            }
            self.event = Some(if click {
                ViewEvent::Click { page, x, y }
            } else {
                ViewEvent::Scroll { page, y }
            });
        }
        (close, jump)
    }
}
