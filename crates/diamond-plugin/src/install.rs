// SPDX-License-Identifier: GPL-3.0-only
//! Install a plugin's files without executing its code, build scripts or hooks.
use crate::PluginManifest;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
use wait_timeout::ChildExt;
#[derive(Clone, Debug)]
pub struct InstalledPlugin {
    pub name: String,
    pub manifest: PathBuf,
}

pub fn installed(root: &Path) -> Vec<InstalledPlugin> {
    let mut result = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten().take(128) {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                let path = entry.path().join("manifest.json");
                if let Ok(manifest) = PluginManifest::load(&path) {
                    result.push(InstalledPlugin {
                        name: manifest.name,
                        manifest: path,
                    });
                }
            }
        }
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

pub fn install(source: &str, root: &Path) -> Result<InstalledPlugin, String> {
    fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let stage = tempfile::tempdir_in(root).map_err(|error| error.to_string())?;
    let repository = stage.path().join("repository");
    let source_path = if source.starts_with("https://") {
        let url = github_url(source)?;
        let mut child = Command::new("git")
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "protocol.file.allow=never",
                "-c",
                "protocol.ext.allow=never",
                "clone",
                "--depth",
                "1",
                "--no-recurse-submodules",
                "--",
                &url,
            ])
            .arg(&repository)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("Cannot run git: {error}"))?;
        let status = child
            .wait_timeout(Duration::from_secs(120))
            .map_err(|error| error.to_string())?;
        if status.is_none() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Plugin download timed out".into());
        }
        if !status.is_some_and(|status| status.success()) {
            return Err("GitHub download failed; check the link and network access".into());
        }
        repository
    } else {
        PathBuf::from(source)
    };
    let metadata = fs::symlink_metadata(&source_path).map_err(|error| error.to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Select a plugin folder, not a link".into());
    }
    let manifest = PluginManifest::load(&source_path.join("manifest.json"))
        .map_err(|error| error.to_string())?;
    let destination = root.join(&manifest.id);
    if destination.exists() {
        return Err("This plugin is already installed".into());
    }
    let payload = stage.path().join("payload");
    let mut budget = (0usize, 0u64);
    copy_tree(&source_path, &payload, 0, &mut budget)?;
    // Validate the exact staged manifest, not a possibly changed source copy.
    let copied =
        PluginManifest::load(&payload.join("manifest.json")).map_err(|error| error.to_string())?;
    if copied.id != manifest.id {
        return Err("Plugin manifest changed during installation".into());
    }
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        &payload,
        rustix::fs::CWD,
        &destination,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(|error| error.to_string())?;
    Ok(InstalledPlugin {
        name: copied.name,
        manifest: destination.join("manifest.json"),
    })
}
fn github_url(source: &str) -> Result<String, String> {
    let tail = source
        .trim()
        .strip_prefix("https://github.com/")
        .ok_or("Use an https://github.com/owner/repository link")?
        .trim_end_matches('/');
    let parts: Vec<_> = tail.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|part| {
            part.is_empty()
                || matches!(*part, "." | "..")
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        })
    {
        return Err("Use the repository link without a branch, query or credentials".into());
    }
    Ok(format!("https://github.com/{tail}"))
}
fn copy_tree(
    source: &Path,
    target: &Path,
    depth: usize,
    budget: &mut (usize, u64),
) -> Result<(), String> {
    if depth > 32 {
        return Err("Plugin folder nesting exceeds 32 levels".into());
    }
    fs::create_dir(target).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_name() == ".git" {
            continue;
        }
        budget.0 += 1;
        if budget.0 > 10000 {
            return Err("Plugin has too many files".into());
        }
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        let destination = target.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &destination, depth + 1, budget)?;
        } else if kind.is_file() {
            let mut file = fs::File::open(entry.path()).map_err(|error| error.to_string())?;
            let metadata = file.metadata().map_err(|error| error.to_string())?;
            if metadata.len() > 64 * 1024 * 1024 {
                return Err("A plugin file exceeds 64 MiB".into());
            }
            budget.1 += metadata.len();
            if budget.1 > 256 * 1024 * 1024 {
                return Err("Plugin exceeds 256 MiB".into());
            }
            let mut bytes = Vec::new();
            file.by_ref()
                .take(64 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            if bytes.len() as u64 != metadata.len() {
                return Err("Plugin files changed during installation".into());
            }
            fs::write(&destination, bytes).map_err(|error| error.to_string())?;
            fs::set_permissions(&destination, metadata.permissions())
                .map_err(|error| error.to_string())?;
        } else {
            return Err("Plugin folders cannot contain symlinks or special files".into());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repository_links_cannot_be_command_arguments_or_other_hosts() {
        assert!(github_url("https://github.com/person/plugin.git").is_ok());
        for url in [
            "--upload-pack=evil",
            "https://github.com.evil/a/b",
            "https://github.com/a/b/tree/main",
            "https://github.com/a/b?x=y",
            "https://github.com/a/..",
        ] {
            assert!(github_url(url).is_err());
        }
    }
    #[test]
    fn folder_install_never_executes_and_refuses_existing_destination() {
        let source = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::write(source.path().join("manifest.json"),r#"{"id":"test.plugin","name":"Test","protocol_version":2,"program":"does-not-exist","capabilities":["document_views"]}"#).unwrap();
        let result = install(source.path().to_str().unwrap(), root.path()).unwrap();
        assert!(result.manifest.is_file());
        assert!(install(source.path().to_str().unwrap(), root.path()).is_err());
    }
}
