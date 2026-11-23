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
    let syntaxes = SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines);
    let themes = THEMES.get_or_init(ThemeSet::load_defaults);
    let requested = language;
    let language = language.split_whitespace().next().unwrap_or("txt");
    let language = match language.to_ascii_lowercase().as_str() {
        "js" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "py" => "python",
        "rs" => "rust",
        "c++" | "cxx" | "cc" | "hpp" => "cpp",
        "sh" | "zsh" | "shell" => "bash",
        "yml" => "yaml",
        _ => language,
    };
    let syntax = syntaxes
        .find_syntax_by_token(language)
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
