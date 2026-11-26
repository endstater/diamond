// SPDX-License-Identifier: GPL-3.0-only
use diamond_core::{AppSettings, ThemeMode};
use egui::{Color32, Context, FontDefinitions, FontFamily};
use std::path::PathBuf;

pub struct Palette {
    pub accent: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub background: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub border: Color32,
}

impl Palette {
    pub fn of(ctx: &Context) -> Self {
        let dark = ctx.theme() == egui::Theme::Dark;
        Self {
            accent: ctx.style().visuals.hyperlink_color,
            text: Color32::from_gray(if dark { 226 } else { 32 }),
            muted: Color32::from_gray(if dark { 148 } else { 100 }),
            background: Color32::from_gray(if dark { 17 } else { 250 }),
            panel: Color32::from_gray(if dark { 22 } else { 244 }),
            raised: Color32::from_gray(if dark { 30 } else { 235 }),
            border: Color32::from_gray(if dark { 43 } else { 215 }),
        }
    }
}

pub fn configure(ctx: &Context, settings: &AppSettings) {
    ctx.set_theme(match settings.theme {
        ThemeMode::Dark => egui::ThemePreference::Dark,
        ThemeMode::Light => egui::ThemePreference::Light,
        ThemeMode::System => egui::ThemePreference::System,
    });
    for dark in [true, false] {
        let mut visuals = if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        let gray = |a, b| Color32::from_gray(if dark { a } else { b });
        let accent = if !dark && settings.accent == [224, 224, 224] {
            Color32::from_gray(65)
        } else {
            Color32::from_rgb(settings.accent[0], settings.accent[1], settings.accent[2])
        };
        visuals.panel_fill = gray(22, 244);
        visuals.window_fill = gray(25, 248);
        visuals.extreme_bg_color = gray(17, 250);
        visuals.faint_bg_color = gray(30, 235);
        visuals.code_bg_color = gray(28, 232);
        visuals.override_text_color = None;
        // A steady caret lets an idle editor sleep instead of waking to blink.
        visuals.text_cursor.blink = false;
        visuals.weak_text_color = Some(gray(148, 100));
        visuals.selection.bg_fill = accent;
        visuals.selection.stroke = egui::Stroke::new(1.0, gray(17, 250));
        visuals.hyperlink_color = accent;
        visuals.warn_fg_color = gray(208, 65);
        visuals.error_fg_color = gray(238, 35);
        visuals.window_corner_radius = egui::CornerRadius::same(14);
        visuals.menu_corner_radius = egui::CornerRadius::same(12);
        visuals.window_stroke = egui::Stroke::new(1.0, gray(52, 205));
        for (widget, fill, stroke, foreground) in [
            (
                &mut visuals.widgets.noninteractive,
                gray(25, 244),
                gray(43, 215),
                gray(226, 32),
            ),
            (
                &mut visuals.widgets.inactive,
                gray(31, 235),
                gray(48, 210),
                gray(218, 35),
            ),
            (
                &mut visuals.widgets.hovered,
                gray(43, 223),
                gray(66, 190),
                gray(242, 15),
            ),
            (
                &mut visuals.widgets.active,
                gray(54, 209),
                accent,
                gray(250, 5),
            ),
            (
                &mut visuals.widgets.open,
                gray(43, 223),
                gray(66, 190),
                gray(242, 15),
            ),
        ] {
            widget.bg_fill = fill;
            widget.weak_bg_fill = fill;
            widget.bg_stroke = egui::Stroke::new(1.0, stroke);
            widget.fg_stroke = egui::Stroke::new(1.0, foreground);
            widget.corner_radius = egui::CornerRadius::same(10);
            widget.expansion = 0.0;
        }
        ctx.set_visuals_of(
            if dark {
                egui::Theme::Dark
            } else {
                egui::Theme::Light
            },
            visuals,
        );
    }
    ctx.all_styles_mut(|style| {
        style.animation_time = if settings.reduced_motion { 0.0 } else { 0.12 };
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 8.0);
        style.spacing.interact_size = egui::vec2(32.0, 32.0);
        style.spacing.window_margin = egui::Margin::symmetric(20, 18);
        style.spacing.menu_margin = egui::Margin::same(8);
        for (kind, size) in [
            (egui::TextStyle::Body, 14.0),
            (egui::TextStyle::Button, 14.0),
            (egui::TextStyle::Small, 12.0),
            (egui::TextStyle::Heading, 22.0),
        ] {
            style.text_styles.insert(
                kind,
                egui::FontId::new(
                    size * settings.ui_font_size.clamp(12.0, 24.0) / 14.0,
                    FontFamily::Name("diamond-ui".into()),
                ),
            );
        }
    });
}

#[derive(Clone)]
pub struct SystemFont {
    pub name: String,
    pub path: PathBuf,
    pub index: u32,
    pub monospace: bool,
}
fn supported_font(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "ttf" | "otf" | "ttc" | "otc"
            )
        })
}
fn spacing_is_mono(value: &str) -> bool {
    value
        .split(',')
        .any(|part| matches!(part.trim(), "100" | "110"))
}
fn parse_system_fonts(value: &str) -> Vec<SystemFont> {
    let mut fonts: Vec<_> = value
        .lines()
        .filter_map(|line| {
            let parts: Vec<_> = line.split('\t').collect();
            if parts.len() != 5 {
                return None;
            }
            let path = PathBuf::from(parts[2]);
            if parts[0].is_empty() || !supported_font(&path) {
                return None;
            }
            Some(SystemFont {
                name: format!("{} · {}", parts[0], parts[1]),
                path,
                index: parts[3].parse::<u32>().ok()? & 0xffff,
                monospace: spacing_is_mono(parts[4]),
            })
        })
        .collect();
    fonts.sort_by(|a, b| a.name.cmp(&b.name));
    fonts.dedup_by(|a, b| a.name == b.name);
    fonts
}
pub fn system_fonts() -> Vec<SystemFont> {
    let Ok(output) = std::process::Command::new("fc-list")
        .args([
            "-f",
            "%{family[0]}\t%{style[0]}\t%{file}\t%{index}\t%{spacing}\n",
        ])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() || output.stdout.len() > 2 * 1024 * 1024 {
        return Vec::new();
    }
    parse_system_fonts(&String::from_utf8_lossy(&output.stdout))
}
fn matched_font(pattern: &str) -> Option<(PathBuf, u32)> {
    let output = std::process::Command::new("fc-match")
        .args(["-s", "-f", "%{file}\t%{index}\n", pattern])
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.len() > 2 * 1024 * 1024 {
        return None;
    }
    std::str::from_utf8(&output.stdout)
        .ok()?
        .lines()
        .find_map(|line| {
            let (path, index) = line.split_once('\t')?;
            let path = PathBuf::from(path);
            supported_font(&path).then_some((path, index.parse::<u32>().ok()? & 0xffff))
        })
}
fn font_properties(path: &std::path::Path, index: u32) -> Option<String> {
    let output = std::process::Command::new("fc-query")
        .args([
            "-i",
            &index.to_string(),
            "-f",
            "%{spacing}\n%{charset}\n",
            "--",
        ])
        .arg(path)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.len() > 512 * 1024 {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}
fn has_character(properties: &str, character: u32) -> bool {
    properties
        .lines()
        .nth(1)
        .unwrap_or("")
        .split_whitespace()
        .any(|entry| {
            let (start, end) = entry.split_once('-').unwrap_or((entry, entry));
            match (u32::from_str_radix(start, 16), u32::from_str_radix(end, 16)) {
                (Ok(start), Ok(end)) => start <= character && character <= end,
                _ => false,
            }
        })
}

fn load_font(path: &std::path::Path, index: u32) -> Option<egui::FontData> {
    use std::io::Read;
    const LIMIT: u64 = 64 * 1024 * 1024;
    if !supported_font(path) {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > LIMIT {
        return None;
    }
    let mut data = egui::FontData::from_owned(bytes);
    data.index = index;
    Some(data)
}

struct FallbackFont {
    name: String,
    data: std::sync::Arc<egui::FontData>,
}
fn fallbacks() -> &'static Vec<FallbackFont> {
    static FONTS: std::sync::OnceLock<Vec<FallbackFont>> = std::sync::OnceLock::new();
    FONTS.get_or_init(|| {
        let mut result = Vec::new();
        let mut loaded = Vec::new();
        let mut bytes = 0;
        // Fontconfig and file reads run once, never in layout or on keystrokes.
        for (pattern, character) in [
            ("monospace:charset=2501", 0x2501),
            ("sans-serif:charset=2192", 0x2192),
            ("Noto Sans Symbols 2:charset=23f5", 0x23f5),
            ("sans-serif:charset=2211", 0x2211),
            ("sans-serif:lang=zh-cn:charset=4e2d", 0x4e2d),
            ("sans-serif:charset=627", 0x627),
            ("sans-serif:charset=905", 0x905),
        ] {
            let Some((path, index)) = matched_font(pattern) else {
                continue;
            };
            if loaded.contains(&(path.clone(), index)) {
                continue;
            }
            if !font_properties(&path, index)
                .is_some_and(|properties| has_character(&properties, character))
            {
                continue;
            }
            if let Some(data) = load_font(&path, index) {
                if bytes + data.font.len() > 96 * 1024 * 1024 {
                    continue;
                }
                bytes += data.font.len();
                let name = format!("diamond-fallback-{}", result.len());
                result.push(FallbackFont {
                    name,
                    data: std::sync::Arc::new(data),
                });
                loaded.push((path, index));
            }
        }
        result
    })
}
fn brand_font() -> Option<std::sync::Arc<egui::FontData>> {
    static FONT: std::sync::OnceLock<Option<std::sync::Arc<egui::FontData>>> =
        std::sync::OnceLock::new();
    FONT.get_or_init(|| {
        let (path, index) = matched_font("Montserrat:style=Bold")?;
        load_font(&path, index).map(std::sync::Arc::new)
    })
    .clone()
}
pub fn install_font(ctx: &Context, settings: &AppSettings) -> Option<String> {
    let mut fonts = FontDefinitions::default();
    install_color_emoji();
    let mut error = None;
    // Keep both bundled families in each fallback chain, then system coverage.
    let proportional = fonts.families[&FontFamily::Proportional].clone();
    let mono = fonts.families[&FontFamily::Monospace].clone();
    for family in fonts.families.values_mut() {
        for name in proportional.iter().chain(mono.iter()) {
            if !family.contains(name) {
                family.push(name.clone());
            }
        }
        for font in fallbacks() {
            family.push(font.name.clone());
        }
    }
    for font in fallbacks() {
        fonts
            .font_data
            .insert(font.name.clone(), std::sync::Arc::clone(&font.data));
    }
    for (name, path, index, key) in [
        (
            "diamond-editor",
            &settings.editor_font_path,
            settings.editor_font_index,
            &settings.editor_font,
        ),
        (
            "diamond-ui",
            &settings.ui_font_path,
            settings.ui_font_index,
            &settings.ui_font,
        ),
        (
            "diamond-code",
            &settings.code_font_path,
            settings.code_font_index,
            &settings.code_font,
        ),
    ] {
        let base = if name == "diamond-code" || key == "mono" {
            FontFamily::Monospace
        } else {
            FontFamily::Proportional
        };
        let mut family = fonts.families[&base].clone();
        if let Some(path) = path {
            let allowed = name != "diamond-code"
                || font_properties(path, index).is_some_and(|properties| {
                    spacing_is_mono(properties.lines().next().unwrap_or(""))
                });
            if !allowed {
                error = Some(
                    settings
                        .language
                        .text("Source font must be monospaced; using the default font.")
                        .into(),
                );
            } else if let Some(data) = load_font(path, index) {
                fonts
                    .font_data
                    .insert(name.into(), std::sync::Arc::new(data));
                family.insert(0, name.into());
            } else {
                error = Some(
                    settings
                        .language
                        .text("Unable to load selected font; using the default font.")
                        .into(),
                );
            }
        }
        fonts
            .families
            .insert(FontFamily::Name(name.into()), family.clone());
        if name == "diamond-code" {
            fonts.families.insert(FontFamily::Monospace, family);
        }
    }
    let mut brand = fonts.families[&FontFamily::Proportional].clone();
    if let Some(data) = brand_font() {
        fonts.font_data.insert("diamond-brand".into(), data);
        brand.insert(0, "diamond-brand".into());
    }
    fonts
        .families
        .insert(FontFamily::Name("diamond-brand".into()), brand);
    ctx.set_fonts(fonts);
    diamond_editor::invalidate_font_layouts(ctx);
    error
}

pub fn font_family(_settings: &AppSettings) -> FontFamily {
    FontFamily::Name("diamond-editor".into())
}

pub fn ui_size(ui: &egui::Ui, base: f32) -> f32 {
    base * egui::TextStyle::Body.resolve(ui.style()).size / 14.0
}

fn install_color_emoji() {
    static LOADED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    LOADED.get_or_init(|| {
        use std::io::Read;
        if let Ok(output) = std::process::Command::new("fc-match")
            .args(["-f", "%{file}", "Noto Color Emoji"])
            .output()
        {
            if output.status.success()
                && let Ok(path) = std::str::from_utf8(&output.stdout)
                && let Ok(file) = std::fs::File::open(path)
            {
                let mut bytes = Vec::new();
                if file
                    .take(32 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .is_ok()
                    && bytes.len() <= 32 * 1024 * 1024
                {
                    diamond_editor::install_emoji_font(bytes);
                }
            }
        }
    });
}

#[cfg(test)]
mod font_tests {
    use super::*;
    #[test]
    fn discovery_keeps_collection_face_and_monospace_metadata() {
        let input = "Sans\tRegular\t/fonts/sans.ttf\t0\t\nMono\tBold\t/fonts/mono.ttc\t2\t100\nWeb\tRegular\t/fonts/web.woff2\t0\t100\n";
        let fonts = parse_system_fonts(input);
        assert_eq!(fonts.len(), 2);
        let mono = fonts.iter().find(|f| f.name == "Mono · Bold").unwrap();
        assert!(mono.monospace);
        assert_eq!(mono.index, 2);
        assert!(
            !fonts
                .iter()
                .find(|f| f.name == "Sans · Regular")
                .unwrap()
                .monospace
        );
    }
    #[test]
    fn fallback_coverage_checks_actual_fontconfig_ranges() {
        assert!(has_character("100\n20-7e 2500-259f\n", 0x2501));
        assert!(!has_character("100\n20-7e 2500\n", 0x2501));
        assert!(spacing_is_mono("100"));
        assert!(spacing_is_mono("110"));
        assert!(!spacing_is_mono("90"));
        assert!(!spacing_is_mono(""));
        assert!(!supported_font(std::path::Path::new("/font.woff2")));
    }
}
