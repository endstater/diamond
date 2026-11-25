// SPDX-License-Identifier: GPL-3.0-only
//! Native Mermaid -> SVG -> texture on one bounded, blocking-receive worker.
use egui::{Context, TextureHandle, Vec2};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock, mpsc};

const MAX_SOURCE: usize = 16 * 1024;
const MAX_ENTRIES: usize = 64;
const MAX_TEXTURE_BYTES: usize = 64 * 1024 * 1024;
#[derive(Clone)]
pub(super) struct Diagram {
    pub texture: Option<TextureHandle>,
    pub size: Vec2,
    pub error: Option<String>,
}
impl Diagram {
    pub fn display_size(&self, width: f32, size: f32) -> Vec2 {
        let natural = self.size * (size / 16.0);
        natural
            * (width.max(1.0) / natural.x.max(1.0))
                .min(900.0 / natural.y.max(1.0))
                .min(1.0)
    }
}
struct Entry {
    source: Arc<str>,
    result: Option<Arc<Diagram>>,
    used: u64,
    cost: usize,
}
#[derive(Default)]
struct Cache {
    entries: HashMap<(u64, bool), Entry>,
    clock: u64,
    bytes: usize,
}
impl Cache {
    fn prune(&mut self, keep: (u64, bool)) {
        while self.entries.len() > MAX_ENTRIES || self.bytes > MAX_TEXTURE_BYTES {
            let oldest = self
                .entries
                .iter()
                .filter(|(key, entry)| **key != keep && entry.result.is_some())
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| *key);
            let Some(oldest) = oldest else {
                break;
            };
            if let Some(entry) = self.entries.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(entry.cost);
            }
        }
    }
}
struct Job {
    key: (u64, bool),
    source: Arc<str>,
    ctx: Context,
}
struct Renderer {
    cache: Arc<Mutex<Cache>>,
    jobs: mpsc::SyncSender<Job>,
}
fn renderer(ctx: &Context, create: bool) -> Option<Arc<Renderer>> {
    let id = egui::Id::new("native-mermaid-renderer");
    if let Some(renderer) = ctx.data(|data| data.get_temp::<Arc<Renderer>>(id)) {
        return Some(renderer);
    }
    if !create {
        return None;
    }
    let cache = Arc::new(Mutex::new(Cache::default()));
    let (sender, receiver) = mpsc::sync_channel::<Job>(16);
    let worker_cache = Arc::clone(&cache);
    std::thread::spawn(move || {
        while let Ok(job) = receiver.recv() {
            let rendered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                render(&job.ctx, &job.source, job.key.1)
            }))
            .unwrap_or_else(|_| Err("Mermaid renderer failed; edit the source to retry".into()));
            let diagram = Arc::new(rendered.unwrap_or_else(error_diagram));
            let cost = diagram.texture.as_ref().map_or(0, |texture| {
                let [w, h] = texture.size();
                w * h * 4
            });
            {
                let mut cache = worker_cache.lock().unwrap_or_else(|e| e.into_inner());
                let previous = cache
                    .entries
                    .get_mut(&job.key)
                    .filter(|entry| entry.source == job.source)
                    .map(|entry| {
                        let previous = entry.cost;
                        entry.result = Some(diagram);
                        entry.cost = cost;
                        previous
                    });
                if let Some(previous) = previous {
                    cache.bytes = cache.bytes.saturating_sub(previous) + cost;
                }
                cache.prune(job.key);
            }
            job.ctx.request_repaint();
        }
    });
    let renderer = Arc::new(Renderer {
        cache,
        jobs: sender,
    });
    ctx.data_mut(|data| data.insert_temp(id, Arc::clone(&renderer)));
    Some(renderer)
}
fn error_diagram(error: String) -> Diagram {
    Diagram {
        texture: None,
        size: egui::vec2(400.0, 60.0),
        error: Some(error),
    }
}
pub(super) fn key(source: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}
fn rejected(source: &str) -> Option<Arc<Diagram>> {
    if source.len() <= MAX_SOURCE && source.lines().take(257).count() <= 256 {
        return None;
    }
    static ERROR: OnceLock<Arc<Diagram>> = OnceLock::new();
    Some(Arc::clone(ERROR.get_or_init(|| {
        Arc::new(error_diagram("Mermaid limit: 16 KiB and 256 lines".into()))
    })))
}
pub(super) fn peek(ctx: &Context, key: u64, source: &str, dark: bool) -> Option<Arc<Diagram>> {
    if let Some(error) = rejected(source) {
        return Some(error);
    }
    let renderer = renderer(ctx, false)?;
    let cache = renderer.cache.lock().unwrap_or_else(|e| e.into_inner());
    cache
        .entries
        .get(&(key, dark))
        .filter(|entry| entry.source.as_ref() == source)
        .and_then(|entry| entry.result.clone())
}
pub(super) fn request(ctx: &Context, key: u64, source: &str, dark: bool) -> Option<Arc<Diagram>> {
    if let Some(error) = rejected(source) {
        return Some(error);
    }
    let renderer = renderer(ctx, true)?;
    let key = (key, dark);
    let mut cache = renderer.cache.lock().unwrap_or_else(|e| e.into_inner());
    cache.clock = cache.clock.wrapping_add(1);
    let used = cache.clock;
    if let Some(entry) = cache
        .entries
        .get_mut(&key)
        .filter(|entry| entry.source.as_ref() == source)
    {
        entry.used = used;
        return entry.result.clone();
    }
    let source: Arc<str> = source.into();
    let result: Option<Arc<Diagram>> = None;
    if let Some(old) = cache.entries.remove(&key) {
        cache.bytes = cache.bytes.saturating_sub(old.cost);
    }
    cache.entries.insert(
        key,
        Entry {
            source: Arc::clone(&source),
            result: result.clone(),
            used,
            cost: 0,
        },
    );
    cache.prune(key);
    drop(cache);
    if renderer
        .jobs
        .try_send(Job {
            key,
            source,
            ctx: ctx.clone(),
        })
        .is_err()
    {
        let result = Arc::new(error_diagram(
            "Mermaid render queue is full; edit to retry".into(),
        ));
        let mut cache = renderer.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = cache.entries.get_mut(&key) {
            entry.result = Some(Arc::clone(&result));
        }
        return Some(result);
    }
    None
}

fn render(ctx: &Context, source: &str, dark: bool) -> Result<Diagram, String> {
    let parsed = mermaid_rs_renderer::parse_mermaid_strict(source).map_err(|e| e.to_string())?;
    if parsed.graph.nodes.len() > 128 || parsed.graph.edges.len() > 256 {
        return Err("Mermaid limit: 128 nodes and 256 edges".into());
    }
    let mut theme = if dark {
        mermaid_rs_renderer::Theme::dark()
    } else {
        mermaid_rs_renderer::Theme::neutral()
    };
    theme.font_family = "sans-serif".into();
    theme.background = if dark { "#1a1a1a" } else { "#ededed" }.into();
    let config = mermaid_rs_renderer::LayoutConfig::default();
    let layout = mermaid_rs_renderer::compute_layout(&parsed.graph, &theme, &config);
    let svg = mermaid_rs_renderer::render_svg(&layout, &theme, &config);
    if svg.len() > 4 * 1024 * 1024 {
        return Err("Mermaid SVG exceeds 4 MiB".into());
    }
    static FONTS: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    let fontdb = FONTS.get_or_init(|| {
        let mut fonts = resvg::usvg::fontdb::Database::new();
        for data in egui::FontDefinitions::default().font_data.values() {
            fonts.load_font_data(data.font.to_vec());
        }
        fonts.load_system_fonts();
        fonts.set_sans_serif_family("Ubuntu");
        Arc::new(fonts)
    });
    let mut options = resvg::usvg::Options {
        fontdb: Arc::clone(fontdb),
        font_family: "sans-serif".into(),
        ..Default::default()
    };
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    options.image_href_resolver.resolve_data = Box::new(|_, _, _| None);
    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &options).map_err(|e| e.to_string())?;
    let size = egui::vec2(tree.size().width(), tree.size().height());
    if !size.x.is_finite()
        || !size.y.is_finite()
        || size.x <= 0.0
        || size.y <= 0.0
        || size.x > 16384.0
        || size.y > 16384.0
    {
        return Err("Mermaid canvas is too large".into());
    }
    let scale = 2.0_f32.min(2048.0 / size.x).min(2048.0 / size.y);
    let width = (size.x * scale).ceil().clamp(1.0, 2048.0) as u32;
    let height = (size.y * scale).ceil().clamp(1.0, 2048.0) as u32;
    let mut pixels =
        resvg::tiny_skia::Pixmap::new(width, height).ok_or("Cannot allocate Mermaid image")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixels.as_mut(),
    );
    let image =
        egui::ColorImage::from_rgba_premultiplied([width as usize, height as usize], pixels.data());
    Ok(Diagram {
        texture: Some(ctx.load_texture("mermaid-diagram", image, egui::TextureOptions::LINEAR)),
        size,
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_flow_sequence_class_and_unicode_render() {
        let ctx = Context::default();
        for source in [
            "flowchart LR\nA[Начало] --> B[Готово]",
            "sequenceDiagram\nAlice->>Bob: Hello",
            "classDiagram\nAnimal <|-- Duck",
        ] {
            let result = render(&ctx, source, false).unwrap();
            assert!(result.texture.is_some());
            assert!(result.size.x > 0.0);
        }
    }
    #[test]
    fn oversized_source_does_not_create_a_worker_or_cache_copy() {
        let ctx = Context::default();
        let source = "x".repeat(MAX_SOURCE + 1);
        let result = request(&ctx, key(&source), &source, false).unwrap();
        assert!(result.error.is_some());
        assert!(renderer(&ctx, false).is_none());
    }
    #[test]
    fn cache_evicts_old_completed_entries_and_keeps_the_visible_result() {
        let mut cache = Cache::default();
        for n in 0..70u64 {
            cache.entries.insert(
                (n, false),
                Entry {
                    source: "x".into(),
                    result: Some(Arc::new(error_diagram("test".into()))),
                    used: n,
                    cost: 0,
                },
            );
        }
        cache.prune((0, false));
        assert!(cache.entries.len() <= MAX_ENTRIES);
        assert!(cache.entries.contains_key(&(0, false)));
        assert!(!cache.entries.contains_key(&(1, false)));
    }
    #[test]
    fn invalid_source_fails_and_display_is_bounded() {
        assert!(render(&Context::default(), "not a diagram", false).is_err());
        let diagram = Diagram {
            texture: None,
            size: egui::vec2(16000.0, 8000.0),
            error: None,
        };
        let size = diagram.display_size(500.0, 16.0);
        assert!(size.x <= 500.0 && size.y <= 900.0);
    }
}
