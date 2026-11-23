// SPDX-License-Identifier: GPL-3.0-only
//! Bundled Lucide SVGs, rasterized once per icon; see assets/lucide/LICENSE.

use egui::{Color32, Painter, Pos2, Rect, Response, Sense, StrokeKind, Ui, Vec2};

const ICON_SIZE: f32 = 18.0;

// Keep the small shared vocabulary available to each part of the desktop shell.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Folder,
    File,
    Search,
    Plugin,
    Settings,
    Plus,
    Save,
    Close,
    Refresh,
    ChevronRight,
    ChevronDown,
    Sidebar,
    Command,
    Diamond,
    Check,
    Code,
    ArrowLeft,
    Info,
    Dot,
    Vaults,
    FileText,
    FileRust,
    FilePython,
    FileJavascript,
    FileTypescript,
    FileC,
    FileCpp,
    FileGo,
    FileJava,
    FileKotlin,
    FileCsharp,
    FileSwift,
    FileTypst,
    FileLatex,
    FileMarkdown,
    FileHtml,
    FileCss,
    FileSql,

    FileCode,
    FileJson,
    FileImage,
    FilePdf,
    FileArchive,
    FileAudio,
    FileVideo,
    FileConfig,
    FileTable,
    CaseSensitive,
    Regex,
    ArrowDown,
    ArrowUp,
}

/// Paint a centered icon within `rect`, inheriting the caller's chosen color.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    if icon == Icon::Diamond {
        if let Some(texture) = painter
            .ctx()
            .data(|data| data.get_temp::<egui::TextureHandle>(egui::Id::new("diamond-brand")))
        {
            let [width, height] = texture.size();
            let scale = (rect.width() / width as f32).min(rect.height() / height as f32);
            let fitted = Rect::from_center_size(
                rect.center(),
                egui::vec2(width as f32 * scale, height as f32 * scale),
            );
            painter.image(
                texture.id(),
                fitted,
                Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            return;
        }
    }
    diamond_editor::svg::paint(
        painter,
        egui::Id::new(("lucide", icon)),
        source(icon),
        rect,
        color,
    );
    let badge = match icon {
        Icon::FileRust => "RS",
        Icon::FilePython => "Py",
        Icon::FileJavascript => "JS",
        Icon::FileTypescript => "TS",
        Icon::FileC => "C",
        Icon::FileCpp => "C+",
        Icon::FileGo => "Go",
        Icon::FileJava => "Jv",
        Icon::FileKotlin => "Kt",
        Icon::FileCsharp => "C#",
        Icon::FileSwift => "Sw",
        Icon::FileTypst => "Ty",
        Icon::FileLatex => "TeX",
        Icon::FileMarkdown => "M↓",
        Icon::FileHtml => "<>",
        Icon::FileCss => "#",
        Icon::FileSql => "DB",
        _ => "",
    };
    if !badge.is_empty() {
        let badge_rect = Rect::from_center_size(
            egui::pos2(rect.center().x, rect.bottom() - 3.0),
            egui::vec2(rect.width(), 8.0),
        );
        painter.rect_filled(
            badge_rect,
            2.0,
            painter.ctx().style().visuals.extreme_bg_color,
        );
        painter.text(
            badge_rect.center(),
            egui::Align2::CENTER_CENTER,
            badge,
            egui::FontId::monospace(7.0),
            color,
        );
    }
}

fn source(icon: Icon) -> &'static [u8] {
    match icon {
        Icon::Folder => include_bytes!("../../../assets/lucide/folder.svg"),
        Icon::File => include_bytes!("../../../assets/lucide/file.svg"),
        Icon::Search => include_bytes!("../../../assets/lucide/search.svg"),
        Icon::Plugin => include_bytes!("../../../assets/lucide/plug.svg"),
        Icon::Settings => include_bytes!("../../../assets/lucide/settings.svg"),
        Icon::Plus => include_bytes!("../../../assets/lucide/plus.svg"),
        Icon::Save => include_bytes!("../../../assets/lucide/save.svg"),
        Icon::Close => include_bytes!("../../../assets/lucide/x.svg"),
        Icon::Refresh => include_bytes!("../../../assets/lucide/refresh-cw.svg"),
        Icon::ChevronRight => include_bytes!("../../../assets/lucide/chevron-right.svg"),
        Icon::ChevronDown => include_bytes!("../../../assets/lucide/chevron-down.svg"),
        Icon::Sidebar => include_bytes!("../../../assets/lucide/panel-right.svg"),
        Icon::Command => include_bytes!("../../../assets/lucide/command.svg"),
        Icon::Diamond => include_bytes!("../../../assets/lucide/diamond.svg"),
        Icon::Check => include_bytes!("../../../assets/lucide/check.svg"),
        Icon::Code => include_bytes!("../../../assets/lucide/code.svg"),
        Icon::ArrowLeft => include_bytes!("../../../assets/lucide/arrow-left.svg"),
        Icon::Info => include_bytes!("../../../assets/lucide/info.svg"),
        Icon::Dot => include_bytes!("../../../assets/lucide/dot.svg"),
        Icon::Vaults => include_bytes!("../../../assets/lucide/library-big.svg"),
        Icon::FileText => include_bytes!("../../../assets/lucide/file-text.svg"),
        Icon::FileRust
        | Icon::FilePython
        | Icon::FileJavascript
        | Icon::FileTypescript
        | Icon::FileC
        | Icon::FileCpp
        | Icon::FileGo
        | Icon::FileJava
        | Icon::FileKotlin
        | Icon::FileCsharp
        | Icon::FileSwift
        | Icon::FileTypst
        | Icon::FileLatex
        | Icon::FileMarkdown
        | Icon::FileHtml
        | Icon::FileCss
        | Icon::FileSql => include_bytes!("../../../assets/lucide/file-code.svg"),
        Icon::FileCode => include_bytes!("../../../assets/lucide/file-code.svg"),
        Icon::FileJson => include_bytes!("../../../assets/lucide/braces.svg"),
        Icon::FileImage => include_bytes!("../../../assets/lucide/image.svg"),
        Icon::FilePdf => include_bytes!("../../../assets/lucide/file-type.svg"),
        Icon::FileArchive => include_bytes!("../../../assets/lucide/file-archive.svg"),
        Icon::FileAudio => include_bytes!("../../../assets/lucide/file-music.svg"),
        Icon::FileVideo => include_bytes!("../../../assets/lucide/video.svg"),
        Icon::FileConfig => include_bytes!("../../../assets/lucide/file-cog.svg"),
        Icon::FileTable => include_bytes!("../../../assets/lucide/file-spreadsheet.svg"),
        Icon::CaseSensitive => include_bytes!("../../../assets/lucide/case-sensitive.svg"),
        Icon::Regex => include_bytes!("../../../assets/lucide/regex.svg"),
        Icon::ArrowDown => include_bytes!("../../../assets/lucide/arrow-down.svg"),
        Icon::ArrowUp => include_bytes!("../../../assets/lucide/arrow-up.svg"),
    }
}

pub use diamond_editor::svg::rasterize;

pub fn brand_image() -> Option<egui::ColorImage> {
    use std::io::Read;
    let mut paths = Vec::new();
    if let Ok(directory) = std::env::current_dir() {
        paths.push(directory.join("diamond.svg"));
    }
    if let Ok(executable) = std::env::current_exe() {
        for directory in executable.ancestors().skip(1).take(3) {
            paths.push(directory.join("diamond.svg"));
        }
    }
    paths.push(std::path::PathBuf::from(
        "/usr/share/icons/hicolor/scalable/apps/diamond.svg",
    ));
    for path in paths {
        if let Ok(file) = std::fs::File::open(path) {
            let mut bytes = Vec::new();
            if file.take(1024 * 1024 + 1).read_to_end(&mut bytes).is_ok()
                && bytes.len() <= 1024 * 1024
            {
                if let Some(image) = rasterize(&bytes, 128) {
                    return Some(image);
                }
            }
        }
    }
    rasterize(include_bytes!("../../../assets/diamond.svg"), 128)
}

pub fn install_brand(ctx: &egui::Context) {
    if let Some(image) = brand_image() {
        let texture = ctx.load_texture("diamond-custom-brand", image, egui::TextureOptions::LINEAR);
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("diamond-brand"), texture));
    }
}

pub fn for_path(path: &std::path::Path) -> Icon {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "md" | "markdown" => Icon::FileMarkdown,
        "txt" | "rst" => Icon::FileText,
        "rs" => Icon::FileRust,
        "py" | "pyi" => Icon::FilePython,
        "js" | "jsx" | "mjs" | "cjs" => Icon::FileJavascript,
        "ts" | "tsx" => Icon::FileTypescript,
        "c" | "h" => Icon::FileC,
        "c++" | "cpp" | "hpp" | "cc" | "cxx" => Icon::FileCpp,
        "go" => Icon::FileGo,
        "java" => Icon::FileJava,
        "kt" | "kts" => Icon::FileKotlin,
        "cs" => Icon::FileCsharp,
        "swift" => Icon::FileSwift,
        "typ" => Icon::FileTypst,
        "tex" | "latex" => Icon::FileLatex,
        "html" | "htm" => Icon::FileHtml,
        "css" | "scss" => Icon::FileCss,
        "sql" => Icon::FileSql,
        "lua" | "sh" | "bash" | "zsh" | "rb" | "php" | "pl" | "r" | "ex" | "exs" | "zig"
        | "dart" | "vue" | "svelte" => Icon::FileCode,
        "json" | "jsonc" => Icon::FileJson,
        "toml" | "yaml" | "yml" | "ini" | "conf" => Icon::FileConfig,
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "svg" | "ico" => Icon::FileImage,
        "pdf" => Icon::FilePdf,
        "zip" | "tar" | "gz" | "xz" | "7z" | "zst" => Icon::FileArchive,
        "mp3" | "wav" | "ogg" | "flac" => Icon::FileAudio,
        "mp4" | "mkv" | "webm" | "mov" => Icon::FileVideo,
        "csv" | "tsv" | "xlsx" | "ods" => Icon::FileTable,
        _ => Icon::File,
    }
}

/// A compact icon-only button with an accessible name and hover hint.
pub fn button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    icon_button(ui, icon, false, tooltip, 28.0)
}

/// A rail button whose active state follows the application's selection palette.
pub fn selectable_button(ui: &mut Ui, icon: Icon, selected: bool, tooltip: &str) -> Response {
    icon_button(ui, icon, selected, tooltip, 32.0)
}

fn icon_button(ui: &mut Ui, icon: Icon, selected: bool, tooltip: &str, edge: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(edge), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), tooltip)
    });
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact_selectable(&response, selected);
        if selected || response.hovered() || response.is_pointer_button_down_on() {
            ui.painter().rect_filled(rect, 10.0, visuals.bg_fill);
        }
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect.shrink(0.5),
                10.0,
                visuals.bg_stroke,
                StrokeKind::Inside,
            );
        }
        paint(
            ui.painter(),
            Rect::from_center_size(rect.center(), Vec2::splat(ICON_SIZE)),
            icon,
            visuals.fg_stroke.color,
        );
    }
    response.on_hover_text(tooltip)
}

/// A text button with a native icon and a fixed, vertically centered baseline.
pub fn label_button(ui: &mut Ui, icon: Icon, label: &str) -> Response {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let height = (font.size + 16.0).max(32.0);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, ui.visuals().text_color());
    let desired_size = Vec2::new(ICON_SIZE + 7.0 + galley.size().x + 18.0, height);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        ui.painter().rect_filled(rect, 10.0, visuals.weak_bg_fill);
        if response.hovered() || response.is_pointer_button_down_on() {
            ui.painter().rect_filled(rect, 10.0, visuals.bg_fill);
        }
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect.shrink(0.5),
                10.0,
                visuals.bg_stroke,
                StrokeKind::Inside,
            );
        }
        let icon_rect = Rect::from_center_size(
            egui::pos2(rect.left() + 9.0 + ICON_SIZE * 0.5, rect.center().y),
            Vec2::splat(ICON_SIZE),
        );
        paint(ui.painter(), icon_rect, icon, visuals.fg_stroke.color);
        let position = egui::pos2(
            icon_rect.right() + 7.0,
            rect.center().y - galley.size().y * 0.5,
        );
        ui.painter()
            .galley(position, galley, visuals.fg_stroke.color);
    }
    response
}
