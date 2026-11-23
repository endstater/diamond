// SPDX-License-Identifier: GPL-3.0-only
//! Shared settings without GUI dependencies or filesystem policy.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod color;
pub mod i18n;
pub mod search;
pub use i18n::Language;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub language: Language,
    pub ui_font: String,
    pub ui_font_path: Option<PathBuf>,
    pub ui_font_size: f32,
    pub last_vault: Option<PathBuf>,
    pub reduced_motion: bool,
    pub editor_font_size: f32,
    pub show_inspector: bool,
    pub show_sidebar: bool,
    pub scroll_speed: f32,
    pub recent_vaults: Vec<PathBuf>,
    pub theme: ThemeMode,
    pub accent: [u8; 3],
    pub editor_font: String,
    pub editor_font_path: Option<PathBuf>,
    pub code_font_size: f32,
    pub editor_width: f32,
    pub word_wrap: bool,
    pub line_numbers: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: Language::En,
            ui_font: "sans".into(),
            ui_font_path: None,
            ui_font_size: 14.0,
            last_vault: None,
            reduced_motion: false,
            editor_font_size: 17.0,
            show_inspector: true,
            show_sidebar: true,
            scroll_speed: 1.0,
            recent_vaults: Vec::new(),
            theme: ThemeMode::Dark,
            accent: [123, 178, 255],
            editor_font: "sans".into(),
            editor_font_path: None,
            code_font_size: 15.0,
            editor_width: 860.0,
            word_wrap: true,
            line_numbers: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn previous_settings_keep_values_and_gain_new_defaults() {
        let settings: AppSettings = serde_json::from_str(
            r#"{"editor_font_size":19.0,"reduced_motion":true,"show_inspector":false}"#,
        )
        .unwrap();
        assert_eq!(settings.editor_font_size, 19.0);
        assert!(settings.reduced_motion);
        assert!(!settings.show_inspector);
        assert_eq!(settings.theme, ThemeMode::Dark);
        assert!(settings.word_wrap);
        assert_eq!(settings.editor_width, 860.0);
        assert!(settings.recent_vaults.is_empty());
    }
}
