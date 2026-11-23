// SPDX-License-Identifier: GPL-3.0-only
//! Resource policy belongs to the host, not to Markdown parsing or its widgets.
use diamond_vault::Vault;
use std::{
    io::Read,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub fn image_loader(vault: Vault, note: PathBuf) -> diamond_editor::ImageLoader {
    Arc::new(move |target| {
        const MAX: u64 = 16 * 1024 * 1024;
        let bytes = if target.starts_with("https://") || target.starts_with("http://") {
            let agent = ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(12))
                .redirects(4)
                .build();
            let response = agent
                .get(target)
                .call()
                .map_err(|error| error.to_string())?;
            let mut bytes = Vec::new();
            response
                .into_reader()
                .take(MAX + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            bytes
        } else {
            if target.contains(':') || Path::new(target).is_absolute() {
                return Err("Unsupported image address".into());
            }
            let mut relative = PathBuf::new();
            for part in note
                .parent()
                .unwrap_or(Path::new(""))
                .join(target)
                .components()
            {
                match part {
                    Component::Normal(value) => relative.push(value),
                    Component::CurDir => {}
                    Component::ParentDir => {
                        if !relative.pop() {
                            return Err("Image escapes the vault".into());
                        }
                    }
                    _ => return Err("Image escapes the vault".into()),
                }
            }
            vault
                .read_bounded(&relative, MAX)
                .map_err(|error| error.to_string())?
        };
        if bytes.len() as u64 > MAX {
            return Err("Image exceeds 16 MiB".into());
        }
        Ok(bytes)
    })
}
