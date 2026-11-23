// SPDX-License-Identifier: GPL-3.0-only
//! Native bounded image decoding. Files/network are supplied by the desktop host.
use diamond_document::Image;
use egui::{Color32, ColorImage, Rect, TextureHandle, Ui};
use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver},
    },
};
pub type Loader = Arc<dyn Fn(&str) -> Result<Vec<u8>, String> + Send + Sync>;
const MAX_BYTES: usize = 16 * 1024 * 1024;
static RUNNING: AtomicUsize = AtomicUsize::new(0);
struct Permit;
impl Drop for Permit {
    fn drop(&mut self) {
        RUNNING.fetch_sub(1, Ordering::AcqRel);
    }
}
enum Slot {
    Loading(Receiver<Result<ColorImage, String>>),
    Ready(TextureHandle),
    Error(String),
}
#[derive(Default)]
pub struct Images {
    loader: Option<Loader>,
    slots: HashMap<String, Slot>,
    visible: HashSet<String>,
    requested: HashSet<String>,
}

impl Images {
    pub fn set_loader(&mut self, loader: Loader) {
        self.loader = Some(loader);
        self.slots.clear();
        self.requested.clear();
    }
    pub fn begin_frame(&mut self, ctx: &egui::Context) {
        self.visible.clear();
        for (target, slot) in &mut self.slots {
            let result = if let Slot::Loading(receiver) = slot {
                match receiver.try_recv() {
                    Ok(result) => Some(result),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Some(Err("Image worker stopped".into()))
                    }
                    Err(mpsc::TryRecvError::Empty) => None,
                }
            } else {
                None
            };
            if let Some(result) = result {
                *slot = match result {
                    Ok(image) => Slot::Ready(ctx.load_texture(
                        format!("note-image:{target}"),
                        image,
                        egui::TextureOptions::LINEAR,
                    )),
                    Err(error) => Slot::Error(error),
                };
            }
        }
    }
    pub fn height(&self, target: &str, width: f32) -> f32 {
        if let Some(Slot::Ready(texture)) = self.slots.get(target) {
            let [w, h] = texture.size();
            return (width.min(w as f32) * h as f32 / w as f32).min(360.0);
        }
        88.0
    }
    fn start(&mut self, ctx: &egui::Context, target: &str) {
        if self.slots.contains_key(target) {
            return;
        }
        let Some(loader) = self.loader.clone() else {
            return;
        };
        if self.slots.len() >= 12 {
            let evict = self
                .slots
                .iter()
                .find(|(key, slot)| {
                    !self.visible.contains(*key) && !matches!(slot, Slot::Loading(_))
                })
                .map(|(key, _)| key.clone());
            if let Some(evict) = evict {
                self.slots.remove(&evict);
            } else {
                return;
            }
        }
        if RUNNING
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < 2).then_some(count + 1)
            })
            .is_err()
        {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let target_owned = target.to_owned();
        let repaint = ctx.clone();
        self.slots.insert(target.into(), Slot::Loading(receiver));
        std::thread::spawn(move || {
            let _permit = Permit;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                loader(&target_owned).and_then(|bytes| decode(&bytes))
            }))
            .unwrap_or_else(|_| Err("Image decoder failed".into()));
            let _ = sender.send(result);
            drop(_permit);
            repaint.request_repaint();
        });
    }
    pub fn show(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        image: &Image,
        language: diamond_core::Language,
    ) {
        if !ui.is_rect_visible(rect) {
            return;
        }
        self.visible.insert(image.target.clone());
        let remote = image.target.starts_with("https://") || image.target.starts_with("http://");
        if !remote || self.requested.contains(&image.target) {
            self.start(ui.ctx(), &image.target);
        }
        let response = ui.interact(
            rect,
            ui.id().with(("image", &image.target)),
            egui::Sense::click(),
        );
        response
            .clone()
            .on_hover_text(format!("{}\n{}", image.title, image.target));
        match self.slots.get(&image.target) {
            Some(Slot::Ready(texture)) => {
                let [w, h] = texture.size();
                let scale = (rect.width() / w as f32).min(rect.height() / h as f32);
                let fitted = Rect::from_center_size(
                    rect.center(),
                    egui::vec2(w as f32 * scale, h as f32 * scale),
                );
                ui.painter().image(
                    texture.id(),
                    fitted,
                    Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            status => {
                if matches!(status, Some(Slot::Error(_))) {
                    unreadable(ui, rect, language);
                } else {
                    ui.painter()
                        .rect_filled(rect, 10.0, ui.visuals().faint_bg_color);
                }
                let label = match status {
                    Some(Slot::Loading(_)) => language.text("Loading image…"),
                    Some(Slot::Error(_)) => "",
                    _ => language.text("Load image"),
                };
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::TextStyle::Button.resolve(ui.style()),
                    ui.visuals().text_color(),
                );
                if let Some(Slot::Error(error)) = status {
                    response.clone().on_hover_text(error);
                }
                if response.clicked() {
                    response.request_focus();
                    self.requested.insert(image.target.clone());
                    self.slots.remove(&image.target);
                    self.start(ui.ctx(), &image.target);
                }
            }
        }
    }
}
pub(crate) fn decode(bytes: &[u8]) -> Result<ColorImage, String> {
    if bytes.len() > MAX_BYTES {
        return Err("Image exceeds 16 MiB".into());
    }
    let svg = std::str::from_utf8(bytes).ok().is_some_and(|text| {
        text.trim_start_matches('\u{feff}')
            .trim_start()
            .starts_with('<')
    });
    if svg {
        let mut options = resvg::usvg::Options::default();
        options.image_href_resolver.resolve_string = Box::new(|_, _| None);
        options.image_href_resolver.resolve_data = Box::new(|_, _, _| None);
        let tree =
            resvg::usvg::Tree::from_data(bytes, &options).map_err(|error| error.to_string())?;
        let size = tree.size();
        if !size.width().is_finite()
            || !size.height().is_finite()
            || size.width() <= 0.0
            || size.height() <= 0.0
        {
            return Err("Invalid SVG dimensions".into());
        }
        let scale = (1200.0 / size.width()).min(1200.0 / size.height()).min(1.0);
        let w = ((size.width() * scale).ceil() as u32).clamp(1, 1200);
        let h = ((size.height() * scale).ceil() as u32).clamp(1, 1200);
        let mut pixels = resvg::tiny_skia::Pixmap::new(w, h).ok_or("Cannot allocate image")?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixels.as_mut(),
        );
        return Ok(ColorImage::from_rgba_premultiplied(
            [w as usize, h as usize],
            pixels.data(),
        ));
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(96 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|error| error.to_string())?
        .thumbnail(1200, 1200)
        .to_rgba8();
    Ok(ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    ))
}
#[cfg(test)]
mod tests {
    #[test]
    fn invalid_images_fail_without_lossy_text_conversion() {
        assert!(super::decode(b"not an image").is_err());
    }
}

/// Bounded hatch geometry: never allocate according to the source image size.
pub fn unreadable(ui: &Ui, rect: Rect, language: diamond_core::Language) {
    let rect = Rect::from_center_size(
        rect.center(),
        egui::Vec2::splat(rect.width().min(rect.height()).min(320.0)),
    );
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(rect, 10.0, ui.visuals().faint_bg_color);
    let mut x = rect.left() - rect.height();
    while x < rect.right() {
        painter.line_segment(
            [
                egui::pos2(x, rect.bottom()),
                egui::pos2(x + rect.height(), rect.top()),
            ],
            egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );
        x += 12.0;
    }
    let text = language.text("Image cannot be read");
    let galley = painter.layout_no_wrap(
        text.into(),
        egui::TextStyle::Button.resolve(ui.style()),
        ui.visuals().text_color(),
    );
    let label = Rect::from_center_size(rect.center(), galley.size() + egui::vec2(20.0, 16.0));
    painter.rect_filled(label, 8.0, ui.visuals().extreme_bg_color);
    painter.galley(
        label.center() - galley.size() * 0.5,
        galley,
        ui.visuals().text_color(),
    );
}

#[cfg(test)]
mod svg_tests {
    use super::decode;
    #[test]
    fn vector_paths_render_with_xml_bom_and_long_comments() {
        let body = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="12"><rect width="24" height="12" fill="#ff0000"/></svg>"##;
        let documents = [
            body.to_owned(),
            format!("<?xml version=\"1.0\"?>\n{body}"),
            format!("\u{feff}<!--{}-->\n{body}", "comment ".repeat(300)),
        ];
        for source in documents {
            let image = decode(source.as_bytes()).unwrap();
            assert_eq!(image.size, [24, 12]);
            assert!(
                image
                    .pixels
                    .iter()
                    .any(|pixel| pixel.r() > 200 && pixel.a() == 255)
            );
        }
    }
    #[test]
    fn vector_viewport_is_scaled_before_allocating_a_bitmap() {
        let svg=br##"<svg xmlns="http://www.w3.org/2000/svg" width="10000" height="5000"><rect width="10000" height="5000" fill="#123456"/></svg>"##;
        let image = decode(svg).unwrap();
        assert_eq!(image.size, [1200, 600]);
        assert!(decode(b"<svg broken").is_err());
    }
}
