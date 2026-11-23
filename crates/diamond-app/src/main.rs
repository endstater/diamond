// SPDX-License-Identifier: GPL-3.0-only
mod icons;
mod plugin_preview;
mod portal;
mod resources;
mod session;
mod settings_ui;
mod theme;
mod watch;
mod workers;

use diamond_core::search::{SearchOptions, SearchPattern};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use diamond_core::AppSettings;
use diamond_editor::{EditorBuffer, EditorMode};
use diamond_plugin::PluginManifest;
use diamond_vault::{Vault, VaultIndex};
use egui::{Color32, RichText};
use icons::Icon;
use session::{PendingAction, Session};
use workers::{PluginEvent, PluginRequest, PluginWorker};

fn main() -> eframe::Result {
    let argument = std::env::args_os().nth(1);
    match argument.as_ref().and_then(|value| value.to_str()) {
        Some("--version" | "-V") => {
            println!("Diamond {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--syntax-licenses") => {
            println!("{}", diamond_editor::syntax_licenses());
            return Ok(());
        }
        Some("--help" | "-h") => {
            println!(
                "Diamond {}\nUsage: diamond [FOLDER | FILE]\n  FOLDER  Open or add a vault\n  FILE    Open one file without indexing its directory\n  --version  Print version\n  --syntax-licenses  Print bundled grammar acknowledgements",
                env!("CARGO_PKG_VERSION")
            );
            return Ok(());
        }
        _ => {}
    }
    let root = argument.map(|argument| expand_user_path(PathBuf::from(argument)));
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty())
        || std::env::var_os("WAYLAND_SOCKET").is_some_and(|value| !value.is_empty());
    let override_ = std::env::var("DIAMOND_VSYNC").ok();
    let vsync = swap_vsync(wayland, override_.as_deref());
    eprintln!(
        "[diamond {}] renderer=glow backend={} blocking_swap={}",
        env!("CARGO_PKG_VERSION"),
        if wayland { "wayland" } else { "x11-or-native" },
        vsync
    );
    eframe::run_native(
        "Diamond",
        eframe::NativeOptions {
            renderer: eframe::Renderer::Glow,
            // A blocking EGL swap can stop dispatching pings while a Wayland window
            // is covered/on another workspace. Avoid that wait; redraw requests
            // remain event-driven rather than introducing a continuous repaint loop.
            vsync,
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1320.0, 860.0])
                .with_min_inner_size([800.0, 540.0])
                .with_app_id("io.diamond.Diamond")
                .with_icon(window_icon()),
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(DiamondApp::new(cc, root)))),
    )
}

fn swap_vsync(wayland: bool, requested: Option<&str>) -> bool {
    match requested.map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case("on") || value == "1" => true,
        Some(value) if value.eq_ignore_ascii_case("off") || value == "0" => false,
        _ => !wayland,
    }
}

#[cfg(test)]
mod graphics_tests {
    use super::swap_vsync;

    #[test]
    fn wayland_uses_nonblocking_swap_and_x11_keeps_vsync() {
        assert!(!swap_vsync(true, None));
        assert!(swap_vsync(false, None));
        assert!(!swap_vsync(true, Some("auto")));
        assert!(swap_vsync(false, Some("auto")));
    }

    #[test]
    fn diagnostic_override_is_explicit_and_backend_independent() {
        for backend in [true, false] {
            assert!(swap_vsync(backend, Some("on")));
            assert!(!swap_vsync(backend, Some("off")));
            assert!(swap_vsync(backend, Some(" 1 ")));
            assert!(!swap_vsync(backend, Some("0")));
        }
        assert!(!swap_vsync(true, Some("invalid")));
        assert!(swap_vsync(false, Some("invalid")));
    }
}

fn window_icon() -> egui::IconData {
    if let Some(image) = icons::brand_image() {
        return egui::IconData {
            width: image.size[0] as u32,
            height: image.size[1] as u32,
            rgba: image
                .pixels
                .iter()
                .flat_map(|pixel| pixel.to_srgba_unmultiplied())
                .collect(),
        };
    }
    let size = 64usize;
    let mut rgba = vec![0; size * size * 4];
    for y in 8..57usize {
        for x in 5..59usize {
            let nx = (x as f32 - 32.0).abs();
            let edge = if y < 22 {
                19.0 + (y as f32 - 8.0) * 0.55
            } else {
                (57.0 - y as f32) * 0.77
            };
            if nx <= edge {
                let color = if x < 32 {
                    [218, 218, 218, 255]
                } else {
                    [126, 126, 126, 255]
                };
                rgba[(y * size + x) * 4..(y * size + x) * 4 + 4].copy_from_slice(&color);
            }
        }
    }
    egui::IconData {
        rgba,
        width: size as u32,
        height: size as u32,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Panel {
    Vaults,
    Files,
    Search,
    Plugins,
}

struct Replacement {
    vault_root: PathBuf,
    path: PathBuf,
    before: Vec<u8>,
    text: String,
}

struct DiamondApp {
    session: Session,
    settings: AppSettings,
    panel: Panel,
    message: Option<String>,
    toast_seen: Option<String>,
    toast_until: Option<Instant>,
    light_mode: bool,
    vault_input: String,
    open_dialog: bool,
    note_input: String,
    new_dialog: bool,
    filter: String,
    expanded_folders: std::collections::HashSet<PathBuf>,
    delete_entry: Option<(PathBuf, bool)>,
    rename_entry: Option<PathBuf>,
    rename_name: String,
    rename_focus: bool,
    vault_query: String,
    palette: bool,
    palette_query: String,
    settings_dialog: bool,
    settings_tab_open: bool,
    fonts: Vec<theme::SystemFont>,
    fonts_receiver: Option<Receiver<Vec<theme::SystemFont>>>,
    fonts_loaded: bool,
    prefs_dirty_at: Option<Instant>,
    watcher: Option<watch::VaultWatch>,
    files_changed_at: Option<Instant>,
    portal_receiver: Option<Receiver<portal::Choice>>,
    portal_folder: bool,
    plugin_install_folder: bool,
    plugin_install_receiver:
        Option<Receiver<Result<diamond_plugin::install::InstalledPlugin, String>>>,
    installed_plugins: Vec<diamond_plugin::install::InstalledPlugin>,
    plugin_views: Vec<diamond_plugin::ViewDescriptor>,
    preview: Option<plugin_preview::Preview>,
    index_pending: bool,
    index_generation: u64,
    vault_search_generation: u64,
    vault_search_results: Vec<usize>,
    focus_vault_search: bool,
    vault_search_options: SearchOptions,
    vault_search_cache: Option<(String, SearchOptions, Result<SearchPattern, String>)>,
    pending: Option<PendingAction>,
    allow_close: bool,
    index: Option<VaultIndex>,
    index_receiver: Option<Receiver<Result<VaultIndex, String>>>,
    plugin_path: String,
    plugin_manifest: Option<PluginManifest>,
    plugin_worker: Option<PluginWorker>,
    plugin_commands: Vec<(String, String)>,
    plugin_busy: bool,
    quit_after_plugin: bool,
    share_document: bool,
    plugin_input: Option<(PathBuf, PathBuf, Vec<u8>)>,
    replacement: Option<Replacement>,
}

impl DiamondApp {
    fn new(cc: &eframe::CreationContext<'_>, initial_root: Option<PathBuf>) -> Self {
        let (settings, message) = session::load_settings();
        theme::configure(&cc.egui_ctx, &settings);
        let font_error = theme::install_font(&cc.egui_ctx, &settings);
        icons::install_brand(&cc.egui_ctx);
        let initial_file = initial_root
            .as_ref()
            .filter(|path| path.is_file())
            .map(|path| path.canonicalize().unwrap_or_else(|_| path.clone()));
        let root = initial_file
            .as_ref()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .or(initial_root)
            .or_else(|| settings.last_vault.clone());
        let mut app = Self {
            session: Session::default(),
            settings,
            panel: Panel::Files,
            message: message.or(font_error),
            toast_seen: None,
            toast_until: None,
            light_mode: initial_file.is_some(),
            vault_input: String::new(),
            open_dialog: false,
            note_input: String::new(),
            new_dialog: false,
            filter: String::new(),
            expanded_folders: Default::default(),
            delete_entry: None,
            rename_entry: None,
            rename_name: String::new(),
            rename_focus: false,
            vault_query: String::new(),
            palette: false,
            palette_query: String::new(),
            settings_dialog: false,
            settings_tab_open: false,
            fonts: Vec::new(),
            fonts_receiver: None,
            fonts_loaded: false,
            prefs_dirty_at: None,
            watcher: None,
            files_changed_at: None,
            portal_receiver: None,
            portal_folder: true,
            plugin_install_folder: false,
            plugin_install_receiver: None,
            installed_plugins: diamond_plugin::install::installed(&plugin_directory()),
            plugin_views: Vec::new(),
            preview: None,
            index_pending: false,
            index_generation: 0,
            vault_search_generation: u64::MAX,
            vault_search_results: Vec::new(),
            focus_vault_search: false,
            vault_search_options: SearchOptions::default(),
            vault_search_cache: None,
            pending: None,
            allow_close: false,
            index: None,
            index_receiver: None,
            plugin_path: String::new(),
            plugin_manifest: None,
            plugin_worker: None,
            plugin_commands: Vec::new(),
            plugin_busy: false,
            quit_after_plugin: false,
            share_document: false,
            plugin_input: None,
            replacement: None,
        };
        if let Some(file) = initial_file {
            if let Err(error) = app.session.open_single_file(&file) {
                app.message = Some(error);
            }
        } else if let Some(root) = root {
            app.vault_input = root.to_string_lossy().into_owned();
            app.switch_vault(&cc.egui_ctx);
        }
        app
    }

    fn switch_vault(&mut self, ctx: &egui::Context) {
        let root = expand_user_path(PathBuf::from(self.vault_input.trim()));
        match self.session.open_vault(&root) {
            Ok(()) => {
                self.light_mode = false;
                let mut rescan_after_registration = false;
                self.message = None;
                self.settings.last_vault =
                    self.session.vault.as_ref().map(|v| v.root().to_path_buf());
                if let Some(root) = self.settings.last_vault.clone() {
                    self.settings.recent_vaults.retain(|path| path != &root);
                    self.settings.recent_vaults.insert(0, root.clone());
                    self.settings.recent_vaults.truncate(16);
                    self.watcher =
                        match watch::VaultWatch::start(&root, &self.session.entries, ctx.clone()) {
                            Ok(watcher) => Some(watcher),
                            Err(error) => {
                                self.message = Some(self.settings.language.message(
                                    "Watch unavailable: {error}",
                                    &[("error", error.to_string())],
                                ));
                                None
                            }
                        };
                    // A post-registration snapshot closes the initial read/watch gap.
                    if let Err(error) = self.session.refresh() {
                        self.message = Some(error);
                    }
                    if let Some(watcher) = &mut self.watcher {
                        match watcher.sync_directories(&self.session.entries) {
                            Ok(added) => rescan_after_registration = added,
                            Err(error) => self.message = Some(error),
                        }
                    }
                }
                self.files_changed_at = if rescan_after_registration {
                    Some(Instant::now())
                } else {
                    None
                };
                if rescan_after_registration {
                    ctx.request_repaint_after(Duration::from_millis(80));
                }
                self.index_receiver = None;
                self.index_pending = false;
                self.settings_dialog = false;
                self.open_dialog = false;
                self.index = None;
                self.filter.clear();
                self.expanded_folders.clear();
                self.delete_entry = None;
                self.rename_entry = None;

                self.persist_settings();
                self.reindex(ctx);
            }
            Err(error) => self.message = Some(error),
        }
    }

    fn persist_settings(&mut self) {
        self.prefs_dirty_at = None;
        if let Err(error) = session::save_settings(&self.settings) {
            self.message = Some(self.settings.language.message(
                "Could not save settings: {error}",
                &[("error", error.to_string())],
            ));
        }
    }

    fn choose_vault(&mut self, ctx: &egui::Context) {
        if self.portal_receiver.is_none() {
            self.portal_folder = true;
            self.portal_receiver = Some(portal::choose(
                true,
                self.settings.language.text("Открыть хранилище").into(),
                ctx.clone(),
            ));
        }
    }

    fn load_plugin_manifest(&mut self) {
        match PluginManifest::load(Path::new(self.plugin_path.trim())) {
            Ok(manifest) => self.plugin_manifest = Some(manifest),
            Err(error) => self.message = Some(error.to_string()),
        }
    }

    fn open_settings(&mut self, ctx: &egui::Context) {
        self.session.deactivate_active(Instant::now());
        self.settings_tab_open = true;
        self.settings_dialog = true;
        if !self.fonts_loaded && self.fonts_receiver.is_none() {
            let (sender, receiver) = mpsc::channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let _ = sender.send(theme::system_fonts());
                ctx.request_repaint();
            });
            self.fonts_receiver = Some(receiver);
        }
    }

    fn vaults_panel(&mut self, ui: &mut egui::Ui) {
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        if icons::label_button(ui, Icon::Plus, tr("Добавить хранилище")).clicked()
        {
            self.choose_vault(ui.ctx());
        }
        if self.portal_receiver.is_some() {
            ui.spinner();
        }
        ui.add_space(16.0);
        let current = self
            .session
            .vault
            .as_ref()
            .map(|vault| vault.root().to_path_buf());
        let mut pick = None;
        let mut remove = None;
        egui::ScrollArea::vertical()
            .id_salt("recent_vaults")
            .show(ui, |ui| {
                for (index, path) in self.settings.recent_vaults.iter().enumerate() {
                    egui::Frame::new()
                        .fill(ui.visuals().faint_bg_color)
                        .corner_radius(10)
                        .inner_margin(egui::Margin::same(10))
                        .show(ui, |ui| {
                            ui.set_min_width((ui.available_width()).max(1.0));
                            ui.horizontal(|ui| {
                                let label = path.file_name().unwrap_or_default().to_string_lossy();
                                let width = (ui.available_width() - 40.0).max(1.0);
                                if ui
                                    .add_sized(
                                        [width, 32.0],
                                        egui::Button::new(egui::RichText::new(label))
                                            .selected(current.as_ref() == Some(path))
                                            .truncate(),
                                    )
                                    .clicked()
                                {
                                    pick = Some(path.clone());
                                }
                                if icons::button(
                                    ui,
                                    Icon::Close,
                                    tr("Убрать из списка (файлы останутся)"),
                                )
                                .clicked()
                                {
                                    remove = Some(index);
                                }
                            });
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(path.to_string_lossy()).small(),
                                )
                                .truncate(),
                            )
                            .on_hover_text(path.to_string_lossy());
                        });
                    ui.add_space(8.0);
                }
            });
        if let Some(index) = remove {
            self.settings.recent_vaults.remove(index);
            self.persist_settings();
        }
        if let Some(path) = pick {
            if current.as_ref() != Some(&path) {
                self.vault_input = path.to_string_lossy().into_owned();
                self.request_action(PendingAction::SwitchVault, ui.ctx());
            }
        }
    }

    fn poll_environment(&mut self, ctx: &egui::Context) {
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        let choice = self
            .portal_receiver
            .as_ref()
            .and_then(|receiver| match receiver.try_recv() {
                Ok(choice) => Some(choice),
                Err(mpsc::TryRecvError::Disconnected) => Some(portal::Choice::Unavailable(
                    tr("Выбор файла прерван").into(),
                )),
                Err(mpsc::TryRecvError::Empty) => None,
            });
        if let Some(choice) = choice {
            self.portal_receiver = None;
            match choice {
                portal::Choice::Folder(path) if self.plugin_install_folder => {
                    self.plugin_install_folder = false;
                    self.start_plugin_install(path.to_string_lossy().into_owned(), ctx);
                }
                portal::Choice::Folder(path) => {
                    self.vault_input = path.to_string_lossy().into_owned();
                    self.request_action(PendingAction::SwitchVault, ctx);
                }
                portal::Choice::File(path) => {
                    self.plugin_path = path.to_string_lossy().into_owned();
                    self.load_plugin_manifest();
                }
                portal::Choice::Cancelled => {}
                portal::Choice::Unavailable(error) => {
                    self.message = Some(self.settings.language.message(
                        "File chooser unavailable: {error}. Enter a path manually.",
                        &[("error", error.to_string())],
                    ));
                    if self.plugin_install_folder {
                        self.plugin_install_folder = false;
                        self.panel = Panel::Plugins;
                    } else if self.portal_folder {
                        self.open_dialog = true;
                    } else {
                        self.panel = Panel::Plugins;
                    }
                }
            }
        }
        let installed =
            self.plugin_install_receiver
                .as_ref()
                .and_then(|receiver| match receiver.try_recv() {
                    Ok(result) => Some(result),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Some(Err("Plugin installer stopped".into()))
                    }
                    Err(mpsc::TryRecvError::Empty) => None,
                });
        if let Some(result) = installed {
            self.plugin_install_receiver = None;
            match result {
                Ok(plugin) => {
                    self.plugin_path = plugin.manifest.to_string_lossy().into_owned();
                    self.installed_plugins.push(plugin);
                    self.message =
                        Some(tr("Plugin installed. Select it to review and launch.").into());
                }
                Err(error) => self.message = Some(error),
            }
        }
        let fonts = self
            .fonts_receiver
            .as_ref()
            .and_then(|receiver| match receiver.try_recv() {
                Ok(fonts) => Some(fonts),
                Err(mpsc::TryRecvError::Disconnected) => Some(Vec::new()),
                Err(mpsc::TryRecvError::Empty) => None,
            });
        if let Some(fonts) = fonts {
            self.fonts = fonts;
            self.fonts_loaded = true;
            self.fonts_receiver = None;
        }
        let now = Instant::now();
        if let Some(watcher) = &self.watcher {
            if watcher.take_changed() {
                self.files_changed_at = Some(now);
            }
            if watcher.take_error() {
                self.message=Some(tr("Ошибка наблюдения за хранилищем. Проверьте лимит inotify; доступно ручное обновление.").into());
            }
        }
        if let Some(changed) = self.files_changed_at {
            let remaining =
                Duration::from_millis(80).saturating_sub(now.saturating_duration_since(changed));
            if remaining.is_zero() {
                self.files_changed_at = None;
                self.refresh_files(ctx);
            } else {
                ctx.request_repaint_after(remaining);
            }
        }
        if let Some(changed) = self.prefs_dirty_at {
            let remaining =
                Duration::from_millis(400).saturating_sub(now.saturating_duration_since(changed));
            if remaining.is_zero() {
                self.prefs_dirty_at = None;
                self.persist_settings();
            } else {
                ctx.request_repaint_after(remaining);
            }
        }
    }

    fn refresh_files(&mut self, ctx: &egui::Context) {
        if let Err(error) = self.session.refresh() {
            self.message = Some(error);
            return;
        }
        if let Some(watcher) = &mut self.watcher {
            match watcher.sync_directories(&self.session.entries) {
                Ok(true) => {
                    // One follow-up snapshot after new watch registration, not
                    // a periodic poll; it closes the registration race.
                    self.files_changed_at = Some(Instant::now());
                    ctx.request_repaint_after(Duration::from_millis(80));
                }
                Ok(false) => {}
                Err(error) => {
                    self.message = Some(self.settings.language.message(
                        "Watch unavailable: {error}",
                        &[("error", error.to_string())],
                    ))
                }
            }
        }
        self.reindex(ctx);
    }

    fn reindex(&mut self, ctx: &egui::Context) {
        if self.index_receiver.is_some() {
            self.index_pending = true;
            return;
        }
        let Some(root) = self.session.vault.as_ref().map(|v| v.root().to_path_buf()) else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = Vault::open(root)
                .and_then(|vault| vault.build_index())
                .map_err(|e| e.to_string());
            let _ = sender.send(result);
            ctx.request_repaint();
        });
        self.index_receiver = Some(receiver);
    }

    fn poll_workers(&mut self, ctx: &egui::Context) {
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        if let Some(receiver) = &self.index_receiver {
            match receiver.try_recv() {
                Ok(result) => {
                    match result {
                        Ok(index) => {
                            self.index = Some(index);
                            self.index_generation = self.index_generation.wrapping_add(1);
                        }
                        Err(e) => self.message = Some(e),
                    }
                    self.index_receiver = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.index_receiver = None;
                    self.message = Some(tr("Индексатор остановился").into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.index_receiver.is_none() && self.index_pending {
            self.index_pending = false;
            self.reindex(ctx);
        }
        let mut events = Vec::new();
        let mut disconnected = false;
        if let Some(worker) = &self.plugin_worker {
            loop {
                match worker.receiver.try_recv() {
                    Ok(event) => events.push(event),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        for event in events {
            match event {
                PluginEvent::Ready(commands, views) => {
                    self.plugin_commands = commands;
                    self.plugin_views = views;
                    self.plugin_busy = false;
                }
                PluginEvent::Preview {
                    document_id,
                    frame,
                    pages,
                } => {
                    self.plugin_busy = false;
                    if let Some(preview) = &mut self.preview {
                        let current = self
                            .session
                            .tabs
                            .get(self.session.active)
                            .map(|tab| tab.editor.revision_key());
                        if preview.document_id == document_id
                            && current.is_some_and(|key| {
                                Some(key) == preview.sent && key.1 == frame.revision
                            })
                        {
                            preview.receive(ctx, frame, pages);
                        }
                    }
                }
                PluginEvent::Output(output) => {
                    self.plugin_busy = false;
                    if let Some(message) = output.message {
                        self.message = Some(message);
                    }
                    if let (Some(text), Some((vault_root, path, before))) =
                        (output.replacement, self.plugin_input.take())
                    {
                        self.replacement = Some(Replacement {
                            vault_root,
                            path,
                            before,
                            text,
                        });
                    }
                }
                PluginEvent::Failed(error) => {
                    self.plugin_busy = false;
                    self.message = Some(error);
                    self.plugin_input = None;
                    self.plugin_commands.clear();
                    self.plugin_views.clear();
                    self.preview = None;
                }
                PluginEvent::Stopped => {
                    self.plugin_busy = false;
                    self.plugin_commands.clear();
                    self.plugin_views.clear();
                    self.preview = None;
                    self.plugin_worker = None;
                }
            }
        }
        if disconnected {
            self.plugin_worker = None;
            self.plugin_busy = false;
            self.plugin_commands.clear();
        }
    }

    fn save_active(&mut self, ctx: &egui::Context) {
        if self.session.tabs.is_empty() {
            return;
        }
        match self.session.save(self.session.active) {
            Ok(()) => {
                self.message = None;
                if self.watcher.is_none() {
                    self.reindex(ctx);
                }
            }
            Err(error) => {
                self.message = Some(
                    self.settings
                        .language
                        .message("Could not save: {error}", &[("error", error.to_string())]),
                )
            }
        }
    }

    fn request_action(&mut self, action: PendingAction, ctx: &egui::Context) {
        match self.session.flush_autosave(action) {
            Ok(saved) if saved > 0 => self.reindex(ctx),
            Ok(_) => {}
            Err(error) => {
                self.message = Some(
                    self.settings
                        .language
                        .message("Autosave failed: {error}", &[("error", error.to_string())]),
                )
            }
        }
        if self.session.dirty_tabs(action).is_empty() {
            self.perform_action(action, ctx);
        } else {
            self.pending = Some(action);
        }
    }

    fn perform_action(&mut self, action: PendingAction, ctx: &egui::Context) {
        self.pending = None;
        match action {
            PendingAction::CloseTab(index) => self.session.close_tab(index),
            PendingAction::SwitchVault => self.switch_vault(ctx),
            PendingAction::Quit => {
                if self.prefs_dirty_at.take().is_some() {
                    self.persist_settings();
                }
                if let Some(worker) = &self.plugin_worker {
                    self.quit_after_plugin = true;
                    self.plugin_busy = true;
                    let _ = worker.sender.send(PluginRequest::Stop);
                } else {
                    self.allow_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.pending.is_some() || self.plugin_manifest.is_some() || self.replacement.is_some() {
            return;
        }
        let key = |key, shift| {
            egui::KeyboardShortcut::new(
                egui::Modifiers {
                    command: true,
                    shift,
                    ..Default::default()
                },
                key,
            )
        };
        if ctx.input_mut(|i| i.consume_shortcut(&key(egui::Key::S, false))) {
            self.save_active(ctx);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&key(egui::Key::O, false))) {
            self.choose_vault(ctx);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&key(egui::Key::N, false)))
            && self.session.vault.is_some()
        {
            self.new_dialog = true;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&key(egui::Key::P, true))) {
            self.palette = !self.palette;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&key(egui::Key::W, false)))
            && !self.session.tabs.is_empty()
        {
            self.request_action(PendingAction::CloseTab(self.session.active), ctx);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&key(egui::Key::F, true))) {
            self.panel = Panel::Search;
            self.focus_vault_search = true;
            self.settings_dialog = false;
        }
    }

    fn top_bar(&mut self, ctx: &egui::Context) {
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        let colors = theme::Palette::of(ctx);
        egui::TopBottomPanel::top("titlebar")
            .show_separator_line(false)
            .exact_height((self.settings.ui_font_size.clamp(12.0, 24.0) * 3.4 + 16.0).max(64.0))
            .frame(
                egui::Frame::new()
                    .fill(colors.panel)
                    .inner_margin(egui::Margin::symmetric(16, 10))
                    .stroke(egui::Stroke::NONE),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    draw_icon(ui, Icon::Diamond, 28.0, colors.text);
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("Diamond")
                                .strong()
                                .size(theme::ui_size(ui, 17.0)),
                        );
                        ui.label(
                            RichText::new(tr("Личное пространство"))
                                .size(theme::ui_size(ui, 11.0))
                                .color(colors.muted),
                        );
                    });
                    ui.add_space(20.0);
                    if icons::label_button(ui, Icon::Folder, tr("Открыть"))
                        .on_hover_text(tr("Открыть хранилище · Ctrl+O"))
                        .clicked()
                    {
                        self.choose_vault(ctx);
                    }
                    if ui
                        .add_enabled_ui(self.session.vault.is_some(), |ui| {
                            icons::label_button(ui, Icon::Plus, tr("Заметка"))
                        })
                        .inner
                        .on_hover_text(tr("Новая заметка · Ctrl+N"))
                        .clicked()
                    {
                        self.new_dialog = true;
                    }
                    let manual_save = !self.settings_dialog
                        && self
                            .session
                            .tabs
                            .get(self.session.active)
                            .is_some_and(|tab| {
                                !tab.autosaves() && tab.editor.mode() != EditorMode::Image
                            });
                    if manual_save
                        && icons::label_button(ui, Icon::Save, tr("Сохранить"))
                            .on_hover_text("Ctrl+S")
                            .clicked()
                    {
                        self.save_active(ctx);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::selectable_button(
                            ui,
                            Icon::ArrowLeft,
                            self.settings.show_sidebar && !self.light_mode,
                            tr("Toggle left sidebar"),
                        )
                        .clicked()
                        {
                            self.settings.show_sidebar = !self.settings.show_sidebar;
                            self.persist_settings();
                        }
                        if icons::selectable_button(
                            ui,
                            Icon::Sidebar,
                            self.settings.show_inspector,
                            tr("Структура и связи"),
                        )
                        .clicked()
                        {
                            self.settings.show_inspector = !self.settings.show_inspector;
                            self.persist_settings();
                        }
                        if icons::label_button(ui, Icon::Command, tr("Команды"))
                            .on_hover_text("Ctrl+Shift+P")
                            .clicked()
                        {
                            self.palette = true;
                        }
                    });
                });
            });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        let colors = theme::Palette::of(ctx);
        egui::TopBottomPanel::bottom("status")
            .exact_height((self.settings.ui_font_size.clamp(12.0, 24.0) * 1.6 + 18.0).max(40.0))
            .frame(
                egui::Frame::new()
                    .fill(colors.panel)
                    .inner_margin(egui::Margin::symmetric(16, 3))
                    .stroke(egui::Stroke::new(1.0, colors.border)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    draw_icon(ui, Icon::Diamond, 16.0, colors.muted);
                    ui.label(
                        RichText::new(tr("Локальные файлы"))
                            .color(colors.muted)
                            .size(theme::ui_size(ui, 14.0)),
                    );
                    if self.index_receiver.is_some() {
                        ui.spinner();
                    }
                    if let Some(index) = &self.index {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(lang.message(
                                "{count} notes",
                                &[("count", index.notes.len().to_string())],
                            ))
                            .color(colors.muted)
                            .size(theme::ui_size(ui, 14.0)),
                        );
                    }
                    if let Some(tab) = self.session.tabs.get(self.session.active) {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(if tab.editor.mode() == EditorMode::Hex {
                                "HEX"
                            } else if tab.editor.mode() == EditorMode::Image {
                                "IMAGE"
                            } else {
                                "UTF-8"
                            })
                            .color(colors.muted)
                            .size(theme::ui_size(ui, 14.0)),
                        );
                        ui.add_space(8.0);
                        let (icon, status) = if tab.autosave_error().is_some() {
                            (Icon::Info, tr("Ошибка сохранения"))
                        } else if tab.is_dirty() && tab.autosaves() {
                            (Icon::Dot, tr("Сохраняем…"))
                        } else if tab.is_dirty() {
                            (Icon::Dot, tr("Изменено"))
                        } else {
                            (Icon::Check, tr("Сохранено"))
                        };
                        let color = if !tab.is_dirty() && tab.autosave_error().is_none() {
                            Color32::from_rgb(190, 224, 112)
                        } else {
                            colors.muted
                        };
                        draw_icon(ui, icon, 16.0, color);
                        let response = ui.label(
                            RichText::new(status)
                                .size(theme::ui_size(ui, 14.0))
                                .color(color),
                        );
                        if let Some(error) = tab.autosave_error() {
                            response.on_hover_text(error);
                        }
                    }
                });
            });
    }

    fn rail(&mut self, ctx: &egui::Context) {
        if self.light_mode {
            return;
        }
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        let colors = theme::Palette::of(ctx);
        egui::SidePanel::left("rail")
            .exact_width(64.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(colors.background)
                    .inner_margin(egui::Margin::symmetric(16, 16)),
            )
            .show(ctx, |ui| {
                for (panel, icon, tooltip) in [
                    (Panel::Vaults, Icon::Vaults, tr("Ваши хранилища")),
                    (Panel::Files, Icon::Folder, tr("Файлы")),
                    (Panel::Search, Icon::Search, tr("Поиск")),
                    (Panel::Plugins, Icon::Plugin, tr("Плагины")),
                ] {
                    if icons::selectable_button(
                        ui,
                        icon,
                        !self.settings_dialog && self.panel == panel,
                        tooltip,
                    )
                    .clicked()
                    {
                        self.panel = panel;
                        self.settings_dialog = false;
                        self.settings.show_sidebar = true;
                    }
                    ui.add_space(8.0);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    if icons::selectable_button(
                        ui,
                        Icon::Settings,
                        self.settings_dialog,
                        tr("Настройки"),
                    )
                    .clicked()
                    {
                        self.open_settings(ctx);
                    }
                });
            });
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        if self.settings_dialog || self.light_mode || !self.settings.show_sidebar {
            return;
        }
        let colors = theme::Palette::of(ctx);
        egui::SidePanel::left("sidebar")
            .default_width(264.0)
            .width_range(210.0..=420.0)
            .frame(
                egui::Frame::new()
                    .fill(colors.panel)
                    .inner_margin(egui::Margin::symmetric(16, 16))
                    .stroke(egui::Stroke::new(1.0, colors.border)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(match self.panel {
                            Panel::Vaults => tr("Ваши хранилища"),
                            Panel::Files => tr("Хранилище"),
                            Panel::Search => tr("Поиск"),
                            Panel::Plugins => tr("Плагины"),
                        })
                        .strong()
                        .size(theme::ui_size(ui, 15.0))
                        .color(colors.text),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if matches!(self.panel, Panel::Files | Panel::Search)
                            && icons::button(ui, Icon::Refresh, tr("Обновить файлы и индекс"))
                                .clicked()
                        {
                            self.refresh_files(ctx);
                        }
                    });
                });
                ui.add_space(16.0);
                match self.panel {
                    Panel::Vaults => self.vaults_panel(ui),
                    Panel::Files => self.file_tree(ui),
                    Panel::Search => self.search_panel(ui),
                    Panel::Plugins => self.plugins_panel(ui),
                }
            });
    }

    fn file_tree(&mut self, ui: &mut egui::Ui) {
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        let colors = theme::Palette::of(ui.ctx());
        let Some(vault) = &self.session.vault else {
            ui.label(RichText::new(tr("Откройте папку с заметками")).color(colors.muted));
            return;
        };
        let root_drop = ui.horizontal(|ui| {
            draw_icon(ui, Icon::Folder, 18.0, colors.muted);
            ui.label(
                RichText::new(
                    vault
                        .root()
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy(),
                )
                .strong()
                .size(theme::ui_size(ui, 14.0)),
            );
        });
        ui.add_space(12.0);
        search_field(ui, &mut self.filter, tr("Найти файл"), None);
        ui.add_space(12.0);
        let filter = self.filter.to_lowercase();
        let active_path = self
            .session
            .tabs
            .get(self.session.active)
            .map(|t| t.path.clone());
        let matching: std::collections::HashSet<&Path> = if filter.is_empty() {
            Default::default()
        } else {
            self.session
                .entries
                .iter()
                .filter(|entry| {
                    entry
                        .path
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&filter)
                })
                .flat_map(|entry| {
                    entry
                        .path
                        .ancestors()
                        .filter(|path| !path.as_os_str().is_empty())
                })
                .collect()
        };
        let mut open = None;
        let mut toggle = None;
        let mut delete = None;
        let mut rename = None;
        let mut movement = None;
        if let Some(payload) = root_drop.response.dnd_release_payload::<PathBuf>() {
            movement = Some((payload.as_ref().clone(), PathBuf::new()));
        }
        egui::ScrollArea::vertical()
            .id_salt("vault_files")
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                for entry in &self.session.entries {
                    if filter.is_empty() {
                        if entry
                            .path
                            .ancestors()
                            .skip(1)
                            .filter(|p| !p.as_os_str().is_empty())
                            .any(|p| !self.expanded_folders.contains(p))
                        {
                            continue;
                        }
                    } else if !matching.contains(entry.path.as_path()) {
                        continue;
                    }
                    let label = entry.path.file_name().unwrap_or_default().to_string_lossy();
                    let depth = entry.path.components().count().saturating_sub(1).min(12);
                    let active = active_path.as_ref() == Some(&entry.path);
                    let response = file_row(
                        ui,
                        &label,
                        depth,
                        entry.is_dir,
                        active,
                        if entry.is_dir {
                            if self.expanded_folders.contains(&entry.path) || !filter.is_empty() {
                                Icon::ChevronDown
                            } else {
                                Icon::ChevronRight
                            }
                        } else {
                            icons::for_path(&entry.path)
                        },
                    )
                    .on_hover_text(entry.path.to_string_lossy());
                    if response.clicked() {
                        if entry.is_dir {
                            toggle = Some(entry.path.clone());
                        } else {
                            open = Some(entry.path.clone());
                        }
                    }
                    response.dnd_set_drag_payload(entry.path.clone());
                    if entry.is_dir {
                        if response.dnd_hover_payload::<PathBuf>().is_some() {
                            ui.painter().rect_stroke(
                                response.rect,
                                8.0,
                                egui::Stroke::new(1.5, colors.accent),
                                egui::StrokeKind::Inside,
                            );
                        }
                        if let Some(payload) = response.dnd_release_payload::<PathBuf>() {
                            movement = Some((payload.as_ref().clone(), entry.path.clone()));
                        }
                    }
                    response.context_menu(|ui| {
                        if ui
                            .button(if entry.is_dir {
                                tr("Rename folder")
                            } else {
                                tr("Rename file")
                            })
                            .clicked()
                        {
                            rename = Some(entry.path.clone());
                            ui.close();
                        }
                        if ui
                            .button(if entry.is_dir {
                                tr("Delete folder")
                            } else {
                                tr("Delete file")
                            })
                            .clicked()
                        {
                            delete = Some((entry.path.clone(), entry.is_dir));
                            ui.close();
                        }
                    });
                }
            });
        if let Some(path) = toggle {
            if !self.expanded_folders.remove(&path) {
                self.expanded_folders.insert(path);
            }
        }
        if let Some(path) = rename {
            self.rename_name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            self.rename_entry = Some(path);
            self.rename_focus = true;
            self.message = None;
        }
        if let Some((source, directory)) = movement {
            if let Some(name) = source.file_name() {
                let target = directory.join(name);
                match self.session.move_entry(&source, &target) {
                    Ok(()) => {
                        if !directory.as_os_str().is_empty() {
                            self.expanded_folders.insert(directory);
                        }
                        self.reindex(ui.ctx());
                    }
                    Err(error) => self.message = Some(error),
                }
            }
        }
        if delete.is_some() {
            self.delete_entry = delete;
        }
        if let Some(path) = open {
            if let Err(error) = self.session.open_file(&path) {
                self.message = Some(error);
            }
        }
    }

    fn search_panel(&mut self, ui: &mut egui::Ui) {
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        let colors = theme::Palette::of(ui.ctx());
        let query_id = egui::Id::new("vault-search-query");
        if self.focus_vault_search {
            ui.memory_mut(|memory| memory.request_focus(query_id));
            self.focus_vault_search = false;
        }
        let field = search_field(
            ui,
            &mut self.vault_query,
            tr("Текст, тег или заголовок"),
            Some(query_id),
        );
        if field.has_focus()
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.panel = Panel::Files;
        }
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.vault_search_options.case_sensitive, "Aa")
                .on_hover_text(tr("Учитывать регистр"));
            ui.checkbox(&mut self.vault_search_options.regex, ".*")
                .on_hover_text(tr("Регулярное выражение"));
        });
        let recompile = self.vault_search_cache.as_ref().is_none_or(|cache| {
            cache.0 != self.vault_query || cache.1 != self.vault_search_options
        });
        if recompile {
            self.vault_search_cache = Some((
                self.vault_query.clone(),
                self.vault_search_options.clone(),
                SearchPattern::compile(&self.vault_query, &self.vault_search_options),
            ));
        }
        ui.add_space(12.0);
        ui.label(
            RichText::new(tr("По сохранённым заметкам"))
                .size(theme::ui_size(ui, 12.0))
                .color(colors.muted),
        );
        let mut open = None;
        if let Some(index) = &self.index {
            let pattern = self.vault_search_cache.as_ref().map(|cache| &cache.2);
            if let Some(Err(error)) = pattern {
                ui.label(error);
                return;
            }
            if recompile || self.vault_search_generation != self.index_generation {
                self.vault_search_results = index
                    .notes
                    .iter()
                    .enumerate()
                    .filter_map(|(index, note)| {
                        (self.vault_query.is_empty()
                            || pattern.is_some_and(|p| {
                                p.as_ref().is_ok_and(|p| {
                                    p.is_match(&note.text)
                                        || p.is_match(&note.path.to_string_lossy())
                                })
                            }))
                        .then_some(index)
                    })
                    .collect();
                self.vault_search_generation = self.index_generation;
            }
            let results: Vec<_> = self
                .vault_search_results
                .iter()
                .filter_map(|index_| index.notes.get(*index_))
                .collect();
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    lang.message("Found: {count}", &[("count", results.len().to_string())]),
                )
                .size(theme::ui_size(ui, 13.0))
                .color(colors.muted),
            );
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .id_salt("vault_search")
                .show(ui, |ui| {
                    for note in results {
                        let width = ui.available_width();
                        let card = egui::Frame::new()
                            .fill(colors.raised)
                            .corner_radius(10)
                            .inner_margin(egui::Margin::same(12))
                            .show(ui, |ui| {
                                ui.set_width((width - 24.0).max(1.0));
                                ui.add(
                                    egui::Label::new(RichText::new(&note.title).strong())
                                        .truncate(),
                                );
                                ui.label(
                                    RichText::new(note.path.to_string_lossy())
                                        .size(theme::ui_size(ui, 12.0))
                                        .color(colors.muted),
                                );
                                if !note.tags.is_empty() {
                                    ui.label(
                                        RichText::new(note.tags.join("  "))
                                            .size(theme::ui_size(ui, 12.0))
                                            .color(colors.muted),
                                    );
                                }
                            });
                        let response = ui.interact(
                            card.response.rect,
                            ui.id().with(("search-result", &note.path)),
                            egui::Sense::click(),
                        );
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                ui.is_enabled(),
                                &note.title,
                            )
                        });
                        if response.hovered() || response.has_focus() {
                            ui.painter().rect_stroke(
                                card.response.rect,
                                10.0,
                                egui::Stroke::new(1.0, colors.accent),
                                egui::StrokeKind::Inside,
                            );
                        }
                        if response.clicked() {
                            open = Some(note.path.clone());
                        }
                        ui.add_space(8.0);
                    }
                });
        } else {
            ui.label(RichText::new(tr("Подготавливаем индекс…")).color(colors.muted));
        }
        if let Some(path) = open {
            if let Err(error) = self.session.open_file(&path) {
                self.message = Some(error);
            }
        }
    }

    fn plugins_panel(&mut self, ui: &mut egui::Ui) {
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        let colors = theme::Palette::of(ui.ctx());
        ui.label(tr("Собственная система плагинов"));
        ui.label(
            RichText::new(tr("Любой язык · JSON по stdio"))
                .color(colors.muted)
                .size(theme::ui_size(ui, 11.0)),
        );
        ui.add_space(12.0);
        let field = ui.add(
            egui::TextEdit::singleline(&mut self.plugin_path)
                .hint_text(tr("GitHub repository URL or plugin folder"))
                .desired_width(f32::INFINITY),
        );
        let submit = (field.has_focus() || field.lost_focus())
            && ui.input(|input| input.key_pressed(egui::Key::Enter));
        let available = self.plugin_install_receiver.is_none();
        if ui
            .add_enabled(available, egui::Button::new(tr("Install plugin")))
            .clicked()
            || submit && available
        {
            self.start_plugin_install(self.plugin_path.trim().into(), ui.ctx());
        }
        if ui
            .add_enabled(
                available && self.portal_receiver.is_none(),
                egui::Button::new(tr("Choose plugin folder…")),
            )
            .clicked()
        {
            self.plugin_install_folder = true;
            self.portal_folder = true;
            self.portal_receiver = Some(portal::choose(
                true,
                tr("Choose plugin folder…").into(),
                ui.ctx().clone(),
            ));
        }
        if self.plugin_install_receiver.is_some() {
            ui.spinner();
        }
        ui.add_space(12.0);
        let mut selected = None;
        for plugin in &self.installed_plugins {
            if ui
                .add_enabled(
                    self.plugin_worker.is_none(),
                    egui::Button::new(&plugin.name),
                )
                .clicked()
            {
                selected = Some(plugin.manifest.clone());
            }
        }
        if let Some(path) = selected {
            self.plugin_path = path.to_string_lossy().into_owned();
            self.load_plugin_manifest();
        }
        ui.collapsing(tr("Open manifest directly"), |ui| {
            if ui
                .add_enabled(
                    self.plugin_worker.is_none(),
                    egui::Button::new(tr("Открыть манифест")),
                )
                .clicked()
            {
                self.load_plugin_manifest();
            }
        });
        if self.plugin_busy {
            ui.spinner();
        }
        if self.plugin_worker.is_some() {
            ui.add_space(12.0);
            ui.checkbox(
                &mut self.share_document,
                tr("Передавать текст текущего документа"),
            );
            ui.label(
                RichText::new(tr(
                    "Передаётся весь UTF-8 текст. Результат можно проверить перед применением.",
                ))
                .size(theme::ui_size(ui, 11.0))
                .color(colors.muted),
            );
            let mut invoke = None;
            for (id, title) in &self.plugin_commands {
                if ui
                    .add_enabled(!self.plugin_busy, egui::Button::new(title))
                    .clicked()
                {
                    invoke = Some(id.clone());
                }
            }
            if let Some(command) = invoke {
                self.invoke_plugin(command);
            }
            let mut selected_view = None;
            for view in &self.plugin_views {
                let available = self
                    .session
                    .tabs
                    .get(self.session.active)
                    .is_some_and(|tab| {
                        tab.path
                            .extension()
                            .and_then(|extension| extension.to_str())
                            .is_some_and(|extension| {
                                view.extensions
                                    .iter()
                                    .any(|candidate| candidate.eq_ignore_ascii_case(extension))
                            })
                    });
                if ui
                    .add_enabled(
                        available && !self.plugin_busy,
                        egui::Button::new(&view.title),
                    )
                    .on_hover_text(view.extensions.join(", "))
                    .clicked()
                {
                    selected_view = Some(view.clone());
                }
            }
            if let Some(view) = selected_view {
                if let Some(tab) = self.session.tabs.get_mut(self.session.active) {
                    if view.editable {
                        match tab.editor.set_mode(EditorMode::Source) {
                            Ok(()) => tab.plugin_autosave = true,
                            Err(error) => {
                                self.message = Some(error.to_string());
                                return;
                            }
                        }
                    }
                }
                self.preview = Some(plugin_preview::Preview::new(view));
            }
            if ui.button(tr("Остановить плагин")).clicked() {
                if let Some(worker) = &self.plugin_worker {
                    let _ = worker.sender.send(PluginRequest::Stop);
                }
                self.plugin_commands.clear();
                self.plugin_busy = true;
                self.plugin_input = None;
            }
        }
    }

    fn start_plugin_install(&mut self, source: String, ctx: &egui::Context) {
        if self.plugin_install_receiver.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.plugin_install_receiver = Some(receiver);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = diamond_plugin::install::install(&source, &plugin_directory());
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }

    fn update_preview(&mut self, ctx: &egui::Context) {
        let Some(preview) = &mut self.preview else {
            return;
        };
        let Some(tab) = self
            .session
            .tabs
            .get(self.session.active)
            .filter(|tab| preview.accepts(&tab.path))
        else {
            return;
        };
        let key = tab.editor.revision_key();
        if preview.observed != Some(key) {
            preview.observed = Some(key);
            preview.changed_at = Instant::now();
        }
        if preview.sent == Some(key) && preview.event.is_none() {
            return;
        }
        if self.plugin_busy {
            return;
        }
        let delay = Duration::from_millis(250);
        if preview.sent.is_some() && preview.changed_at.elapsed() < delay {
            ctx.request_repaint_after(delay.saturating_sub(preview.changed_at.elapsed()));
            return;
        }
        let Some(vault) = &self.session.vault else {
            return;
        };
        let text = if matches!(
            tab.editor.mode(),
            EditorMode::Source | EditorMode::Formatted
        ) {
            tab.editor.source_text()
        } else {
            None
        };
        let document = diamond_plugin::ViewDocument {
            view: preview.view.id.clone(),
            document_id: preview.document_id.clone(),
            revision: key.1,
            file_path: vault.root().join(&tab.path).to_string_lossy().into_owned(),
            text,
        };
        if let Some(worker) = &self.plugin_worker {
            match worker.sender.send(PluginRequest::Preview {
                document,
                event: preview.event.take(),
            }) {
                Ok(()) => {
                    preview.sent = Some(key);
                    self.plugin_busy = true;
                }
                Err(error) => self.message = Some(error.to_string()),
            }
        }
    }

    fn invoke_plugin(&mut self, command: String) {
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        self.plugin_input = None;
        let mut text = None;
        if self.share_document {
            let Some(tab) = self.session.tabs.get(self.session.active) else {
                self.message = Some(tr("Сначала откройте текстовый документ").into());
                return;
            };
            let bytes = match tab.editor.bytes() {
                Ok(bytes) => bytes,
                Err(error) => {
                    self.message = Some(error.to_string());
                    return;
                }
            };
            match String::from_utf8(bytes.clone()) {
                Ok(content) => {
                    text = Some(content);
                    if let Some(vault) = &self.session.vault {
                        self.plugin_input =
                            Some((vault.root().to_path_buf(), tab.path.clone(), bytes));
                    }
                }
                Err(_) => {
                    self.message = Some(tr("Бинарный документ нельзя передать как текст").into());
                    return;
                }
            }
        }
        if let Some(worker) = &self.plugin_worker {
            match worker.sender.send(PluginRequest::Invoke { command, text }) {
                Ok(()) => self.plugin_busy = true,
                Err(error) => self.message = Some(error.to_string()),
            }
        }
    }

    fn inspector(&mut self, ctx: &egui::Context) {
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        let colors = theme::Palette::of(ctx);
        if self.settings_dialog
            || self.light_mode
            || !self.settings.show_inspector
            || ctx.available_rect().width() < 620.0
        {
            return;
        }
        egui::SidePanel::right("inspector")
            .default_width(256.0)
            .width_range(210.0..=360.0)
            .frame(
                egui::Frame::new()
                    .fill(colors.panel)
                    .inner_margin(egui::Margin::symmetric(16, 16))
                    .stroke(egui::Stroke::new(1.0, colors.border)),
            )
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(tr("Структура и связи"))
                        .size(theme::ui_size(ui, 15.0))
                        .strong(),
                );
                ui.add_space(24.0);
                let Some(tab) = self.session.tabs.get(self.session.active) else {
                    ui.label(RichText::new(tr("Выберите заметку")).color(colors.muted));
                    return;
                };
                section_title(ui, tr("Заголовки"));
                let mut found = false;
                if tab.editor.mode() == EditorMode::Formatted
                    && let Some(document) = tab.editor.document()
                {
                    for block in document.blocks() {
                        if let diamond_document::BlockKind::Heading(level) = block.kind {
                            ui.horizontal(|ui| {
                                ui.add_space(f32::from(level.saturating_sub(1)) * 12.0);
                                ui.label(
                                    RichText::new(&block.text)
                                        .size(theme::ui_size(ui, 14.0))
                                        .color(colors.text),
                                );
                            });
                            found = true;
                        }
                    }
                }
                if !found {
                    ui.label(
                        RichText::new(tr("Нет заголовков в этом режиме"))
                            .size(theme::ui_size(ui, 13.0))
                            .color(colors.muted),
                    );
                }
                ui.add_space(24.0);
                section_title(ui, tr("Обратные ссылки"));
                let mut open = None;
                if let Some(index) = &self.index {
                    let backlinks = index.backlinks(&tab.path);
                    if backlinks.is_empty() {
                        ui.label(
                            RichText::new(tr("Пока нет ссылок"))
                                .size(theme::ui_size(ui, 13.0))
                                .color(colors.muted),
                        );
                    }
                    for note in backlinks {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(&note.title).size(theme::ui_size(ui, 14.0)),
                                )
                                .frame(false),
                            )
                            .clicked()
                        {
                            open = Some(note.path.clone());
                        }
                    }
                    if let Some(note) = index.note(&tab.path) {
                        ui.add_space(24.0);
                        section_title(ui, tr("Задачи"));
                        let tasks: Vec<_> = tab
                            .editor
                            .document()
                            .filter(|_| tab.editor.mode() == EditorMode::Formatted)
                            .map(|document| {
                                document
                                    .blocks()
                                    .iter()
                                    .filter_map(|block| {
                                        if let diamond_document::BlockKind::Task(checked) =
                                            block.kind
                                        {
                                            Some(checked)
                                        } else {
                                            None
                                        }
                                    })
                                    .collect()
                            })
                            .unwrap_or_else(|| {
                                note.tasks.iter().map(|task| task.checked).collect()
                            });
                        let done = tasks.iter().filter(|checked| **checked).count();
                        ui.label(
                            RichText::new(lang.message(
                                "{done} of {count} completed",
                                &[
                                    ("done", done.to_string()),
                                    ("count", tasks.len().to_string()),
                                ],
                            ))
                            .size(theme::ui_size(ui, 14.0)),
                        );
                        if !tasks.is_empty() {
                            let target = done as f32 / tasks.len() as f32;
                            let value = if self.settings.reduced_motion {
                                target
                            } else {
                                ctx.animate_value_with_time(
                                    egui::Id::new(("task-progress", &tab.path)),
                                    target,
                                    0.25,
                                )
                            };
                            ui.add_space(4.0);
                            ui.add(
                                egui::ProgressBar::new(value)
                                    .fill(colors.accent)
                                    .desired_height(8.0),
                            );
                        }
                        if !note.tags.is_empty() {
                            ui.add_space(24.0);
                            section_title(ui, tr("Теги"));
                            ui.horizontal_wrapped(|ui| {
                                for tag in &note.tags {
                                    egui::Frame::new()
                                        .fill(colors.raised)
                                        .corner_radius(8)
                                        .inner_margin(egui::Margin::symmetric(8, 4))
                                        .show(ui, |ui| {
                                            ui.label(
                                                RichText::new(tag)
                                                    .size(theme::ui_size(ui, 12.0))
                                                    .color(colors.muted),
                                            );
                                        });
                                }
                            });
                        }
                    }
                }
                ui.add_space(24.0);
                ui.label(
                    RichText::new(tr("Связи и задачи обновляются после сохранения"))
                        .size(theme::ui_size(ui, 12.0))
                        .color(colors.muted),
                );
                if let Some(path) = open {
                    if let Err(error) = self.session.open_file(&path) {
                        self.message = Some(error);
                    }
                }
            });
    }

    fn editor_panel(&mut self, ctx: &egui::Context) {
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        let colors = theme::Palette::of(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(colors.background))
            .show(ctx, |ui| {
                if self.session.tabs.is_empty() && !self.settings_tab_open {
                    self.welcome(ui);
                    return;
                }
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut activate = None;
                let mut close = None;
                egui::Frame::new()
                    .fill(colors.panel)
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        egui::ScrollArea::horizontal()
                            .auto_shrink([false, true])
                            .id_salt("tab_scroll")
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    for (index, tab) in self.session.tabs.iter().enumerate() {
                                        let active =
                                            !self.settings_dialog && index == self.session.active;
                                        egui::Frame::new()
                                            .fill(if active { colors.accent } else { colors.panel })
                                            .corner_radius(10)
                                            .inner_margin(egui::Margin::symmetric(8, 3))
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    if active {
                                                        ui.visuals_mut()
                                                            .widgets
                                                            .inactive
                                                            .fg_stroke
                                                            .color = colors.background;
                                                    }
                                                    draw_icon(
                                                        ui,
                                                        if tab.is_dirty() {
                                                            Icon::Dot
                                                        } else {
                                                            icons::for_path(&tab.path)
                                                        },
                                                        16.0,
                                                        if active {
                                                            colors.background
                                                        } else {
                                                            colors.muted
                                                        },
                                                    );
                                                    if ui
                                                        .add(
                                                            egui::Button::new(
                                                                RichText::new(tab.title())
                                                                    .size(theme::ui_size(ui, 14.0))
                                                                    .color(if active {
                                                                        colors.background
                                                                    } else {
                                                                        colors.muted
                                                                    }),
                                                            )
                                                            .frame(false),
                                                        )
                                                        .on_hover_text(tab.path.to_string_lossy())
                                                        .clicked()
                                                    {
                                                        activate = Some(index);
                                                    }
                                                    if icons::button(
                                                        ui,
                                                        Icon::Close,
                                                        tr("Закрыть вкладку"),
                                                    )
                                                    .clicked()
                                                    {
                                                        close = Some(index);
                                                    }
                                                });
                                            });
                                    }
                                    if self.settings_tab_open {
                                        egui::Frame::new()
                                            .fill(if self.settings_dialog {
                                                ui.visuals().faint_bg_color
                                            } else {
                                                ui.visuals().panel_fill
                                            })
                                            .corner_radius(10)
                                            .inner_margin(egui::Margin::symmetric(8, 3))
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    if icons::label_button(
                                                        ui,
                                                        Icon::Settings,
                                                        tr("Настройки"),
                                                    )
                                                    .clicked()
                                                    {
                                                        self.open_settings(ctx);
                                                    }
                                                    if icons::button(
                                                        ui,
                                                        Icon::Close,
                                                        tr("Закрыть настройки"),
                                                    )
                                                    .clicked()
                                                    {
                                                        self.settings_tab_open = false;
                                                        self.settings_dialog = false;
                                                    }
                                                });
                                            });
                                    }
                                });
                            });
                    });
                if let Some(index) = activate {
                    self.settings_dialog = false;
                    self.session.activate_tab(index, Instant::now());
                }
                if let Some(index) = close {
                    self.request_action(PendingAction::CloseTab(index), ctx);
                }
                if self.settings_dialog {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::same(24))
                        .show(ui, |ui| {
                            let changes = settings_ui::show(ui, &mut self.settings, &self.fonts);
                            if changes.reset {
                                let last = self.settings.last_vault.clone();
                                let recent = self.settings.recent_vaults.clone();
                                let language = self.settings.language;
                                self.settings = AppSettings::default();
                                self.settings.last_vault = last;
                                self.settings.recent_vaults = recent;
                                self.settings.language = language;
                            }
                            if changes.changed || changes.reset {
                                theme::configure(ctx, &self.settings);
                                if changes.font_changed || changes.reset {
                                    if let Some(error) = theme::install_font(ctx, &self.settings) {
                                        self.message = Some(error);
                                    }
                                }
                                self.prefs_dirty_at = Some(Instant::now());
                                ctx.request_repaint();
                                ctx.request_repaint_after(Duration::from_millis(400));
                            }
                        });
                    return;
                }
                let active = self.session.active;
                let mut follow_link = None;
                let mut edited = false;
                let mut composing = false;
                if let Some(tab) = self.session.tabs.get_mut(active) {
                    let area = ui.available_rect_before_wrap();
                    let preview_active = self
                        .preview
                        .as_ref()
                        .is_some_and(|preview| preview.accepts(&tab.path));
                    let preview_only = preview_active
                        && self
                            .preview
                            .as_ref()
                            .is_some_and(|preview| !preview.view.editable);
                    let split = if preview_active && !preview_only {
                        area.width() * 0.52
                    } else {
                        area.width()
                    };
                    let preview_rect = egui::Rect::from_min_max(
                        egui::pos2(
                            if preview_only {
                                area.left()
                            } else {
                                area.left() + split + 8.0
                            },
                            area.top(),
                        ),
                        area.max,
                    );
                    let editor_area = egui::Rect::from_min_max(
                        area.min,
                        egui::pos2(area.left() + split, area.bottom()),
                    );
                    let width = (editor_area.width() - 48.0).max(1.0).min(
                        if tab.editor.mode() == EditorMode::Formatted {
                            self.settings.editor_width.clamp(480.0, 1600.0)
                        } else {
                            f32::INFINITY
                        },
                    );
                    let rect = egui::Rect::from_center_size(
                        editor_area.center(),
                        egui::vec2(width, editor_area.height()),
                    );
                    ui.allocate_rect(area, egui::Sense::hover());
                    if !preview_only {
                        ui.scope_builder(
                            egui::UiBuilder::new()
                                .max_rect(rect)
                                .layout(egui::Layout::top_down(egui::Align::Min)),
                            |ui| {
                                ui.set_min_height(rect.height());
                                let family = if tab.editor.mode() != EditorMode::Formatted
                                    && self.settings.editor_font == "sans"
                                    && self.settings.editor_font_path.is_none()
                                {
                                    egui::FontFamily::Monospace
                                } else {
                                    theme::font_family(&self.settings)
                                };
                                tab.editor.set_language(self.settings.language);
                                tab.editor.set_view_options(
                                    family,
                                    self.settings.word_wrap,
                                    self.settings.line_numbers,
                                );
                                let size = if tab.editor.mode() == EditorMode::Formatted {
                                    self.settings.editor_font_size
                                } else {
                                    self.settings.code_font_size
                                };
                                let response = tab.editor.ui_with_options(ui, size);
                                edited = response.changed;
                                composing = response.composing;
                                follow_link = response.open_link;
                                if let Some(error) = response.error {
                                    self.message = Some(error);
                                }
                            },
                        );
                    }
                    if preview_active {
                        let mut close = false;
                        if let Some(preview) = &mut self.preview {
                            let key = tab.editor.revision_key();
                            let id = format!("{}:{}", key.0, tab.path.display());
                            if preview.document_id != id {
                                preview.clear_document(id);
                            }
                            ui.scope_builder(egui::UiBuilder::new().max_rect(preview_rect), |ui| {
                                ui.set_clip_rect(preview_rect.intersect(ui.clip_rect()));
                                let result = preview.ui(
                                    ui,
                                    tab.editor.preview_offset(),
                                    self.settings.language,
                                );
                                close = result.0;
                                if let Some(offset) = result.1 {
                                    tab.editor.jump_to_source(offset);
                                }
                            });
                        }
                        if close {
                            self.preview = None;
                            tab.plugin_autosave = false;
                        }
                    }
                }
                self.session
                    .set_composing(active, composing, Instant::now());
                if edited {
                    self.session.note_edited(active, Instant::now());
                }
                if let Some(target) = follow_link {
                    if target.starts_with("https://") || target.starts_with("http://") {
                        ctx.open_url(egui::OpenUrl::new_tab(target));
                    } else {
                        let path = self.session.tabs.get(active).and_then(|tab| {
                            self.index.as_ref().and_then(|index| {
                                index.resolve_link(
                                    &tab.path,
                                    target.split('#').next().unwrap_or(&target),
                                )
                            })
                        });
                        match path {
                            Some(path) => {
                                if let Err(error) = self.session.open_file(&path) {
                                    self.message = Some(error);
                                }
                            }
                            None => {
                                self.message = Some(
                                    lang.message("Link not found: {target}", &[("target", target)]),
                                )
                            }
                        }
                    }
                }
            });
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        let colors = theme::Palette::of(ui.ctx());
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.20).max(40.0));
            draw_icon(ui, Icon::Diamond, 72.0, Color32::from_gray(194));
            ui.add_space(20.0);
            ui.label(
                RichText::new(tr("Место для ваших мыслей"))
                    .size(theme::ui_size(ui, 30.0))
                    .strong(),
            );
            ui.add_space(12.0);
            ui.label(
                RichText::new(tr("Заметки, исходники и связи"))
                    .size(theme::ui_size(ui, 16.0))
                    .color(colors.muted),
            );
            ui.label(
                RichText::new(tr("В обычной папке на вашем компьютере"))
                    .size(theme::ui_size(ui, 14.0))
                    .color(colors.muted),
            );
            ui.add_space(28.0);
            if icons::label_button(ui, Icon::Folder, tr("Открыть хранилище"))
                .on_hover_text("Ctrl+O")
                .clicked()
            {
                self.choose_vault(ui.ctx());
            }
            if self.session.vault.is_some()
                && icons::label_button(ui, Icon::Plus, tr("Создать заметку"))
                    .on_hover_text("Ctrl+N")
                    .clicked()
            {
                self.new_dialog = true;
            }
            ui.add_space(32.0);
            ui.label(
                RichText::new(tr("Заметки остаются у вас"))
                    .size(theme::ui_size(ui, 13.0))
                    .color(colors.muted),
            );
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        if self.open_dialog {
            let mut open = true;
            let mut submit = false;
            egui::Window::new(tr("Открыть хранилище"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    if let Some(message) = &self.message {
                        ui.label(message);
                    }
                    ui.label(tr("Полный путь к существующей папке:"));
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.vault_input).desired_width(430.0),
                    );
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        submit = true;
                    }
                    ui.add_space(8.0);
                    if ui.button(tr("Открыть")).clicked() {
                        submit = true;
                    }
                });
            self.open_dialog = open;
            if submit {
                self.request_action(PendingAction::SwitchVault, ctx);
            }
        }
        if self.new_dialog {
            let mut open = true;
            let mut submit = false;
            egui::Window::new(tr("Новая заметка"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(tr(
                        "Имя файла относительно хранилища, например Notes/Идея.md:",
                    ));
                    let field = ui
                        .add(egui::TextEdit::singleline(&mut self.note_input).desired_width(430.0));
                    submit |= (field.has_focus() || field.lost_focus())
                        && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if ui.button(tr("Создать")).clicked() {
                        submit = true;
                    }
                });
            self.new_dialog = open;
            if submit {
                let mut path = PathBuf::from(self.note_input.trim());
                if path.extension().is_none() {
                    path.set_extension("md");
                }
                match self.session.create_note(&path) {
                    Ok(()) => {
                        self.settings_dialog = false;
                        self.note_input.clear();
                        self.new_dialog = false;
                        if self.watcher.is_none() {
                            self.reindex(ctx);
                        }
                    }
                    Err(error) => self.message = Some(error),
                }
            }
        }
        self.rename_dialog(ctx);
        self.delete_dialog(ctx);
        self.close_dialog(ctx);
        self.plugin_dialog(ctx);
        self.palette_dialog(ctx);
    }

    fn rename_dialog(&mut self, ctx: &egui::Context) {
        let Some(path) = self.rename_entry.clone() else {
            return;
        };
        let language = self.settings.language;
        let tr = |key| language.text(key);
        let mut confirm = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("rename-file")).show(ctx, |ui| {
            ui.heading(tr("Rename file"));
            ui.label(path.to_string_lossy());
            let response =
                ui.add(egui::TextEdit::singleline(&mut self.rename_name).desired_width(360.0));
            if self.rename_focus {
                response.request_focus();
                self.rename_focus = false;
            }
            if let Some(error) = &self.message {
                ui.label(error);
            }
            confirm |= (response.has_focus() || response.lost_focus())
                && ui.input(|input| input.key_pressed(egui::Key::Enter));
            ui.horizontal(|ui| {
                confirm |= ui.button(tr("Rename")).clicked();
                cancel = ui.button(tr("Cancel")).clicked();
            });
        });
        if cancel {
            self.rename_entry = None;
        }
        if confirm {
            match self
                .session
                .rename_file(&path, Path::new(self.rename_name.trim()))
            {
                Ok(()) => {
                    self.rename_entry = None;
                    self.message = None;
                    self.reindex(ctx);
                }
                Err(error) => self.message = Some(error),
            }
        }
    }

    fn delete_dialog(&mut self, ctx: &egui::Context) {
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        let Some((path, directory)) = self.delete_entry.clone() else {
            return;
        };
        let dirty = self.session.tabs.iter().any(|tab| {
            (tab.path == path || (directory && tab.path.starts_with(&path))) && tab.is_dirty()
        });
        let mut confirm = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("delete-entry")).show(ctx, |ui| {
            ui.set_min_width(380.0);
            ui.heading(if directory {
                tr("Delete folder?")
            } else {
                tr("Delete file?")
            });
            ui.label(path.to_string_lossy());
            ui.label(tr(
                "Deletion is permanent. Folder contents will also be deleted.",
            ));
            if dirty {
                ui.strong(tr("Unsaved changes in these files will be discarded."));
            }
            ui.horizontal(|ui| {
                confirm = ui.button(tr("Delete permanently")).clicked();
                cancel = ui.button(tr("Cancel")).clicked();
            });
        });
        if cancel {
            self.delete_entry = None;
        }
        if confirm {
            match self.session.delete_entry(&path, directory) {
                Ok(()) => {
                    self.expanded_folders.retain(|p| !p.starts_with(&path));
                    self.reindex(ctx);
                }
                Err(error) => self.message = Some(error),
            }
            self.delete_entry = None;
        }
    }

    fn close_dialog(&mut self, ctx: &egui::Context) {
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        let Some(action) = self.pending else { return };
        let dirty = self.session.dirty_tabs(action);
        let mut choice = None;
        egui::Modal::new(egui::Id::new("unsaved")).show(ctx, |ui| {
            ui.set_min_width(390.0);
            ui.heading(tr("Есть несохранённые изменения"));
            ui.add_space(8.0);
            for &index in &dirty {
                ui.label(self.session.tabs[index].path.to_string_lossy());
            }
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ui.button(tr("Сохранить")).clicked() {
                    choice = Some(0);
                }
                if ui.button(tr("Не сохранять")).clicked() {
                    choice = Some(1);
                }
                if ui.button(tr("Отмена")).clicked() {
                    choice = Some(2);
                }
            });
        });
        match choice {
            Some(0) => {
                for index in dirty {
                    if let Err(error) = self.session.save(index) {
                        self.message = Some(error);
                        return;
                    }
                }
                self.reindex(ctx);
                self.perform_action(action, ctx);
            }
            Some(1) => self.perform_action(action, ctx),
            Some(2) => self.pending = None,
            _ => {}
        }
    }

    fn plugin_dialog(&mut self, ctx: &egui::Context) {
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        let mut decision = None;
        if let Some(manifest) = &self.plugin_manifest {
            egui::Modal::new(egui::Id::new("plugin_launch")).show(ctx, |ui| {
                ui.set_max_width(560.0);
                ui.heading(lang.message("Run {name}?", &[("name", manifest.name.clone())]));
                ui.label(tr("Плагин запускается с правами вашего пользователя в системе. Протокол не является песочницей."));
                ui.add_space(8.0);
                ui.monospace(format!("{} {:?}", manifest.program, manifest.args));
                ui.label(lang.message("Protocol capabilities: {value}", &[("value", format!("{:?}",manifest.capabilities))]));
                ui.horizontal(|ui| {
                    if ui.button(tr("Запустить")).clicked() { decision = Some(true); }
                    if ui.button(tr("Отмена")).clicked() { decision = Some(false); }
                });
            });
        }
        if let Some(launch) = decision {
            if let Some(manifest) = self.plugin_manifest.take() {
                if launch {
                    self.plugin_busy = true;
                    self.plugin_worker = Some(PluginWorker::launch(manifest, ctx.clone()));
                }
            }
        }
        let mut apply = None;
        if let Some(replacement) = &self.replacement {
            egui::Modal::new(egui::Id::new("plugin_output")).show(ctx, |ui| {
                ui.set_max_width(650.0);
                ui.heading(tr("Результат плагина"));
                ui.label(lang.message(
                    "Replace text in {path}?",
                    &[("path", replacement.path.display().to_string())],
                ));
                egui::ScrollArea::vertical()
                    .max_height(330.0)
                    .show(ui, |ui| {
                        ui.monospace(&replacement.text);
                    });
                ui.horizontal(|ui| {
                    if ui.button(tr("Применить к документу")).clicked() {
                        apply = Some(true);
                    }
                    if ui.button(tr("Отклонить")).clicked() {
                        apply = Some(false);
                    }
                });
            });
        }
        if let Some(approved) = apply {
            if let Some(replacement) = self.replacement.take() {
                if approved {
                    if self
                        .session
                        .vault
                        .as_ref()
                        .is_none_or(|vault| vault.root() != replacement.vault_root)
                    {
                        self.message = Some(tr("Хранилище изменилось после запуска плагина. Результат не применён.").into());
                        return;
                    }
                    if let Some(index) = self
                        .session
                        .tabs
                        .iter()
                        .position(|t| t.path == replacement.path)
                    {
                        let tab = &mut self.session.tabs[index];
                        if tab
                            .editor
                            .bytes()
                            .is_ok_and(|bytes| bytes == replacement.before)
                        {
                            let mode = tab.editor.mode();
                            tab.editor =
                                EditorBuffer::for_path(&tab.path, replacement.text.as_bytes());
                            if let Err(error) = tab.editor.set_mode(mode) {
                                self.message = Some(error.to_string());
                            }
                            self.session.note_edited(index, Instant::now());
                        } else {
                            self.message = Some(tr("Документ изменился после запуска плагина. Результат не применён.").into());
                        }
                    } else {
                        self.message =
                            Some(tr("Исходная вкладка уже закрыта. Результат не применён.").into());
                    }
                }
            }
        }
    }

    fn palette_dialog(&mut self, ctx: &egui::Context) {
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        if !self.palette {
            return;
        }
        let mut open = true;
        let mut command = None;
        egui::Window::new(tr("Команды"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 90.0])
            .show(ctx, |ui| {
                ui.set_min_width(420.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.palette_query)
                        .hint_text(tr("Найти команду…")),
                );
                let query = self.palette_query.to_lowercase();
                for (id, title) in [
                    (0, tr("Открыть хранилище    Ctrl+O")),
                    (1, tr("Новая заметка    Ctrl+N")),
                    (2, tr("Сохранить    Ctrl+S")),
                    (3, tr("Поиск по хранилищу    Ctrl+Shift+F")),
                    (4, tr("Показать / скрыть структуру")),
                    (5, tr("Плагины")),
                    (6, tr("Настройки")),
                    (7, tr("Обновить файлы и индекс")),
                ] {
                    if id == 2
                        && self
                            .session
                            .tabs
                            .get(self.session.active)
                            .is_none_or(|tab| tab.autosaves())
                    {
                        continue;
                    }
                    if title.to_lowercase().contains(&query)
                        && ui.selectable_label(false, title).clicked()
                    {
                        command = Some(id);
                    }
                }
            });
        self.palette = open;
        if let Some(command) = command {
            self.palette = false;
            self.palette_query.clear();
            match command {
                0 => self.choose_vault(ctx),
                1 => self.new_dialog = self.session.vault.is_some(),
                2 => self.save_active(ctx),
                3 => {
                    self.panel = Panel::Search;
                    self.focus_vault_search = true;
                    self.settings_dialog = false;
                }
                4 => {
                    self.settings.show_inspector = !self.settings.show_inspector;
                    self.persist_settings();
                }
                5 => self.panel = Panel::Plugins,
                6 => self.open_settings(ctx),
                7 => self.refresh_files(ctx),
                _ => {}
            }
        }
    }
}

impl eframe::App for DiamondApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        let value = if visuals.dark_mode {
            17.0 / 255.0
        } else {
            250.0 / 255.0
        };
        [value, value, value, 1.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let lang = self.settings.language;
        let tr = |key| lang.text(key);
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.quit_after_plugin {
                self.request_action(PendingAction::Quit, ctx);
            }
        }
        self.poll_environment(ctx);
        self.poll_workers(ctx);
        let autosave = self.session.autosave_due(Instant::now());
        if autosave.saved > 0 && self.watcher.is_none() {
            self.reindex(ctx);
        }
        if let Some(error) = autosave.errors.into_iter().next() {
            self.message = Some(
                self.settings
                    .language
                    .message("Autosave failed: {error}", &[("error", error.to_string())]),
            );
        }
        if self.quit_after_plugin {
            if self.plugin_worker.is_none() {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.spinner();
                    ui.label(tr("Останавливаем плагин перед выходом…"));
                });
            });
            return;
        }
        let speed = self.settings.scroll_speed.clamp(0.25, 4.0);
        ctx.input_mut(|input| {
            input.smooth_scroll_delta *= speed;
            input.raw_scroll_delta *= speed;
        });
        self.shortcuts(ctx);
        let background = theme::Palette::of(ctx).background;
        let viewport = ctx
            .input(|input| input.viewport().inner_rect)
            .unwrap_or_else(|| ctx.available_rect());
        ctx.layer_painter(egui::LayerId::background())
            .rect_filled(viewport, 0.0, background);
        self.top_bar(ctx);
        self.status_bar(ctx);
        self.rail(ctx);
        self.sidebar(ctx);
        self.inspector(ctx);
        self.editor_panel(ctx);
        self.dialogs(ctx);
        self.update_preview(ctx);
        if self.toast_seen != self.message {
            self.toast_seen = self.message.clone();
            self.toast_until = self
                .message
                .as_ref()
                .map(|_| Instant::now() + Duration::from_secs(3));
        }
        if let Some(deadline) = self.toast_until {
            if Instant::now() >= deadline {
                self.message = None;
                self.toast_seen = None;
                self.toast_until = None;
            } else {
                if let Some(message) = &self.message {
                    egui::Area::new(egui::Id::new("notification"))
                        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -56.0))
                        .order(egui::Order::Foreground)
                        .show(ctx, |ui| {
                            egui::Frame::popup(ui.style())
                                .inner_margin(egui::Margin::same(14))
                                .show(ui, |ui| {
                                    ui.set_max_width(420.0);
                                    ui.label(message);
                                });
                        });
                }
                ctx.request_repaint_after(deadline.saturating_duration_since(Instant::now()));
            }
        }
        if let Some(delay) = self.session.next_autosave_in(Instant::now()) {
            ctx.request_repaint_after(delay);
        }
    }
}

fn draw_icon(ui: &mut egui::Ui, icon: Icon, size: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    icons::paint(ui.painter(), rect, icon, color);
}

fn section_title(ui: &mut egui::Ui, title: &str) {
    let colors = theme::Palette::of(ui.ctx());
    ui.label(
        RichText::new(title)
            .strong()
            .size(theme::ui_size(ui, 13.0))
            .color(colors.muted),
    );
    ui.add_space(8.0);
}

fn search_field(
    ui: &mut egui::Ui,
    text: &mut String,
    hint: &str,
    id: Option<egui::Id>,
) -> egui::Response {
    let colors = theme::Palette::of(ui.ctx());
    egui::Frame::new()
        .fill(colors.background)
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .stroke(egui::Stroke::new(1.0, colors.border))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                draw_icon(ui, Icon::Search, 16.0, colors.muted);
                let mut edit = egui::TextEdit::singleline(text)
                    .hint_text(hint)
                    .frame(false)
                    .desired_width(f32::INFINITY)
                    .min_size(egui::vec2(0.0, 36.0));
                if let Some(id) = id {
                    edit = edit.id(id);
                }
                ui.add(edit)
            })
            .inner
        })
        .inner
}

fn file_row(
    ui: &mut egui::Ui,
    label: &str,
    depth: usize,
    directory: bool,
    selected: bool,
    icon: Icon,
) -> egui::Response {
    let colors = theme::Palette::of(ui.ctx());
    let sense = egui::Sense::click_and_drag();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(
            ui.available_width(),
            (egui::TextStyle::Body.resolve(ui.style()).size + 14.0).max(34.0),
        ),
        sense,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::SelectableLabel, ui.is_enabled(), label)
    });
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(8),
            if selected {
                ui.visuals().selection.bg_fill
            } else {
                colors.raised
            },
        );
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.shrink(0.5),
            egui::CornerRadius::same(8),
            egui::Stroke::new(1.0, colors.muted),
            egui::StrokeKind::Inside,
        );
    }
    let x = rect.left() + 8.0 + depth as f32 * 16.0;
    let icon_rect =
        egui::Rect::from_center_size(egui::pos2(x + 8.0, rect.center().y), egui::vec2(16.0, 16.0));
    icons::paint(
        ui.painter(),
        icon_rect,
        icon,
        if selected {
            colors.background
        } else {
            colors.muted
        },
    );
    ui.painter().with_clip_rect(rect).text(
        egui::pos2(x + 24.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::TextStyle::Body.resolve(ui.style()),
        if selected {
            colors.background
        } else if directory {
            colors.muted
        } else {
            colors.text
        },
    );
    response
}

fn plugin_directory() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(".local/share")
        })
        .join("diamond/plugins")
}

fn expand_user_path(path: PathBuf) -> PathBuf {
    if let Ok(relative) = path.strip_prefix("~")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(relative);
    }
    path
}
