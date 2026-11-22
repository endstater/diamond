use diamond_core::{AppSettings, ThemeMode};
use egui::{Color32, Context, FontDefinitions, FontFamily};
use std::path::PathBuf;

pub struct Palette {
    pub accent: Color32, pub text: Color32, pub muted: Color32,
    pub background: Color32, pub panel: Color32, pub raised: Color32, pub border: Color32,
}

impl Palette {
    pub fn of(ctx: &Context) -> Self {
        let dark = ctx.theme() == egui::Theme::Dark;
        Self {
            accent: ctx.style().visuals.hyperlink_color,
            text: Color32::from_gray(if dark {226} else {32}),
            muted: Color32::from_gray(if dark {148} else {100}),
            background: Color32::from_gray(if dark {17} else {250}),
            panel: Color32::from_gray(if dark {22} else {244}),
            raised: Color32::from_gray(if dark {30} else {235}),
            border: Color32::from_gray(if dark {43} else {215}),
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
        let mut visuals = if dark {egui::Visuals::dark()} else {egui::Visuals::light()};
        let gray = |a,b| Color32::from_gray(if dark {a} else {b});
        let accent = if !dark && settings.accent == [224,224,224] {Color32::from_gray(65)}
            else {Color32::from_rgb(settings.accent[0],settings.accent[1],settings.accent[2])};
        visuals.panel_fill = gray(22,244);
        visuals.window_fill = gray(25,248);
        visuals.extreme_bg_color = gray(17,250);
        visuals.faint_bg_color = gray(30,235);
        visuals.code_bg_color = gray(28,232);
        visuals.override_text_color = Some(gray(226,32));
        visuals.weak_text_color = Some(gray(148,100));
        visuals.selection.bg_fill = accent.gamma_multiply(0.28);
        visuals.selection.stroke = egui::Stroke::new(1.0, gray(245,20));
        visuals.hyperlink_color = accent;
        visuals.warn_fg_color = gray(208,65);
        visuals.error_fg_color = gray(238,35);
        visuals.window_corner_radius = egui::CornerRadius::same(14);
        visuals.menu_corner_radius = egui::CornerRadius::same(12);
        visuals.window_stroke = egui::Stroke::new(1.0,gray(52,205));
        for (widget, fill, stroke, foreground) in [
            (&mut visuals.widgets.noninteractive,gray(25,244),gray(43,215),gray(148,100)),
            (&mut visuals.widgets.inactive,gray(31,235),gray(48,210),gray(218,35)),
            (&mut visuals.widgets.hovered,gray(43,223),gray(66,190),gray(242,15)),
            (&mut visuals.widgets.active,gray(54,209),accent,gray(250,5)),
            (&mut visuals.widgets.open,gray(43,223),gray(66,190),gray(242,15)),
        ] {
            widget.bg_fill=fill; widget.weak_bg_fill=fill;
            widget.bg_stroke=egui::Stroke::new(1.0,stroke);
            widget.fg_stroke=egui::Stroke::new(1.0,foreground);
            widget.corner_radius=egui::CornerRadius::same(10); widget.expansion=0.0;
        }
        ctx.set_visuals_of(if dark {egui::Theme::Dark} else {egui::Theme::Light}, visuals);
    }
    ctx.all_styles_mut(|style| {
        style.animation_time=if settings.reduced_motion {0.0} else {0.12};
        style.spacing.item_spacing=egui::vec2(8.0,8.0);
        style.spacing.button_padding=egui::vec2(12.0,8.0);
        style.spacing.interact_size=egui::vec2(32.0,32.0);
        style.spacing.window_margin=egui::Margin::symmetric(20,18);
        style.spacing.menu_margin=egui::Margin::same(8);
        for (kind,size) in [(egui::TextStyle::Body,14.0),(egui::TextStyle::Button,14.0),
            (egui::TextStyle::Small,12.0),(egui::TextStyle::Heading,22.0)] {
            style.text_styles.insert(kind,egui::FontId::new(size * settings.ui_font_size.clamp(12.0,24.0) / 14.0,FontFamily::Name("diamond-ui".into())));
        }
    });
}

#[derive(Clone)]
pub struct SystemFont { pub name:String, pub path:PathBuf }

pub fn system_fonts() -> Vec<SystemFont> {
    let Ok(output)=std::process::Command::new("fc-list").args(["-f","%{family[0]}\t%{style[0]}\t%{file}\n"]).output() else {return Vec::new()};
    if !output.status.success() || output.stdout.len()>2*1024*1024 {return Vec::new()}
    let mut fonts:Vec<_>=String::from_utf8_lossy(&output.stdout).lines().filter_map(|line| {
        let mut parts=line.splitn(3,'\t');let name=parts.next()?;let style=parts.next()?;let path=parts.next()?;
        (!name.is_empty()).then(||SystemFont{name:format!("{name} · {style}"),path:PathBuf::from(path)})
    }).collect();
    fonts.sort_by(|a,b|a.name.cmp(&b.name)); fonts.dedup_by(|a,b|a.name==b.name); fonts.truncate(256); fonts
}

/// Fontconfig discovery happens only when installing preferences, never per frame.
fn cjk_font() -> Option<(PathBuf, u32)> {
    let output = std::process::Command::new("fc-match")
        .args(["-f", "%{lang}\n%{file}\n%{index}\n", ":lang=zh-cn"]).output().ok()?;
    if !output.status.success() { return None; }
    let value = std::str::from_utf8(&output.stdout).ok()?;
    let mut lines = value.lines();
    if !lines.next()?.split('|').any(|lang| lang.starts_with("zh")) { return None; }
    Some((PathBuf::from(lines.next()?), lines.next()?.parse().ok()?))
}

fn load_font(path: &std::path::Path, index: u32) -> Option<egui::FontData> {
    use std::io::Read;
    const LIMIT: u64 = 64 * 1024 * 1024;
    let mut bytes = Vec::new();
    std::fs::File::open(path).ok()?.take(LIMIT+1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > LIMIT { return None; }
    let mut data = egui::FontData::from_owned(bytes); data.index = index; Some(data)
}

pub fn install_font(ctx:&Context,settings:&AppSettings)->Option<String> {
    let mut fonts=FontDefinitions::default();
    let mut error=None;
    if let Some((path,index))=cjk_font() {
        if let Some(data)=load_font(&path,index) {
            fonts.font_data.insert("diamond-cjk".into(),std::sync::Arc::new(data));
            for family in fonts.families.values_mut() {family.push("diamond-cjk".into());}
        }
    } else if settings.language == diamond_core::Language::Zh {
        error=Some(settings.language.text("Chinese glyphs need a system CJK font: install noto-fonts-cjk and restart Diamond.").into());
    }
    for (name,path,key) in [("diamond-editor",&settings.editor_font_path,&settings.editor_font),("diamond-ui",&settings.ui_font_path,&settings.ui_font)] {
        let base = if key=="mono" {FontFamily::Monospace}else{FontFamily::Proportional};
        let mut family = fonts.families[&base].clone();
        if let Some(path)=path {
            if let Some(data)=load_font(path,0) {
                fonts.font_data.insert(name.into(),std::sync::Arc::new(data));
                family.insert(0,name.into());
            } else {error=Some(settings.language.text("Unable to load selected font; using the default font.").into());}
        }
        fonts.families.insert(FontFamily::Name(name.into()),family);
    }
    ctx.set_fonts(fonts);error
}

pub fn font_family(_settings:&AppSettings)->FontFamily {
    FontFamily::Name("diamond-editor".into())
}

pub fn ui_size(ui:&egui::Ui, base:f32)->f32 {
    base * egui::TextStyle::Body.resolve(ui.style()).size / 14.0
}
