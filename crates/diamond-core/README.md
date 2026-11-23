# diamond-core

Shared serializable desktop settings. This crate has no GUI dependency and does not access the filesystem. The application chooses the XDG settings path and controls persistence.

`AppSettings` records the last vault, reduced motion, editor font size and inspector visibility. Unknown or missing settings use defaults when decoded from JSON.

Version 0.3 adds backward-compatible defaults for theme, accent, system font preference, editor layout and recent vaults. `search::SearchPattern` compiles bounded Rust regular expressions shared by global and document find. This remains GUI- and filesystem-independent.

0.4 adds Language (English default, Russian, Simplified Chinese) and independent interface font/path/size preferences. Translations are static data with placeholder substitution, without runtime translation services.

0.7 adds backward-compatible `show_sidebar` and `scroll_speed` preferences,
pastel rainbow accent defaults and localized plugin/table/image controls.
