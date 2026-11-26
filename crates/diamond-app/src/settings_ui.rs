// SPDX-License-Identifier: GPL-3.0-only
use crate::theme::SystemFont;
use diamond_core::{AppSettings, ThemeMode};

pub struct Changes {
    pub changed: bool,
    pub font_changed: bool,
    pub reset: bool,
}

pub fn show(ui: &mut egui::Ui, settings: &mut AppSettings, fonts: &[SystemFont]) -> Changes {
    let lang = settings.language;
    let tr = |key| lang.text(key);
    let before = (
        settings.editor_font.clone(),
        settings.editor_font_path.clone(),
        settings.editor_font_index,
        settings.ui_font.clone(),
        settings.ui_font_path.clone(),
        settings.ui_font_index,
        settings.code_font.clone(),
        settings.code_font_path.clone(),
        settings.code_font_index,
        settings.language,
    );
    let mut changed = false;
    let mut reset = false;
    egui::ScrollArea::vertical().auto_shrink([false,false]).show(ui,|ui|{
        ui.set_max_width(780.0);
        ui.heading(tr("Настройки"));
        ui.label(tr("Сохраняются автоматически для всех хранилищ"));
        ui.add_space(24.0);ui.strong(tr("Внешний вид"));ui.add_space(12.0);
        egui::Grid::new("appearance_settings").num_columns(2).spacing([32.0,16.0]).show(ui,|ui|{
            ui.label(tr("Language"));
            egui::ComboBox::from_id_salt("language").width(220.0)
                .selected_text(match settings.language {diamond_core::Language::En=>"English",diamond_core::Language::Ru=>"Русский",diamond_core::Language::Zh=>"简体中文"})
                .show_ui(ui,|ui|{
                    for (language,label) in [(diamond_core::Language::En,"English"),(diamond_core::Language::Ru,"Русский"),(diamond_core::Language::Zh,"简体中文")] {
                        changed |= ui.selectable_value(&mut settings.language, language, label).changed();
                    }
                });ui.end_row();
            ui.label(tr("Interface font"));
            changed|=font_picker(ui,"ui-font",FontChoice{name:&mut settings.ui_font,path:&mut settings.ui_font_path,index:&mut settings.ui_font_index},fonts,FontKind::Interface,lang);ui.end_row();
            ui.label(tr("Interface text size"));
            changed |= committed_slider(ui, "ui_font_size", &mut settings.ui_font_size, 12.0..=24.0);ui.end_row();
            ui.label(tr("Тема"));
            egui::ComboBox::from_id_salt("theme_choice").width(220.0)
                .selected_text(match settings.theme {ThemeMode::Dark=>tr("Тёмная"),ThemeMode::Light=>tr("Светлая"),ThemeMode::System=>tr("Системная")})
                .show_ui(ui,|ui|{
                    for (mode,title) in [(ThemeMode::Dark,tr("Тёмная")),(ThemeMode::Light,tr("Светлая")),(ThemeMode::System,tr("Системная"))]{
                        changed|=ui.selectable_value(&mut settings.theme,mode,title).changed();
                    }
                });ui.end_row();
            ui.label(tr("Акцентный цвет"));
            ui.horizontal_wrapped(|ui|{
                changed|=ui.color_edit_button_srgb(&mut settings.accent).changed();
                let key=ui.make_persistent_id("accent-hex-draft");
                let mut draft=ui.ctx().data_mut(|data|data.get_temp::<(String,[u8;3])>(key)).unwrap_or_else(||(format!("#{:02X}{:02X}{:02X}",settings.accent[0],settings.accent[1],settings.accent[2]),settings.accent));
                if draft.1!=settings.accent{draft=(format!("#{:02X}{:02X}{:02X}",settings.accent[0],settings.accent[1],settings.accent[2]),settings.accent);}
                let field=ui.add(egui::TextEdit::singleline(&mut draft.0).desired_width(100.0).char_limit(7).hint_text("#RRGGBB"));
                let commit=field.lost_focus()||field.has_focus()&&ui.input(|input|input.key_pressed(egui::Key::Enter));
                if commit&&let Some(color)=diamond_core::color::parse_hex(&draft.0){changed|=settings.accent!=color;settings.accent=color;draft.1=color;}
                if diamond_core::color::parse_hex(&draft.0).is_none(){field.on_hover_text(tr("Use a six-digit color, for example #7BB2FF"));}
                ui.ctx().data_mut(|data|data.insert_temp(key,draft));
                for color in [[255,137,152],[255,166,109],[243,168,30],[190,224,112],[118,215,174],[123,178,255],[212,144,255]]{
                    let (rect,response)=ui.allocate_exact_size(egui::vec2(22.0,22.0),egui::Sense::click());
                    ui.painter().rect_filled(rect,7.0,egui::Color32::from_rgb(color[0],color[1],color[2]));
                    if response.clicked(){settings.accent=color;changed=true;}
                }
            });ui.end_row();
            ui.label(tr("Шрифт редактора"));
            changed|=font_picker(ui,"editor-font",FontChoice{name:&mut settings.editor_font,path:&mut settings.editor_font_path,index:&mut settings.editor_font_index},fonts,FontKind::Editor,lang);ui.end_row();
            ui.label(tr("Source font"));
            changed|=font_picker(ui,"source-font",FontChoice{name:&mut settings.code_font,path:&mut settings.code_font_path,index:&mut settings.code_font_index},fonts,FontKind::Source,lang);ui.end_row();
            ui.label(tr("Размер текста Markdown"));
            changed|=committed_slider(ui, "editor_font_size", &mut settings.editor_font_size, 12.0..=30.0);ui.end_row();
            ui.label(tr("Размер исходного кода"));
            changed|=committed_slider(ui, "code_font_size", &mut settings.code_font_size, 11.0..=26.0);ui.end_row();
            ui.label(tr("Scroll speed"));
            changed|=committed_slider(ui, "scroll_speed", &mut settings.scroll_speed, 0.25..=4.0);ui.end_row();
            ui.label(tr("Ширина заметки"));
            changed|=committed_slider(ui, "editor_width", &mut settings.editor_width, 640.0..=1200.0);ui.end_row();
        });
        ui.add_space(28.0);ui.strong(tr("Редактор и интерфейс"));ui.add_space(12.0);
        changed|=ui.checkbox(&mut settings.word_wrap,tr("Перенос длинных строк")).changed();
        changed|=ui.checkbox(&mut settings.line_numbers,tr("Номера строк в исходном коде")).changed();
        changed|=ui.checkbox(&mut settings.show_inspector,tr("Показывать структуру заметки и обратные ссылки")).changed();
        changed|=ui.checkbox(&mut settings.reduced_motion,tr("Уменьшить анимацию")).changed();
        ui.add_space(24.0);
        ui.label(tr("Markdown в режиме Live сохраняется автоматически. Поиск: Ctrl+F; поиск по хранилищу: Ctrl+Shift+F; Esc закрывает поиск."));
        ui.add_space(24.0);
        if ui.button(tr("Сбросить настройки внешнего вида")).clicked(){reset=true;}
        ui.add_space(20.0);ui.small(format!("Diamond {} · GPL-3.0-only · Lucide Icons (ISC / MIT)",env!("CARGO_PKG_VERSION")));
        ui.label(tr("Color emoji need a system color font, such as noto-fonts-emoji. Restart after installing fonts."));
        ui.collapsing(tr("Syntax licenses"),|ui|{ui.label(diamond_editor::syntax_licenses());});
        ui.collapsing(tr("Лицензия иконок Lucide"),|ui|{
            ui.label(include_str!("../../../assets/lucide/LICENSE"));
        });
    });
    Changes {
        changed,
        font_changed: before
            != (
                settings.editor_font.clone(),
                settings.editor_font_path.clone(),
                settings.editor_font_index,
                settings.ui_font.clone(),
                settings.ui_font_path.clone(),
                settings.ui_font_index,
                settings.code_font.clone(),
                settings.code_font_path.clone(),
                settings.code_font_index,
                settings.language,
            ),
        reset,
    }
}

struct FontChoice<'a> {
    name: &'a mut String,
    path: &'a mut Option<std::path::PathBuf>,
    index: &'a mut u32,
}
#[derive(Clone, Copy, PartialEq)]
enum FontKind {
    Interface,
    Editor,
    Source,
}
#[derive(Clone, Default)]
struct FontSearch {
    query: String,
    open: bool,
    cached_query: String,
    font_count: usize,
    matches: std::sync::Arc<Vec<usize>>,
}
fn font_matches(fonts: &[SystemFont], query: &str, mono: bool) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    let terms: Vec<_> = query.split_whitespace().collect();
    let mut result: Vec<_> = fonts
        .iter()
        .enumerate()
        .filter(|(_, font)| {
            (!mono || font.monospace)
                && terms
                    .iter()
                    .all(|term| font.name.to_lowercase().contains(*term))
        })
        .map(|(i, _)| i)
        .collect();
    result.sort_by_key(|&i| {
        let name = fonts[i].name.to_lowercase();
        (!name.starts_with(&query), !name.contains("regular"), name)
    });
    result
}
fn font_picker(
    ui: &mut egui::Ui,
    key: &str,
    choice: FontChoice<'_>,
    fonts: &[SystemFont],
    kind: FontKind,
    lang: diamond_core::Language,
) -> bool {
    let id = ui.make_persistent_id(("font-search", key));
    let query_id = id.with("query");
    let mut draft = ui
        .ctx()
        .data(|data| data.get_temp::<FontSearch>(id))
        .unwrap_or_default();
    let display = if choice.path.is_none() {
        lang.text(if *choice.name == "mono" {
            "Моноширинный"
        } else {
            "По умолчанию"
        })
        .to_owned()
    } else {
        choice.name.clone()
    };
    let first_open = !draft.open;
    let mut selected = None;
    let popup = egui::ComboBox::from_id_salt(key)
        .width(300.0)
        .selected_text(display)
        .show_ui(ui, |ui| {
            if first_open {
                draft.query.clear();
                draft.font_count = usize::MAX;
            }
            let enter = ui.memory(|memory| memory.has_focus(query_id))
                && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            let field = ui.add(
                egui::TextEdit::singleline(&mut draft.query)
                    .id(query_id)
                    .desired_width(280.0)
                    .hint_text(lang.text("Search fonts…")),
            );
            if first_open {
                field.request_focus();
            }
            if draft.cached_query != draft.query || draft.font_count != fonts.len() {
                draft.matches = std::sync::Arc::new(font_matches(
                    fonts,
                    &draft.query,
                    kind == FontKind::Source,
                ));
                draft.cached_query.clone_from(&draft.query);
                draft.font_count = fonts.len();
            }
            ui.add_space(6.0);
            let query = draft.query.trim().to_lowercase();
            let mut builtin_match = false;
            for (name, label) in [
                ("sans", lang.text("По умолчанию")),
                ("mono", lang.text("Моноширинный")),
            ] {
                if (kind == FontKind::Source && name != "mono")
                    || (kind == FontKind::Interface && name != "sans")
                {
                    continue;
                }
                if !query.is_empty()
                    && !name.contains(&query)
                    && !label.to_lowercase().contains(&query)
                {
                    continue;
                }
                builtin_match = true;
                if ui
                    .selectable_label(choice.path.is_none() && choice.name.as_str() == name, label)
                    .clicked()
                    || (enter && selected.is_none())
                {
                    selected = Some((name.to_owned(), None, 0));
                    ui.close();
                }
            }
            egui::ScrollArea::vertical()
                .id_salt(id.with("results"))
                .max_height(260.0)
                .show(ui, |ui| {
                    for &index in draft.matches.iter().take(100) {
                        let font = &fonts[index];
                        if ui
                            .selectable_label(
                                choice.path.as_ref() == Some(&font.path)
                                    && *choice.index == font.index,
                                &font.name,
                            )
                            .clicked()
                            || (enter && selected.is_none())
                        {
                            selected =
                                Some((font.name.clone(), Some(font.path.clone()), font.index));
                            ui.close();
                        }
                    }
                    if draft.matches.is_empty() && !builtin_match && !query.is_empty() {
                        ui.label(lang.text("No matching fonts"));
                    }
                });
        });
    draft.open = popup.inner.is_some();
    ui.ctx().data_mut(|data| data.insert_temp(id, draft));
    if let Some((name, path, index)) = selected {
        let changed = *choice.name != name || *choice.path != path || *choice.index != index;
        *choice.name = name;
        *choice.path = path;
        *choice.index = index;
        changed
    } else {
        false
    }
}

fn committed_slider(
    ui: &mut egui::Ui,
    key: &'static str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
) -> bool {
    let id = ui.make_persistent_id(("settings-slider", key));
    let mut draft = ui
        .ctx()
        .data_mut(|data| data.get_temp::<f32>(id))
        .unwrap_or(*value);
    let response = ui.add(
        egui::Slider::new(&mut draft, range).suffix(if key == "scroll_speed" {
            " ×"
        } else {
            " px"
        }),
    );
    if response.dragged() || response.is_pointer_button_down_on() {
        ui.ctx().data_mut(|data| data.insert_temp(id, draft));
        false
    } else {
        ui.ctx().data_mut(|data| data.remove::<f32>(id));
        if response.drag_stopped() || response.changed() || draft != *value {
            let changed = *value != draft;
            *value = draft;
            changed
        } else {
            false
        }
    }
}

#[cfg(test)]
mod font_search_tests {
    use super::*;
    #[test]
    fn font_query_matches_each_word_and_source_only_offers_monospace_faces() {
        let fonts = vec![
            SystemFont {
                name: "Montserrat · Regular".into(),
                path: "/fonts/montserrat.ttf".into(),
                index: 0,
                monospace: false,
            },
            SystemFont {
                name: "DejaVu Sans Mono · Bold".into(),
                path: "/fonts/dejavu.ttc".into(),
                index: 1,
                monospace: true,
            },
            SystemFont {
                name: "DejaVu Sans Mono · Regular".into(),
                path: "/fonts/dejavu.ttc".into(),
                index: 0,
                monospace: true,
            },
        ];
        assert_eq!(font_matches(&fonts, "monTSerrat", false), vec![0]);
        assert!(font_matches(&fonts, "Montserrat", true).is_empty());
        assert_eq!(font_matches(&fonts, "dejavu mono", true), vec![2, 1]);
        assert_eq!(font_matches(&fonts, "dejavu mono bold", true), vec![1]);
        assert!(font_matches(&fonts, "unknown", false).is_empty());
    }
}
