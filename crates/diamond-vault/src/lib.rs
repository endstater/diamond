// SPDX-License-Identifier: GPL-3.0-only
//! Byte-preserving, conflict-aware access to an ordinary folder of files.

mod index;

pub use index::{HeadingMetadata, NoteMetadata, TaskMetadata, VaultIndex};

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use tempfile::NamedTempFile;
use thiserror::Error;

pub const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_INDEX_FILE_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_INDEX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 50_000;
const MAX_DEPTH: usize = 64;

pub type Result<T> = std::result::Result<T, VaultError>;

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("invalid vault-relative path: {0}")]
    InvalidPath(PathBuf),
    #[error("symbolic links are not allowed: {0}")]
    Symlink(PathBuf),
    #[error("not a regular file: {0}")]
    NotFile(PathBuf),
    #[error("not a directory: {0}")]
    NotDirectory(PathBuf),
    #[error("file exceeds the {limit}-byte limit: {path}")]
    TooLarge { path: PathBuf, limit: u64 },
    #[error("file changed on disk; reload or save a copy: {0}")]
    Conflict(PathBuf),
    #[error("file or directory already exists: {0}")]
    AlreadyExists(PathBuf),
    #[error("vault exceeds the enumeration limit ({0})")]
    EnumerationLimit(&'static str),
    #[error("filesystem error for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

fn io_error(path: &Path, source: std::io::Error) -> VaultError {
    VaultError::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultEntry {
    pub path: PathBuf,
    pub is_dir: bool,
}

#[derive(Debug, Clone)]
pub struct Vault {
    root: PathBuf,
}

impl Vault {
    /// Open an existing directory. Its canonical path becomes the boundary.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let root = fs::canonicalize(path).map_err(|e| io_error(path, e))?;
        if !root.is_dir() {
            return Err(VaultError::NotDirectory(root));
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Enumerate files and directories without following symlinks.
    pub fn entries(&self) -> Result<Vec<VaultEntry>> {
        let mut entries = Vec::new();
        self.walk(Path::new(""), 0, &mut entries)?;
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(entries)
    }

    fn walk(&self, relative: &Path, depth: usize, entries: &mut Vec<VaultEntry>) -> Result<()> {
        if depth > MAX_DEPTH {
            return Err(VaultError::EnumerationLimit("64 directory levels"));
        }
        let absolute = if relative.as_os_str().is_empty() {
            self.root.clone()
        } else {
            self.existing_path(relative)?
        };
        let directory = fs::read_dir(&absolute).map_err(|e| io_error(&absolute, e))?;
        for item in directory {
            let item = item.map_err(|e| io_error(&absolute, e))?;
            let name = item.file_name();
            let name_text = name.to_string_lossy();
            if ignored_name(&name_text) {
                continue;
            }
            let kind = item.file_type().map_err(|e| io_error(&item.path(), e))?;
            if kind.is_symlink() || (!kind.is_dir() && !kind.is_file()) {
                continue;
            }
            if entries.len() >= MAX_ENTRIES {
                return Err(VaultError::EnumerationLimit("50,000 entries"));
            }
            let path = relative.join(name);
            entries.push(VaultEntry {
                path: path.clone(),
                is_dir: kind.is_dir(),
            });
            if kind.is_dir() {
                self.walk(&path, depth + 1, entries)?;
            }
        }
        Ok(())
    }

    /// Move files or whole folders; never replace a destination or follow links.
    pub fn move_entry(&self, relative: &Path, target_relative: &Path) -> Result<PathBuf> {
        let source = self.existing_path(relative)?;
        if relative == target_relative {
            return Ok(relative.into());
        }
        if source.is_dir() && target_relative.starts_with(relative) {
            return Err(VaultError::InvalidPath(target_relative.into()));
        }
        let target = self.new_path(target_relative)?;
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            &source,
            rustix::fs::CWD,
            &target,
            rustix::fs::RenameFlags::NOREPLACE,
        )
        .map_err(|error| {
            let error = std::io::Error::from(error);
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                VaultError::AlreadyExists(target_relative.into())
            } else {
                io_error(&source, error)
            }
        })?;
        if let Some(parent) = source.parent() {
            let _ = sync_directory(parent);
        }
        if let Some(parent) = target.parent() {
            let _ = sync_directory(parent);
        }
        Ok(target_relative.into())
    }

    pub fn rename_file(&self, relative: &Path, name: &Path) -> Result<PathBuf> {
        if name.components().count() != 1
            || !matches!(name.components().next(), Some(Component::Normal(_)))
        {
            return Err(VaultError::InvalidPath(name.into()));
        }
        self.move_entry(
            relative,
            &relative.parent().unwrap_or(Path::new("")).join(name),
        )
    }

    /// Explicit, permanent deletion of a validated vault-relative entry.
    /// std::fs::remove_dir_all does not follow nested symbolic links.
    pub fn remove(&self, relative: &Path, directory: bool) -> Result<()> {
        let path = self.existing_path(relative)?;
        let metadata = fs::symlink_metadata(&path).map_err(|e| io_error(&path, e))?;
        if directory {
            if !metadata.is_dir() {
                return Err(VaultError::NotDirectory(relative.into()));
            }
            fs::remove_dir_all(&path).map_err(|e| io_error(&path, e))
        } else {
            if !metadata.is_file() {
                return Err(VaultError::NotFile(relative.into()));
            }
            fs::remove_file(&path).map_err(|e| io_error(&path, e))
        }
    }

    /// Read exact bytes. No encoding conversion is performed.
    pub fn read(&self, relative: &Path) -> Result<Vec<u8>> {
        self.read_bounded(relative, MAX_FILE_BYTES)
    }

    pub fn read_bounded(&self, relative: &Path, limit: u64) -> Result<Vec<u8>> {
        let limit = limit.min(MAX_FILE_BYTES);
        let path = self.existing_path(relative)?;
        let metadata = fs::metadata(&path).map_err(|e| io_error(&path, e))?;
        if !metadata.is_file() {
            return Err(VaultError::NotFile(relative.to_path_buf()));
        }
        if metadata.len() > limit {
            return Err(VaultError::TooLarge {
                path: relative.to_path_buf(),
                limit,
            });
        }
        let file = File::open(&path).map_err(|e| io_error(&path, e))?;
        let mut bytes = Vec::new();
        file.take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| io_error(&path, e))?;
        if bytes.len() as u64 > limit {
            return Err(VaultError::TooLarge {
                path: relative.to_path_buf(),
                limit,
            });
        }
        Ok(bytes)
    }

    /// Save only if the current disk bytes still equal the loaded version.
    pub fn save(&self, relative: &Path, expected: &[u8], bytes: &[u8]) -> Result<()> {
        self.check_size(relative, bytes)?;
        if self.read(relative)? != expected {
            return Err(VaultError::Conflict(relative.to_path_buf()));
        }
        let path = self.existing_path(relative)?;
        let parent = path
            .parent()
            .ok_or_else(|| VaultError::InvalidPath(relative.to_path_buf()))?;
        let permissions = fs::metadata(&path)
            .map_err(|e| io_error(&path, e))?
            .permissions();
        let mut temporary = NamedTempFile::with_prefix_in(".diamond-save-", parent)
            .map_err(|e| io_error(parent, e))?;
        temporary
            .as_file()
            .set_permissions(permissions)
            .map_err(|e| io_error(&path, e))?;
        temporary.write_all(bytes).map_err(|e| io_error(&path, e))?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|e| io_error(&path, e))?;
        if self.read(relative)? != expected {
            return Err(VaultError::Conflict(relative.to_path_buf()));
        }
        temporary
            .persist(&path)
            .map_err(|e| io_error(&path, e.error))?;
        sync_directory(parent)?;
        Ok(())
    }

    /// Atomically create a file without replacing an existing destination.
    pub fn create(&self, relative: &Path, bytes: &[u8]) -> Result<()> {
        self.check_size(relative, bytes)?;
        let path = self.new_path(relative)?;
        let parent = path
            .parent()
            .ok_or_else(|| VaultError::InvalidPath(relative.to_path_buf()))?;
        let mut temporary = NamedTempFile::with_prefix_in(".diamond-save-", parent)
            .map_err(|e| io_error(parent, e))?;
        temporary.write_all(bytes).map_err(|e| io_error(&path, e))?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|e| io_error(&path, e))?;
        temporary.persist_noclobber(&path).map_err(|e| {
            if e.error.kind() == std::io::ErrorKind::AlreadyExists {
                VaultError::AlreadyExists(relative.to_path_buf())
            } else {
                io_error(&path, e.error)
            }
        })?;
        sync_directory(parent)?;
        Ok(())
    }

    pub fn create_dir(&self, relative: &Path) -> Result<()> {
        let path = self.new_path(relative)?;
        fs::create_dir(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                VaultError::AlreadyExists(relative.to_path_buf())
            } else {
                io_error(&path, e)
            }
        })?;
        if let Some(parent) = path.parent() {
            sync_directory(parent)?;
        }
        Ok(())
    }

    pub fn build_index(&self) -> Result<VaultIndex> {
        self.build_index_reusing(None)
    }

    /// Event-triggered rebuild: reuse unchanged notes, re-read/parse only changed
    /// files. Metadata includes inode and ctime, not just coarse timestamps/size.
    pub fn build_index_reusing(&self, previous: Option<&VaultIndex>) -> Result<VaultIndex> {
        let mut index = VaultIndex::default();
        let mut remaining = MAX_INDEX_TOTAL_BYTES;
        let old: std::collections::HashMap<_, _> = previous
            .into_iter()
            .flat_map(|index| index.notes.iter())
            .map(|note| (note.path.as_path(), note))
            .collect();
        for entry in self.entries()? {
            if entry.is_dir
                || !entry
                    .path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
            {
                continue;
            }
            let absolute = self.existing_path(&entry.path)?;
            let metadata = fs::metadata(&absolute).map_err(|error| io_error(&absolute, error))?;
            let stamp = index::FileStamp::of(&metadata);
            let limit = MAX_INDEX_FILE_BYTES.min(remaining);
            if metadata.len() > limit {
                index.skipped.push(entry.path);
                continue;
            }
            if previous.and_then(|index| index.stamps.get(&entry.path)) == Some(&stamp)
                && let Some(note) = old.get(entry.path.as_path())
            {
                remaining -= note.text.len() as u64;
                index.notes.push((**note).clone());
                index.stamps.insert(entry.path, stamp);
                continue;
            }
            let bytes = match self.read_bounded(&entry.path, limit) {
                Ok(bytes) => bytes,
                Err(VaultError::TooLarge { .. }) => {
                    index.skipped.push(entry.path);
                    continue;
                }
                Err(error) => return Err(error),
            };
            let after = fs::metadata(&absolute).map_err(|error| io_error(&absolute, error))?;
            match String::from_utf8(bytes) {
                Ok(text) => {
                    remaining -= text.len() as u64;
                    if stamp == index::FileStamp::of(&after) && text.len() as u64 == metadata.len()
                    {
                        index.stamps.insert(entry.path.clone(), stamp);
                    }
                    if let Some(note) = old
                        .get(entry.path.as_path())
                        .filter(|note| note.text.as_bytes() == text.as_bytes())
                    {
                        index.notes.push((**note).clone());
                    } else {
                        index.notes.push(NoteMetadata::parse(entry.path, text));
                    }
                }
                Err(_) => index.skipped.push(entry.path),
            }
        }
        Ok(index)
    }

    fn check_size(&self, relative: &Path, bytes: &[u8]) -> Result<()> {
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(VaultError::TooLarge {
                path: relative.to_path_buf(),
                limit: MAX_FILE_BYTES,
            });
        }
        Ok(())
    }

    fn validate_relative(&self, relative: &Path) -> Result<()> {
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(VaultError::InvalidPath(relative.to_path_buf()));
        }
        Ok(())
    }

    fn existing_path(&self, relative: &Path) -> Result<PathBuf> {
        self.validate_relative(relative)?;
        let mut path = self.root.clone();
        for component in relative.components() {
            path.push(component.as_os_str());
            let metadata = fs::symlink_metadata(&path).map_err(|e| io_error(&path, e))?;
            if metadata.file_type().is_symlink() {
                return Err(VaultError::Symlink(relative.to_path_buf()));
            }
        }
        let canonical = fs::canonicalize(&path).map_err(|e| io_error(&path, e))?;
        if !canonical.starts_with(&self.root) {
            return Err(VaultError::InvalidPath(relative.to_path_buf()));
        }
        Ok(path)
    }

    fn new_path(&self, relative: &Path) -> Result<PathBuf> {
        self.validate_relative(relative)?;
        let parent = relative.parent().unwrap_or_else(|| Path::new(""));
        let parent_path = if parent.as_os_str().is_empty() {
            self.root.clone()
        } else {
            self.existing_path(parent)?
        };
        if !parent_path.is_dir() {
            return Err(VaultError::NotDirectory(parent.to_path_buf()));
        }
        let path = parent_path.join(
            relative
                .file_name()
                .ok_or_else(|| VaultError::InvalidPath(relative.to_path_buf()))?,
        );
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                Err(VaultError::Symlink(relative.to_path_buf()))
            }
            Ok(_) => Err(VaultError::AlreadyExists(relative.to_path_buf())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path),
            Err(error) => Err(io_error(&path, error)),
        }
    }
}

fn ignored_name(name: &str) -> bool {
    matches!(
        name,
        ".git" | ".hg" | ".svn" | ".diamond" | "node_modules" | "target"
    ) || name.starts_with(".diamond-save-")
}

fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|e| io_error(path, e))?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renaming_never_clobbers_files_or_escapes_the_vault() {
        let root = tempfile::tempdir().unwrap();
        let vault = Vault::open(root.path()).unwrap();
        fs::write(root.path().join("a.md"), b"original").unwrap();
        fs::write(root.path().join("b.md"), b"keep").unwrap();
        assert!(
            vault
                .rename_file(Path::new("a.md"), Path::new("b.md"))
                .is_err()
        );
        assert!(
            vault
                .rename_file(Path::new("a.md"), Path::new("../oops.md"))
                .is_err()
        );
        assert_eq!(vault.read(Path::new("b.md")).unwrap(), b"keep");
        assert_eq!(
            vault
                .rename_file(Path::new("a.md"), Path::new("new.md"))
                .unwrap(),
            Path::new("new.md")
        );
        assert!(!root.path().join("a.md").exists());
        assert_eq!(vault.read(Path::new("new.md")).unwrap(), b"original");
    }

    #[cfg(unix)]
    #[test]
    fn recursive_deletion_does_not_follow_links_outside_vault() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("keep.md"), b"keep").unwrap();
        fs::create_dir(root.path().join("notes")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("notes/link")).unwrap();
        let vault = Vault::open(root.path()).unwrap();
        assert!(vault.remove(Path::new("notes/link"), true).is_err());
        vault.remove(Path::new("notes"), true).unwrap();
        assert_eq!(fs::read(outside.path().join("keep.md")).unwrap(), b"keep");
    }

    #[test]
    fn deletion_is_bounded_and_checks_entry_kind() {
        let root = tempfile::tempdir().unwrap();
        let vault = Vault::open(root.path()).unwrap();
        fs::create_dir(root.path().join("notes")).unwrap();
        fs::write(root.path().join("notes/a.md"), b"hello").unwrap();
        assert!(vault.remove(Path::new(""), true).is_err());
        assert!(vault.remove(Path::new("../outside"), true).is_err());
        assert!(vault.remove(Path::new("notes/a.md"), true).is_err());
        assert!(root.path().join("notes/a.md").exists());
        vault.remove(Path::new("notes"), true).unwrap();
        assert!(!root.path().join("notes").exists());
    }

    #[test]
    fn stale_save_is_rejected_without_touching_disk() {
        let directory = tempfile::tempdir().unwrap();
        let vault = Vault::open(directory.path()).unwrap();
        vault.create(Path::new("note.md"), b"original").unwrap();
        let original = vault.read(Path::new("note.md")).unwrap();
        fs::write(directory.path().join("note.md"), b"external").unwrap();
        assert!(matches!(
            vault.save(Path::new("note.md"), &original, b"mine"),
            Err(VaultError::Conflict(_))
        ));
        assert_eq!(vault.read(Path::new("note.md")).unwrap(), b"external");
    }

    #[test]
    fn binary_save_preserves_every_byte_and_create_never_clobbers() {
        let directory = tempfile::tempdir().unwrap();
        let vault = Vault::open(directory.path()).unwrap();
        let bytes = [0, 255, 254, 13, 10, 128];
        vault.create(Path::new("image.bin"), &bytes).unwrap();
        assert_eq!(vault.read(Path::new("image.bin")).unwrap(), bytes);
        assert!(matches!(
            vault.create(Path::new("image.bin"), b"replace"),
            Err(VaultError::AlreadyExists(_))
        ));
        let new_bytes = [255, 0, 12, 128];
        vault
            .save(Path::new("image.bin"), &bytes, &new_bytes)
            .unwrap();
        assert_eq!(vault.read(Path::new("image.bin")).unwrap(), new_bytes);
        assert!(fs::read_dir(directory.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".diamond-save-")
        }));
    }

    #[test]
    fn relative_path_boundary_and_parent_directories() {
        let directory = tempfile::tempdir().unwrap();
        let vault = Vault::open(directory.path()).unwrap();
        for path in ["", "..", "../escape", "/tmp/escape", "folder/../escape"] {
            assert!(matches!(
                vault.create(Path::new(path), b"bad"),
                Err(VaultError::InvalidPath(_))
            ));
            assert!(vault.read(Path::new(path)).is_err());
        }
        vault.create_dir(Path::new("folder")).unwrap();
        vault.create(Path::new("folder/note.md"), b"safe").unwrap();
        assert_eq!(vault.read(Path::new("folder/note.md")).unwrap(), b"safe");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_rejected_and_skipped_by_index() {
        use std::os::unix::fs::symlink;
        let directory = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.md"), b"outside").unwrap();
        symlink(outside.path(), directory.path().join("escape")).unwrap();
        symlink(
            outside.path().join("secret.md"),
            directory.path().join("linked.md"),
        )
        .unwrap();
        let vault = Vault::open(directory.path()).unwrap();
        assert!(matches!(
            vault.read(Path::new("escape/secret.md")),
            Err(VaultError::Symlink(_))
        ));
        assert!(matches!(
            vault.create(Path::new("escape/new.md"), b"bad"),
            Err(VaultError::Symlink(_))
        ));
        assert!(matches!(
            vault.save(Path::new("linked.md"), b"outside", b"bad"),
            Err(VaultError::Symlink(_))
        ));
        assert!(vault.entries().unwrap().is_empty());
        assert!(vault.build_index().unwrap().notes.is_empty());
        assert_eq!(
            fs::read(outside.path().join("secret.md")).unwrap(),
            b"outside"
        );
    }

    #[test]
    fn index_ignores_internal_files_and_invalid_text() {
        let directory = tempfile::tempdir().unwrap();
        let vault = Vault::open(directory.path()).unwrap();
        vault.create_dir(Path::new(".git")).unwrap();
        vault.create_dir(Path::new(".diamond")).unwrap();
        vault
            .create(Path::new(".git/private.md"), b"secret")
            .unwrap();
        vault
            .create(Path::new(".diamond/cache.md"), b"cache")
            .unwrap();
        vault.create(Path::new("good.md"), b"# Good").unwrap();
        vault.create(Path::new("binary.md"), &[0xff, 0xfe]).unwrap();
        let index = vault.build_index().unwrap();
        assert_eq!(index.notes.len(), 1);
        assert_eq!(index.notes[0].title, "Good");
        assert_eq!(index.skipped, [PathBuf::from("binary.md")]);
        assert_eq!(vault.entries().unwrap().len(), 2);
    }

    #[test]
    fn oversized_files_are_rejected_before_reading_and_skipped_by_index() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("huge.md");
        File::create(&path)
            .unwrap()
            .set_len(MAX_FILE_BYTES + 1)
            .unwrap();
        let vault = Vault::open(directory.path()).unwrap();
        assert!(matches!(
            vault.read(Path::new("huge.md")),
            Err(VaultError::TooLarge {
                limit: MAX_FILE_BYTES,
                ..
            })
        ));
        let index = vault.build_index().unwrap();
        assert!(index.notes.is_empty());
        assert_eq!(index.skipped, [PathBuf::from("huge.md")]);
    }

    #[test]
    fn saving_a_deleted_file_does_not_recreate_it() {
        let directory = tempfile::tempdir().unwrap();
        let vault = Vault::open(directory.path()).unwrap();
        vault.create(Path::new("note.md"), b"original").unwrap();
        fs::remove_file(directory.path().join("note.md")).unwrap();
        assert!(
            vault
                .save(Path::new("note.md"), b"original", b"mine")
                .is_err()
        );
        assert!(!directory.path().join("note.md").exists());
    }

    #[cfg(unix)]
    #[test]
    fn atomic_save_preserves_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let vault = Vault::open(directory.path()).unwrap();
        vault.create(Path::new("note.md"), b"before").unwrap();
        fs::set_permissions(
            directory.path().join("note.md"),
            fs::Permissions::from_mode(0o640),
        )
        .unwrap();
        vault
            .save(Path::new("note.md"), b"before", b"after")
            .unwrap();
        let permissions = fs::metadata(directory.path().join("note.md"))
            .unwrap()
            .permissions();
        assert_eq!(permissions.mode() & 0o777, 0o640);
    }
}

#[cfg(test)]
mod move_tests {
    use super::*;
    #[test]
    fn directory_moves_preserve_children_and_never_replace_or_recurse() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("from")).unwrap();
        fs::create_dir(root.path().join("to")).unwrap();
        fs::write(root.path().join("from/я.md"), b"unchanged").unwrap();
        let vault = Vault::open(root.path()).unwrap();
        assert!(
            vault
                .move_entry(Path::new("from"), Path::new("from/inside"))
                .is_err()
        );
        assert!(
            vault
                .move_entry(Path::new("from"), Path::new("to"))
                .is_err()
        );
        vault
            .move_entry(Path::new("from"), Path::new("to/nested"))
            .unwrap();
        assert_eq!(
            vault.read(Path::new("to/nested/я.md")).unwrap(),
            b"unchanged"
        );
        assert!(!root.path().join("from").exists());
        assert!(
            vault
                .move_entry(Path::new("to/nested/я.md"), Path::new("../escape.md"))
                .is_err()
        );
    }
}

#[cfg(test)]
mod index_cache_tests {
    use super::*;
    #[test]
    fn unchanged_note_sources_are_shared_and_atomic_replacements_are_reindexed() {
        let directory = tempfile::tempdir().unwrap();
        let vault = Vault::open(directory.path()).unwrap();
        vault.create(Path::new("a.md"), b"# old\n").unwrap();
        vault.create(Path::new("b.md"), b"# keep\n").unwrap();
        let first = vault.build_index().unwrap();
        let second = vault.build_index_reusing(Some(&first)).unwrap();
        assert!(std::sync::Arc::ptr_eq(
            &first.notes[0].text,
            &second.notes[0].text
        ));
        assert!(std::sync::Arc::ptr_eq(
            &first.notes[1].text,
            &second.notes[1].text
        ));
        vault
            .save(Path::new("a.md"), b"# old\n", b"# new\n")
            .unwrap();
        let changed = vault.build_index_reusing(Some(&second)).unwrap();
        assert_eq!(&*changed.notes[0].text, "# new\n");
        assert_eq!(changed.notes[0].title, "new");
        assert!(std::sync::Arc::ptr_eq(
            &second.notes[1].text,
            &changed.notes[1].text
        ));
        vault.remove(Path::new("b.md"), false).unwrap();
        assert_eq!(
            vault
                .build_index_reusing(Some(&changed))
                .unwrap()
                .notes
                .len(),
            1
        );
    }
}
