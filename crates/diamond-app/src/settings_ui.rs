use diamond_core::{AppSettings,ThemeMode};
use crate::theme::SystemFont;

pub struct Changes {pub changed:bool,pub font_changed:bool,pub reset:bool}

pub fn show(ui:&mut egui::Ui,settings:&mut AppSettings,fonts:&[SystemFont])->Changes {
    let lang = settings.language;
    let tr = |key| lang.text(key);
    let before=(settings.editor_font.clone(),settings.editor_font_path.clone(),settings.ui_font.clone(),settings.ui_font_path.clone(),settings.language);
    let mut changed=false;let mut reset=false;
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
            egui::ComboBox::from_id_salt("ui-font").width(280.0)
                .selected_text(if settings.ui_font == "sans" {tr("По умолчанию")} else {&settings.ui_font})
                .show_ui(ui,|ui|{
                    if ui.selectable_label(settings.ui_font_path.is_none(),tr("По умолчанию")).clicked() {
                        settings.ui_font="sans".into();settings.ui_font_path=None;changed=true;
                    }
                    for font in fonts {
                        if ui.selectable_label(settings.ui_font_path.as_ref()==Some(&font.path),&font.name).clicked() {
                            settings.ui_font=font.name.clone();settings.ui_font_path=Some(font.path.clone());changed=true;
                        }
                    }
                });ui.end_row();
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
            ui.horizontal(|ui|{
                changed|=ui.color_edit_button_srgb(&mut settings.accent).changed();
                for color in [[224,224,224],[110,164,255],[166,132,245],[86,190,155],[231,172,90],[222,128,154]]{
                    let (rect,response)=ui.allocate_exact_size(egui::vec2(22.0,22.0),egui::Sense::click());
                    ui.painter().rect_filled(rect,7.0,egui::Color32::from_rgb(color[0],color[1],color[2]));
                    if response.clicked(){settings.accent=color;changed=true;}
                }
            });ui.end_row();
            ui.label(tr("Шрифт редактора"));
            egui::ComboBox::from_id_salt("editor_font_choice").width(280.0).selected_text(
                if settings.editor_font=="sans"{tr("По умолчанию")}else if settings.editor_font=="mono"{tr("Моноширинный")}else{&settings.editor_font}
            ).show_ui(ui,|ui|{
                for (key,title) in [("sans",tr("По умолчанию")),("mono",tr("Моноширинный"))]{
                    if ui.selectable_label(settings.editor_font==key,title).clicked(){
                        settings.editor_font=key.into();settings.editor_font_path=None;changed=true;
                    }
                }
                for font in fonts {
                    if ui.selectable_label(settings.editor_font_path.as_ref()==Some(&font.path),&font.name).clicked(){
                        settings.editor_font=font.name.clone();settings.editor_font_path=Some(font.path.clone());changed=true;
                    }
                }
            });ui.end_row();
            ui.label(tr("Размер текста Markdown"));
            changed|=committed_slider(ui, "editor_font_size", &mut settings.editor_font_size, 12.0..=30.0);ui.end_row();
            ui.label(tr("Размер исходного кода"));
            changed|=committed_slider(ui, "code_font_size", &mut settings.code_font_size, 11.0..=26.0);ui.end_row();
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
        ui.add_space(16.0);
        ui.label(tr("Иконка: diamond2.svg рядом с Cargo.toml или исполняемым файлом. Приложение подхватит её при следующем запуске."));
        ui.add_space(24.0);
        if ui.button(tr("Сбросить настройки внешнего вида")).clicked(){reset=true;}
        ui.add_space(20.0);ui.small(format!("Diamond {} · Lucide Icons (ISC / MIT)",env!("CARGO_PKG_VERSION")));
        ui.collapsing(tr("Лицензия иконок Lucide"),|ui|{
            ui.label(include_str!("../../../assets/lucide/LICENSE"));
        });
    });
    Changes{changed,font_changed:before!=(settings.editor_font.clone(),settings.editor_font_path.clone(),settings.ui_font.clone(),settings.ui_font_path.clone(),settings.language),reset}
}

fn committed_slider(ui: &mut egui::Ui, key: &'static str, value: &mut f32, range: std::ops::RangeInclusive<f32>) -> bool {
    let id = ui.make_persistent_id(("settings-slider", key));
    let mut draft = ui.ctx().data_mut(|data| data.get_temp::<f32>(id)).unwrap_or(*value);
    let response = ui.add(egui::Slider::new(&mut draft, range).suffix(" px"));
    if response.dragged() || response.is_pointer_button_down_on() {
        ui.ctx().data_mut(|data| data.insert_temp(id, draft));
        false
    } else {
        ui.ctx().data_mut(|data| data.remove::<f32>(id));
        if response.drag_stopped() || response.changed() || draft != *value {
            let changed = *value != draft; *value = draft; changed
        } else { false }
    }
}
