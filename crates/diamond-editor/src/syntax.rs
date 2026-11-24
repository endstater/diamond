// SPDX-License-Identifier: GPL-3.0-only
//! Cached grammar-based highlighting shared by source buffers and fenced blocks.
use egui::{Color32, Context};
use std::ops::Range;
use std::sync::{Arc, OnceLock};
use syntect::{
    easy::HighlightLines,
    highlighting::{FontStyle, ThemeSet},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

#[derive(Clone)]
pub struct Span {
    pub bytes: Range<usize>,
    pub scalars: Range<usize>,
    pub color: Color32,
    pub italic: bool,
    pub bold: bool,
    pub underline: bool,
}
#[derive(Clone)]
struct Entry {
    text: String,
    language: String,
    dark: bool,
    spans: Arc<Vec<Span>>,
}
#[derive(Clone, Default)]
struct Cache(Vec<Entry>);
static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
static THEMES: OnceLock<ThemeSet> = OnceLock::new();

pub fn spans(ctx: &Context, text: &str, language: &str, dark: bool) -> Arc<Vec<Span>> {
    if text.len() > 512 * 1024 || text.lines().any(|line| line.len() > 16384) {
        return Arc::new(vec![Span {
            bytes: 0..text.len(),
            scalars: 0..text.chars().count(),
            color: if dark {
                Color32::LIGHT_GRAY
            } else {
                Color32::DARK_GRAY
            },
            italic: false,
            bold: false,
            underline: false,
        }]);
    }
    let id = egui::Id::new("diamond-syntax-cache");
    if let Some(hit) = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Cache>(id)
            .0
            .iter()
            .find(|entry| entry.dark == dark && entry.language == language && entry.text == text)
            .map(|entry| entry.spans.clone())
    }) {
        return hit;
    }
    let syntaxes = SYNTAXES.get_or_init(two_face::syntax::extra_newlines);
    let themes = THEMES.get_or_init(ThemeSet::load_defaults);
    let requested = language;
    let language = language.split_whitespace().next().unwrap_or("txt");
    let language = match language.to_ascii_lowercase().as_str() {
        "js" => "javascript",
        "jsx" => "jsx",
        "ts" => "typescript",
        "tsx" => "tsx",
        "py" => "python",
        "rs" => "rust",
        "c++" | "cxx" | "cc" | "hpp" => "cpp",
        "sh" | "zsh" | "shell" => "bash",
        "yml" => "yaml",
        "dockerfile" => "Dockerfile",
        "makefile" => "Makefile",
        _ => language,
    };
    let syntax = syntaxes
        .find_syntax_by_name(language)
        .or_else(|| syntaxes.find_syntax_by_token(language))
        .or_else(|| syntaxes.find_syntax_by_extension(language))
        .or_else(|| {
            if matches!(language, "typescript" | "tsx" | "ts") {
                syntaxes.find_syntax_by_extension("js")
            } else {
                None
            }
        })
        .unwrap_or_else(|| syntaxes.find_syntax_plain_text());
    let theme = &themes.themes[if dark {
        "base16-ocean.dark"
    } else {
        "InspiredGitHub"
    }];
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut spans = Vec::new();
    let mut byte = 0;
    let mut scalar = 0;
    for line in LinesWithEndings::from(text) {
        match highlighter.highlight_line(line, syntaxes) {
            Ok(runs) => {
                for (style, fragment) in runs {
                    let count = fragment.chars().count();
                    spans.push(Span {
                        bytes: byte..byte + fragment.len(),
                        scalars: scalar..scalar + count,
                        color: Color32::from_rgb(
                            style.foreground.r,
                            style.foreground.g,
                            style.foreground.b,
                        ),
                        italic: style.font_style.contains(FontStyle::ITALIC),
                        bold: style.font_style.contains(FontStyle::BOLD),
                        underline: style.font_style.contains(FontStyle::UNDERLINE),
                    });
                    byte += fragment.len();
                    scalar += count;
                }
            }
            Err(_) => {
                let count = line.chars().count();
                spans.push(Span {
                    bytes: byte..byte + line.len(),
                    scalars: scalar..scalar + count,
                    color: if dark {
                        Color32::LIGHT_GRAY
                    } else {
                        Color32::DARK_GRAY
                    },
                    italic: false,
                    bold: false,
                    underline: false,
                });
                byte += line.len();
                scalar += count;
            }
        }
    }
    let spans = Arc::new(spans);
    ctx.data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<Cache>(id);
        // Bound both source snapshots and decoded runs; large buffers stay transient.
        if text.len() <= 512 * 1024 {
            if cache.0.len() >= 12 {
                cache.0.remove(0);
            }
            cache.0.push(Entry {
                text: text.into(),
                language: requested.into(),
                dark,
                spans: spans.clone(),
            });
        }
    });
    spans
}

pub fn layout(
    ctx: &Context,
    text: &str,
    language: &str,
    size: f32,
    dark: bool,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    for span in spans(ctx, text, language, dark).iter() {
        let mut format = egui::TextFormat::simple(egui::FontId::monospace(size), span.color);
        format.italics = span.italic;
        if span.bold {
            format.extra_letter_spacing = 0.25;
        }
        if span.underline {
            format.underline = egui::Stroke::new(1.0, span.color);
        }
        job.append(&text[span.bytes.clone()], 0.0, format);
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiline_grammars_preserve_unicode_and_cover_all_bytes() {
        let ctx = Context::default();
        for (language, text) in [
            (
                "rs",
                "/* много\nстрок */\nfn main() { let x = \"你好\"; }\n",
            ),
            ("py", "# comment\ndef f():\n    return 'мир'\n"),
            ("html", "<div class=\"x\">你好</div>"),
        ] {
            let runs = spans(&ctx, text, language, true);
            let reconstructed: String = runs.iter().map(|run| &text[run.bytes.clone()]).collect();
            assert_eq!(reconstructed, text);
            assert!(
                runs.iter()
                    .map(|run| run.color)
                    .any(|color| color != runs[0].color)
            );
            assert!(Arc::ptr_eq(&runs, &spans(&ctx, text, language, true)));
        }
    }
}

#[cfg(test)]
mod extra_language_tests {
    #[test]
    fn modern_language_grammars_are_real_definitions() {
        let syntaxes = two_face::syntax::extra_newlines();
        for extension in ["ts", "tsx", "typ", "kt", "toml", "swift", "zig"] {
            assert!(
                syntaxes.find_syntax_by_extension(extension).is_some(),
                "Missing {extension}"
            );
        }
    }
}

/// One retained source galley per editor; no token/job reconstruction on redraw.
#[derive(Default)]
pub(super) struct LayoutCache(Option<CachedLayout>);
struct CachedLayout {
    source: String,
    style: LayoutStyle,
    matches: Vec<Range<usize>>,
    active: Option<Range<usize>>,
    galley: Arc<egui::Galley>,
}
#[derive(PartialEq)]
struct LayoutStyle {
    font: egui::FontId,
    language: String,
    width: u32,
    wrap: bool,
    dark: bool,
    accent: Color32,
    epoch: u64,
    scale: u32,
}
pub(super) struct LayoutOptions<'a> {
    pub language: &'a str,
    pub size: f32,
    pub width: f32,
    pub wrap: bool,
    pub family: &'a egui::FontFamily,
    pub matches: &'a [Range<usize>],
    pub active: Option<&'a Range<usize>>,
    pub accent: Color32,
}
impl LayoutCache {
    pub fn layout(
        &mut self,
        ui: &egui::Ui,
        text: &str,
        options: &LayoutOptions<'_>,
    ) -> Arc<egui::Galley> {
        let style = LayoutStyle {
            font: egui::FontId::new(options.size, options.family.clone()),
            language: options.language.to_owned(),
            width: if options.wrap {
                options.width.to_bits()
            } else {
                0
            },
            wrap: options.wrap,
            dark: ui.visuals().dark_mode,
            accent: options.accent,
            epoch: super::font_epoch(ui.ctx()),
            scale: ui.ctx().pixels_per_point().to_bits(),
        };
        if let Some(hit) = self.0.as_ref().filter(|hit| {
            hit.style == style
                && hit.source == text
                && hit.matches.as_slice() == options.matches
                && hit.active.as_ref() == options.active
        }) {
            return Arc::clone(&hit.galley);
        }
        let mut job = layout(ui.ctx(), text, options.language, options.size, style.dark);
        for section in &mut job.sections {
            section.format.font_id.family = options.family.clone();
        }
        if options.wrap {
            job.wrap.max_width = options.width;
        }
        super::emoji::decorate_job(ui.ctx(), &mut job);
        let job = super::search::highlight(job, options.matches, options.active, options.accent);
        let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
        // Large buffers retain the existing fallback, without another unbounded snapshot.
        self.0 = (text.len() <= 2 * 1024 * 1024).then(|| CachedLayout {
            source: text.to_owned(),
            style,
            matches: options.matches.to_vec(),
            active: options.active.cloned(),
            galley: Arc::clone(&galley),
        });
        galley
    }
}

#[cfg(test)]
mod layout_cache_tests {
    use super::*;
    #[test]
    fn source_galley_is_shared_until_text_or_find_changes() {
        let ctx = egui::Context::default();
        let mut cache = LayoutCache::default();
        let family = egui::FontFamily::Monospace;
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut options = LayoutOptions {
                    language: "rs",
                    size: 16.0,
                    width: 500.0,
                    wrap: true,
                    family: &family,
                    matches: &[],
                    active: None,
                    accent: Color32::LIGHT_BLUE,
                };
                let first = cache.layout(ui, "let x = 1;", &options);
                let second = cache.layout(ui, "let x = 1;", &options);
                assert!(Arc::ptr_eq(&first, &second));
                let changed = cache.layout(ui, "let x = 2;", &options);
                assert!(!Arc::ptr_eq(&first, &changed));
                let search_matches = vec![4..5];
                options.matches = &search_matches;
                let found = cache.layout(ui, "let x = 2;", &options);
                assert!(!Arc::ptr_eq(&changed, &found));
            });
        });
    }
}
