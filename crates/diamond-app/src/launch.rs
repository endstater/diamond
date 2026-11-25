// SPDX-License-Identifier: GPL-3.0-only
//! Detach before GUI/thread initialization. No shell, nohup or runtime polling.
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::{self, Read};
use std::os::fd::OwnedFd;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Info {
    Version,
    Help,
    Licenses,
}
#[derive(Debug)]
pub struct Cli {
    pub path: Option<PathBuf>,
    pub info: Option<Info>,
    foreground: bool,
    detached_child: bool,
    original: Vec<OsString>,
}

pub fn parse(arguments: Vec<OsString>) -> io::Result<Cli> {
    let mut cli = Cli {
        path: None,
        info: None,
        foreground: false,
        detached_child: false,
        original: arguments.clone(),
    };
    let mut literal = false;
    for argument in arguments {
        if !literal {
            match argument.to_str() {
                Some("--") => {
                    literal = true;
                    continue;
                }
                Some("--foreground") => {
                    cli.foreground = true;
                    continue;
                }
                Some("--detached-child") => {
                    cli.detached_child = true;
                    continue;
                }
                Some("--help" | "-h") => {
                    cli.info = Some(Info::Help);
                    continue;
                }
                Some("--version" | "-V") => {
                    cli.info = Some(Info::Version);
                    continue;
                }
                Some("--syntax-licenses") => {
                    cli.info = Some(Info::Licenses);
                    continue;
                }
                Some(option) if option.starts_with('-') => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("Unknown option: {option}"),
                    ));
                }
                _ => {}
            }
        }
        if cli.path.replace(PathBuf::from(argument)).is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Expected at most one folder or file",
            ));
        }
    }
    Ok(cli)
}

fn log_path() -> io::Result<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "HOME or XDG_STATE_HOME is needed for the background log",
            )
        })?;
    let directory = state.join("diamond");
    std::fs::create_dir_all(&directory)?;
    Ok(directory.join("diamond.log"))
}

/// True means the parent should return: the GUI runs in the spawned session.
#[allow(clippy::zombie_processes)] // The parent exits immediately after spawn; it must not wait for the GUI.
pub fn detach(cli: &Cli) -> io::Result<bool> {
    if cli.detached_child {
        // The re-executed child inherits its parent's PGID, so it is not the
        // process-group leader. setsid drops the controlling terminal.
        let _session = rustix::process::setsid().map_err(io::Error::from)?;
        let written = rustix::io::write(std::io::stdin(), b"R").map_err(io::Error::from)?;
        if written != 1 {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "Cannot acknowledge detached session",
            ));
        }
        return Ok(false);
    }
    if cli.foreground
        || std::env::var_os("DIAMOND_PROFILE").as_deref() == Some(std::ffi::OsStr::new("1"))
    {
        return Ok(false);
    }
    let path = log_path()?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&path)?;
    let (mut acknowledgement, child_input) = UnixStream::pair()?;
    acknowledgement.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
    let child_input: OwnedFd = child_input.into();
    let _child = Command::new(std::env::current_exe()?)
        .arg("--detached-child")
        .args(&cli.original)
        .stdin(Stdio::from(child_input))
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .spawn()?;
    // The terminal is released only after the child has its own session.
    // There is no timer thread: one startup socket read, then both ends hit EOF.
    let mut ready = [0u8; 1];
    acknowledgement.read_exact(&mut ready).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("Background startup failed: {error}; see {}", path.display()),
        )
    })?;
    if ready != *b"R" {
        return Err(io::Error::other(
            "Invalid background startup acknowledgement",
        ));
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(parts: &[&str]) -> Vec<OsString> {
        parts.iter().map(|part| OsString::from(*part)).collect()
    }
    #[test]
    fn foreground_and_unicode_path_are_independent_arguments() {
        let cli = parse(args(&["--foreground", "/tmp/Мои заметки"])).unwrap();
        assert!(cli.foreground);
        assert_eq!(cli.path, Some(PathBuf::from("/tmp/Мои заметки")));
        assert_eq!(
            parse(args(&[".", "--foreground"])).unwrap().path,
            Some(PathBuf::from("."))
        );
    }
    #[test]
    fn literal_paths_and_invalid_flags() {
        assert_eq!(
            parse(args(&["--", "--version"])).unwrap().path,
            Some(PathBuf::from("--version"))
        );
        assert_eq!(
            parse(args(&["--version"])).unwrap().info,
            Some(Info::Version)
        );
        assert!(parse(args(&["--unknown"])).is_err());
        assert!(parse(args(&["a", "b"])).is_err());
    }
}
