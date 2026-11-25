// SPDX-License-Identifier: GPL-3.0-only
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use diamond_editor::{EditorBuffer, EditorMode};
use diamond_vault::{Vault, VaultEntry};

pub const AUTOSAVE_DELAY: Duration = Duration::from_millis(600);

pub struct Tab {
    pub path: PathBuf,
    pub loaded: Vec<u8>,
    pub editor: EditorBuffer,
    pub plugin_autosave: bool,
    last_edit: Option<Instant>,
    autosave_error: Option<String>,
    composing: bool,
    dirty_cache: std::cell::Cell<Option<((u64, u64), bool)>>,
}

impl Tab {
    pub fn is_dirty(&self) -> bool {
        let key = self.editor.revision_key();
        if let Some((cached, dirty)) = self.dirty_cache.get() {
            if cached == key {
                return dirty;
            }
        }
        let dirty = !self.editor.matches_bytes(&self.loaded);
        self.dirty_cache.set(Some((key, dirty)));
        dirty
    }

    pub fn title(&self) -> String {
        self.path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }

    pub fn autosaves(&self) -> bool {
        (self.editor.mode() == EditorMode::Formatted
            && self
                .path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md")))
            || (self.plugin_autosave && self.editor.mode() == EditorMode::Source)
    }

    pub fn autosave_error(&self) -> Option<&str> {
        self.autosave_error.as_deref()
    }
}

#[derive(Default)]
pub struct AutosaveReport {
    pub saved: usize,
    pub errors: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
pub enum PendingAction {
    CloseTab(usize),
    SwitchVault,
    Quit,
}

#[derive(Default)]
pub struct Session {
    pub vault: Option<Vault>,
    pub entries: Vec<VaultEntry>,
    pub tabs: Vec<Tab>,
    pub active: usize,
}

impl Session {
    /// A file argument opens no sibling tree, watcher, or vault index.
    pub fn open_single_file(&mut self, path: &Path) -> Result<(), String> {
        let path = path.canonicalize().map_err(|error| error.to_string())?;
        let parent = path.parent().ok_or("File has no parent")?;
        self.vault = Some(Vault::open(parent).map_err(|error| error.to_string())?);
        self.entries.clear();
        self.tabs.clear();
        self.active = 0;
        self.open_file(Path::new(path.file_name().ok_or("File has no name")?))
    }

    pub fn open_vault(&mut self, root: &Path) -> Result<(), String> {
        let vault = Vault::open(root).map_err(|e| e.to_string())?;
        let entries = vault.entries().map_err(|e| e.to_string())?;
        self.vault = Some(vault);
        self.entries = entries;
        self.tabs.clear();
        self.active = 0;
        Ok(())
    }

    pub fn refresh(&mut self) -> Result<(), String> {
        if let Some(vault) = &self.vault {
            self.entries = vault.entries().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn rename_file(&mut self, path: &Path, name: &Path) -> Result<(), String> {
        if name.components().count() != 1
            || !matches!(
                name.components().next(),
                Some(std::path::Component::Normal(_))
            )
        {
            return Err("Enter a filename without a path".into());
        }
        self.move_entry(path, &path.parent().unwrap_or(Path::new("")).join(name))
    }

    pub fn move_entry(&mut self, path: &Path, target: &Path) -> Result<(), String> {
        if self
            .tabs
            .iter()
            .any(|tab| !tab.path.starts_with(path) && tab.path.starts_with(target))
        {
            return Err("A destination file is already open".into());
        }
        self.deactivate_active(Instant::now());
        let vault = self.vault.as_ref().ok_or("Open a vault first")?;
        let target = vault
            .move_entry(path, target)
            .map_err(|error| error.to_string())?;
        for tab in &mut self.tabs {
            if let Ok(suffix) = tab.path.strip_prefix(path) {
                tab.path = if suffix.as_os_str().is_empty() {
                    target.clone()
                } else {
                    target.join(suffix)
                };
                tab.editor.retarget_path(&tab.path);
                tab.editor.set_image_loader(crate::resources::image_loader(
                    vault.clone(),
                    tab.path.clone(),
                ));
            }
        }
        self.refresh()
    }

    pub fn delete_entry(&mut self, path: &Path, directory: bool) -> Result<(), String> {
        self.vault
            .as_ref()
            .ok_or("Open a vault first")?
            .remove(path, directory)
            .map_err(|e| e.to_string())?;
        let active_path = self.tabs.get(self.active).map(|tab| tab.path.clone());
        self.tabs
            .retain(|tab| !(tab.path == path || (directory && tab.path.starts_with(path))));
        self.active = active_path
            .and_then(|active| self.tabs.iter().position(|tab| tab.path == active))
            .unwrap_or_else(|| self.active.min(self.tabs.len().saturating_sub(1)));
        self.refresh()
    }

    pub fn open_file(&mut self, path: &Path) -> Result<(), String> {
        if let Some(index) = self.tabs.iter().position(|t| t.path == path) {
            self.activate_tab(index, Instant::now());
            return Ok(());
        }
        let vault = self.vault.as_ref().ok_or("Open a vault first")?;
        let image = diamond_editor::is_image_path(path);
        let result = if image {
            vault.read_bounded(path, 16 * 1024 * 1024)
        } else {
            vault.read(path)
        };
        let (loaded, image_error) = match result {
            Ok(bytes) => (bytes, None),
            Err(error) if image => (Vec::new(), Some(error.to_string())),
            Err(error) => return Err(error.to_string()),
        };
        let mut editor = EditorBuffer::for_path(path, &loaded);
        if let Some(error) = image_error {
            editor.set_unreadable_image(error);
        }
        editor.set_image_loader(crate::resources::image_loader(vault.clone(), path.into()));
        self.cancel_tab_composition(self.active, Instant::now());
        self.tabs.push(Tab {
            path: path.to_path_buf(),
            loaded,
            editor,
            plugin_autosave: false,
            last_edit: None,
            autosave_error: None,
            composing: false,
            dirty_cache: std::cell::Cell::new(None),
        });
        self.active = self.tabs.len() - 1;
        Ok(())
    }

    fn cancel_tab_composition(&mut self, index: usize, now: Instant) {
        if let Some(tab) = self.tabs.get_mut(index) {
            if tab.composing {
                tab.editor.cancel_composition();
                tab.composing = false;
                tab.last_edit = Some(now);
            }
        }
    }

    pub fn activate_tab(&mut self, index: usize, now: Instant) {
        if index < self.tabs.len() && index != self.active {
            self.cancel_tab_composition(self.active, now);
            self.active = index;
        }
    }

    pub fn deactivate_active(&mut self, now: Instant) {
        self.cancel_tab_composition(self.active, now);
    }

    pub fn create_note(&mut self, path: &Path) -> Result<(), String> {
        let vault = self.vault.as_ref().ok_or("Open a vault first")?;
        let title = path.file_stem().unwrap_or_default().to_string_lossy();
        vault
            .create(path, format!("# {title}\n\n").as_bytes())
            .map_err(|e| e.to_string())?;
        self.refresh()?;
        self.open_file(path)
    }

    pub fn save(&mut self, index: usize) -> Result<(), String> {
        let vault = self.vault.as_ref().ok_or("Open a vault first")?;
        let tab = self.tabs.get_mut(index).ok_or("Tab no longer exists")?;
        if tab.composing {
            return Err("Завершите ввод текста через IME перед сохранением".into());
        }
        let bytes = tab.editor.bytes().map_err(|e| e.to_string())?;
        vault
            .save(&tab.path, &tab.loaded, &bytes)
            .map_err(|e| e.to_string())?;
        tab.loaded = bytes;
        tab.dirty_cache.set(None);
        tab.last_edit = None;
        tab.autosave_error = None;
        Ok(())
    }

    /// Record a real editor change, including mode transitions and plugin edits.
    pub fn note_edited(&mut self, index: usize, now: Instant) {
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.last_edit = Some(now);
            // Conflicts need an explicit resolution; typing must not start a
            // repeated save/error loop or erase the error indicator.
        }
    }

    pub fn set_composing(&mut self, index: usize, composing: bool, now: Instant) {
        if let Some(tab) = self.tabs.get_mut(index) {
            if tab.composing && !composing {
                tab.last_edit = Some(now);
            }
            tab.composing = composing;
        }
    }

    pub fn next_autosave_in(&self, now: Instant) -> Option<Duration> {
        self.tabs
            .iter()
            .filter(|tab| {
                tab.autosaves() && !tab.composing && tab.autosave_error.is_none() && tab.is_dirty()
            })
            .filter_map(|tab| {
                tab.last_edit.map(|edited| {
                    AUTOSAVE_DELAY.saturating_sub(now.saturating_duration_since(edited))
                })
            })
            .min()
    }

    /// Persist debounced formatted Markdown using the same byte conflict check
    /// as manual saves. Failed documents stay dirty and do not retry silently.
    pub fn autosave_due(&mut self, now: Instant) -> AutosaveReport {
        let due: Vec<usize> = self
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(index, tab)| {
                (tab.autosaves()
                    && !tab.composing
                    && tab.autosave_error.is_none()
                    && tab.is_dirty()
                    && tab.last_edit.is_some_and(|edited| {
                        now.saturating_duration_since(edited) >= AUTOSAVE_DELAY
                    }))
                .then_some(index)
            })
            .collect();
        let mut report = AutosaveReport::default();
        for index in due {
            match self.save(index) {
                Ok(()) => report.saved += 1,
                Err(error) => {
                    let message = format!("{}: {error}", self.tabs[index].path.display());
                    self.tabs[index].autosave_error = Some(message.clone());
                    report.errors.push(message);
                }
            }
        }
        report
    }

    /// Save formatted notes immediately before a tab close, vault switch or
    /// quit. A failed save leaves the original session and dirty bytes intact.
    pub fn flush_autosave(&mut self, action: PendingAction) -> Result<usize, String> {
        let targets: Vec<usize> = match action {
            PendingAction::CloseTab(index) => vec![index],
            PendingAction::SwitchVault | PendingAction::Quit => (0..self.tabs.len()).collect(),
        };
        let now = Instant::now();
        for index in targets {
            self.cancel_tab_composition(index, now);
        }
        let eligible: Vec<usize> = self
            .dirty_tabs(action)
            .into_iter()
            .filter(|&index| self.tabs[index].autosaves())
            .collect();
        let mut saved = 0;
        for index in eligible {
            if let Some(error) = self.tabs[index].autosave_error.clone() {
                return Err(error);
            }
            if let Err(error) = self.save(index) {
                let message = format!("{}: {error}", self.tabs[index].path.display());
                self.tabs[index].autosave_error = Some(message.clone());
                return Err(message);
            }
            saved += 1;
        }
        Ok(saved)
    }

    pub fn dirty_tabs(&self, action: PendingAction) -> Vec<usize> {
        match action {
            PendingAction::CloseTab(index) => self
                .tabs
                .get(index)
                .filter(|t| t.is_dirty())
                .map(|_| vec![index])
                .unwrap_or_default(),
            PendingAction::SwitchVault | PendingAction::Quit => self
                .tabs
                .iter()
                .enumerate()
                .filter_map(|(i, tab)| tab.is_dirty().then_some(i))
                .collect(),
        }
    }

    pub fn close_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.tabs.remove(index);
            if self.active > index {
                self.active -= 1;
            }
            self.active = self.active.min(self.tabs.len().saturating_sub(1));
        }
    }
}

pub fn config_path() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
        .map(|p| p.join("diamond/settings.json"))
}

pub fn load_settings() -> (diamond_core::AppSettings, Option<String>) {
    let Some(path) = config_path() else {
        return (Default::default(), None);
    };
    match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(settings) => (settings, None),
            Err(error) => (
                Default::default(),
                Some(format!("Cannot read settings: {error}")),
            ),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Default::default(), None),
        Err(error) => (
            Default::default(),
            Some(format!("Cannot read settings: {error}")),
        ),
    }
}

pub fn save_settings(settings: &diamond_core::AppSettings) -> Result<(), String> {
    use std::io::Write;
    let path = config_path().ok_or("No XDG config or home directory")?;
    let parent = path.parent().ok_or("Invalid configuration path")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    temporary.write_all(&bytes).map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    temporary.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_preserves_dirty_buffer_and_restricts_live_to_markdown() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a.md"), b"old").unwrap();
        let mut session = Session::default();
        session.open_vault(root.path()).unwrap();
        session.open_file(Path::new("a.md")).unwrap();
        session.tabs[0].editor = EditorBuffer::for_path(Path::new("a.md"), b"new text");
        session
            .rename_file(Path::new("a.md"), Path::new("a.txt"))
            .unwrap();
        assert_eq!(session.tabs[0].editor.mode(), EditorMode::Source);
        assert!(session.tabs[0].is_dirty());
        assert_eq!(session.tabs[0].editor.bytes().unwrap(), b"new text");
        assert_eq!(session.tabs[0].loaded, b"old");
        assert!(
            session.tabs[0]
                .editor
                .set_mode(EditorMode::Formatted)
                .is_err()
        );
        session.save(0).unwrap();
        assert_eq!(
            std::fs::read(root.path().join("a.txt")).unwrap(),
            b"new text"
        );
    }

    #[test]
    fn deletion_closes_descendants_but_failed_deletion_keeps_buffers() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("notes")).unwrap();
        std::fs::write(root.path().join("notes/a.md"), b"a").unwrap();
        std::fs::write(root.path().join("keep.md"), b"keep").unwrap();
        let mut session = Session::default();
        session.open_vault(root.path()).unwrap();
        session.open_file(Path::new("notes/a.md")).unwrap();
        session.open_file(Path::new("keep.md")).unwrap();
        assert!(session.delete_entry(Path::new("notes"), false).is_err());
        assert_eq!(session.tabs.len(), 2);
        session.delete_entry(Path::new("notes"), true).unwrap();
        assert_eq!(session.tabs.len(), 1);
        assert_eq!(session.active, 0);
        assert_eq!(session.tabs[0].path, Path::new("keep.md"));
    }

    #[test]
    fn conflict_retains_unsaved_editor_and_loaded_version() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("note.md"), "original\n").unwrap();
        let mut session = Session::default();
        session.open_vault(directory.path()).unwrap();
        session.open_file(Path::new("note.md")).unwrap();
        session.tabs[0].editor = EditorBuffer::from_bytes(b"my unsaved edit\n");
        std::fs::write(directory.path().join("note.md"), "external edit\n").unwrap();

        assert!(session.save(0).is_err());
        assert_eq!(session.tabs[0].loaded, b"original\n");
        assert_eq!(
            session.tabs[0].editor.bytes().unwrap(),
            b"my unsaved edit\n"
        );
        assert!(session.tabs[0].is_dirty());
        assert_eq!(
            std::fs::read(directory.path().join("note.md")).unwrap(),
            b"external edit\n"
        );
    }

    #[test]
    fn binary_open_and_save_preserves_every_byte() {
        let directory = tempfile::tempdir().unwrap();
        let bytes: Vec<u8> = (0..=255).collect();
        std::fs::write(directory.path().join("data.bin"), &bytes).unwrap();
        let mut session = Session::default();
        session.open_vault(directory.path()).unwrap();
        session.open_file(Path::new("data.bin")).unwrap();
        assert_eq!(session.tabs[0].editor.mode(), EditorMode::Hex);
        assert!(!session.tabs[0].is_dirty());
        session.save(0).unwrap();
        assert_eq!(
            std::fs::read(directory.path().join("data.bin")).unwrap(),
            bytes
        );
    }

    #[test]
    fn source_open_and_dirty_close_keep_the_active_document() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("code.rs"), "fn main() {}\n").unwrap();
        std::fs::write(directory.path().join("other.md"), "# Other\n").unwrap();
        let mut session = Session::default();
        session.open_vault(directory.path()).unwrap();
        session.open_file(Path::new("code.rs")).unwrap();
        assert_eq!(session.tabs[0].editor.mode(), EditorMode::Source);
        session.tabs[0].editor = EditorBuffer::from_bytes(b"fn main() { changed(); }\n");
        session.open_file(Path::new("other.md")).unwrap();
        assert_eq!(session.dirty_tabs(PendingAction::Quit), vec![0]);
        session.close_tab(0);
        assert_eq!(session.active, 0);
        assert_eq!(session.tabs[0].path, Path::new("other.md"));
    }

    fn session_with_file(name: &str, content: &[u8]) -> (tempfile::TempDir, Session) {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join(name), content).unwrap();
        let mut session = Session::default();
        session.open_vault(directory.path()).unwrap();
        session.open_file(Path::new(name)).unwrap();
        (directory, session)
    }

    fn replace_text(session: &mut Session, text: &str, now: Instant) {
        let tab = &mut session.tabs[0];
        tab.editor = EditorBuffer::for_path(&tab.path, text.as_bytes());
        session.note_edited(0, now);
    }

    #[test]
    fn formatted_note_autosaves_after_typing_stops() {
        let (directory, mut session) = session_with_file("note.md", b"old\n");
        let now = Instant::now();
        replace_text(&mut session, "# Новый текст 🦀\n", now);
        assert_eq!(
            session.autosave_due(now + Duration::from_millis(599)).saved,
            0
        );
        assert_eq!(
            std::fs::read(directory.path().join("note.md")).unwrap(),
            b"old\n"
        );
        let report = session.autosave_due(now + AUTOSAVE_DELAY);
        assert_eq!(report.saved, 1);
        assert!(report.errors.is_empty());
        assert!(!session.tabs[0].is_dirty());
        assert_eq!(
            std::fs::read_to_string(directory.path().join("note.md")).unwrap(),
            "# Новый текст 🦀\n"
        );
        assert!(session.next_autosave_in(now + AUTOSAVE_DELAY).is_none());
    }

    #[test]
    fn further_typing_restarts_autosave_delay() {
        let (_directory, mut session) = session_with_file("note.md", b"old");
        let now = Instant::now();
        replace_text(&mut session, "first", now);
        replace_text(&mut session, "second", now + Duration::from_millis(400));
        assert_eq!(session.autosave_due(now + AUTOSAVE_DELAY).saved, 0);
        assert_eq!(
            session
                .autosave_due(now + Duration::from_millis(1000))
                .saved,
            1
        );
    }

    #[test]
    fn autosave_conflict_keeps_text_and_does_not_retry() {
        let (directory, mut session) = session_with_file("note.md", b"old");
        let now = Instant::now();
        replace_text(&mut session, "my edit", now);
        std::fs::write(directory.path().join("note.md"), "external").unwrap();
        let report = session.autosave_due(now + AUTOSAVE_DELAY);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.saved, 0);
        assert!(session.tabs[0].is_dirty());
        assert!(session.tabs[0].autosave_error().is_some());
        assert_eq!(session.tabs[0].editor.bytes().unwrap(), b"my edit");
        assert_eq!(
            std::fs::read(directory.path().join("note.md")).unwrap(),
            b"external"
        );
        assert!(
            session
                .autosave_due(now + Duration::from_secs(5))
                .errors
                .is_empty()
        );
        replace_text(&mut session, "continued edit", now + Duration::from_secs(6));
        assert_eq!(session.autosave_due(now + Duration::from_secs(7)).saved, 0);
        assert!(
            session
                .next_autosave_in(now + Duration::from_secs(7))
                .is_none()
        );
    }

    #[test]
    fn source_and_hex_files_require_manual_save() {
        for (name, content) in [
            ("code.rs", &b"fn main() {}"[..]),
            ("binary.md", &b"\xff\x00"[..]),
        ] {
            let (directory, mut session) = session_with_file(name, content);
            assert!(!session.tabs[0].autosaves());
            session.note_edited(0, Instant::now());
            assert_eq!(
                session
                    .autosave_due(Instant::now() + Duration::from_secs(1))
                    .saved,
                0
            );
            assert_eq!(std::fs::read(directory.path().join(name)).unwrap(), content);
        }
        let (directory, mut session) = session_with_file("note.md", b"old");
        let now = Instant::now();
        replace_text(&mut session, "source change", now);
        session.tabs[0].editor.set_mode(EditorMode::Source).unwrap();
        assert_eq!(session.autosave_due(now + AUTOSAVE_DELAY).saved, 0);
        assert_eq!(
            std::fs::read(directory.path().join("note.md")).unwrap(),
            b"old"
        );
    }

    #[test]
    fn closing_formatted_note_flushes_before_debounce() {
        let (directory, mut session) = session_with_file("note.MD", b"old");
        replace_text(&mut session, "new", Instant::now());
        assert_eq!(
            session.flush_autosave(PendingAction::CloseTab(0)).unwrap(),
            1
        );
        assert!(session.dirty_tabs(PendingAction::CloseTab(0)).is_empty());
        assert_eq!(
            std::fs::read(directory.path().join("note.MD")).unwrap(),
            b"new"
        );
    }

    #[test]
    fn ime_preedit_never_reaches_disk() {
        let (directory, mut session) = session_with_file("note.md", b"old");
        let now = Instant::now();
        replace_text(&mut session, "composition", now);
        session.set_composing(0, true, now);
        assert_eq!(session.autosave_due(now + Duration::from_secs(5)).saved, 0);
        assert!(session.next_autosave_in(now).is_none());
        assert!(session.save(0).is_err());
        assert_eq!(
            std::fs::read(directory.path().join("note.md")).unwrap(),
            b"old"
        );
        let committed = now + Duration::from_secs(6);
        session.set_composing(0, false, committed);
        assert_eq!(session.autosave_due(committed).saved, 0);
        assert_eq!(session.autosave_due(committed + AUTOSAVE_DELAY).saved, 1);
        assert_eq!(
            std::fs::read(directory.path().join("note.md")).unwrap(),
            b"composition"
        );
    }

    #[test]
    fn dirty_cache_invalidates_on_real_edits_and_saves() {
        let (_directory, mut session) = session_with_file("note.md", b"old");
        assert!(!session.tabs[0].is_dirty());
        assert_eq!(session.tabs[0].editor.replace_all("old", "new").unwrap(), 1);
        assert!(session.tabs[0].is_dirty());
        assert!(session.tabs[0].is_dirty());
        session.save(0).unwrap();
        assert!(!session.tabs[0].is_dirty());
    }
}

#[cfg(test)]
mod single_file_tests {
    use super::*;
    #[test]
    fn single_file_does_not_enumerate_siblings_and_keeps_markdown_autosave() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("note.md"), b"# note\n").unwrap();
        std::fs::write(directory.path().join("sibling.md"), b"not opened").unwrap();
        let mut session = Session::default();
        session
            .open_single_file(&directory.path().join("note.md"))
            .unwrap();
        assert!(session.entries.is_empty());
        assert_eq!(session.tabs.len(), 1);
        assert!(session.tabs[0].autosaves());
        assert_eq!(session.tabs[0].editor.bytes().unwrap(), b"# note\n");
    }
}
