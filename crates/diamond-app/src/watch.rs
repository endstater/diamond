// SPDX-License-Identifier: GPL-3.0-only
//! OS notifications only: no PollWatcher, polling loop or periodic timer.
use diamond_vault::VaultEntry;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub struct VaultWatch {
    watcher: RecommendedWatcher,
    root: PathBuf,
    directories: HashSet<PathBuf>,
    dirty: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
}

impl VaultWatch {
    pub fn start(root: &Path, entries: &[VaultEntry], ctx: egui::Context) -> Result<Self, String> {
        let dirty = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let (changed, error) = (dirty.clone(), failed.clone());
        let root_for_events = root.to_path_buf();
        let mut watcher = notify::RecommendedWatcher::new(
            move |event: notify::Result<notify::Event>| match event {
                Ok(event) if !matches!(event.kind, EventKind::Access(_)) => {
                    let relevant = event.paths.is_empty()
                        || event
                            .paths
                            .iter()
                            .any(|path| watched_path(&root_for_events, path));
                    if relevant && !changed.swap(true, Ordering::AcqRel) {
                        ctx.request_repaint();
                    }
                }
                Err(_) => {
                    error.store(true, Ordering::Release);
                    ctx.request_repaint();
                }
                _ => {}
            },
            notify::Config::default().with_follow_symlinks(false),
        )
        .map_err(|error| error.to_string())?;
        // Watch only directories admitted by the vault, so .git, target and
        // node_modules do not consume native watch handles or generate storms.
        watcher
            .watch(root, RecursiveMode::NonRecursive)
            .map_err(|error| error.to_string())?;
        let mut result = Self {
            watcher,
            root: root.to_path_buf(),
            directories: HashSet::from([root.to_path_buf()]),
            dirty,
            failed,
        };
        result.sync_directories(entries)?;
        Ok(result)
    }
    pub fn sync_directories(&mut self, entries: &[VaultEntry]) -> Result<bool, String> {
        let wanted: HashSet<_> = std::iter::once(self.root.clone())
            .chain(
                entries
                    .iter()
                    .filter(|entry| entry.is_dir)
                    .map(|entry| self.root.join(&entry.path)),
            )
            .collect();
        let added: Vec<_> = wanted.difference(&self.directories).cloned().collect();
        let removed: Vec<_> = self.directories.difference(&wanted).cloned().collect();
        for path in &added {
            self.watcher
                .watch(path, RecursiveMode::NonRecursive)
                .map_err(|error| error.to_string())?;
            self.directories.insert(path.clone());
        }
        for path in removed {
            let _ = self.watcher.unwatch(&path);
            self.directories.remove(&path);
        }
        Ok(!added.is_empty())
    }
    pub fn take_changed(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }
    pub fn take_error(&self) -> bool {
        self.failed.swap(false, Ordering::AcqRel)
    }
}

fn watched_path(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root).is_ok_and(|relative| {
        !relative.components().any(|part| {
            let name = part.as_os_str().to_string_lossy();
            matches!(part, std::path::Component::ParentDir)
                || matches!(
                    name.as_ref(),
                    ".git" | ".diamond" | ".hg" | ".svn" | "target" | "node_modules"
                )
                || name.starts_with(".diamond-save-")
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn watches_real_unicode_files_and_excludes_access_artifacts() {
        let root = Path::new("/vault");
        assert!(watched_path(root, Path::new("/vault/Заметки/Новая.md")));
        assert!(!watched_path(root, Path::new("/outside/file.md")));
        assert!(!watched_path(root, Path::new("/vault/.git/index")));
        assert!(!watched_path(root, Path::new("/vault/.diamond-save-tmp")));
        assert!(!watched_path(root, Path::new("/vault/../outside.md")));
        let rename = [
            Path::new("/vault/.diamond-save-tmp"),
            Path::new("/vault/note.md"),
        ];
        assert!(rename.iter().any(|path| watched_path(root, path)));
    }
}
