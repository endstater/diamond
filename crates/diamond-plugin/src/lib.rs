//! Language-independent, explicitly launched Diamond plugins.
//!
//! The process has the user's OS permissions. Capabilities constrain only the
//! information passed through this protocol; they do not provide a sandbox.
//! Startup and invocation are blocking and belong on an application worker.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use thiserror::Error;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
pub const MAX_SELECTION_BYTES: usize = 256 * 1024;
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_COMMANDS: usize = 128;
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("plugin I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid plugin JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid plugin manifest: {0}")]
    Manifest(String),
    #[error("plugin protocol error: {0}")]
    Protocol(String),
    #[error("plugin did not respond before the timeout")]
    Timeout,
    #[error("plugin is not running")]
    NotRunning,
    #[error("plugin command is not registered: {0}")]
    UnknownCommand(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    SelectionRead,
    SelectionReplace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub protocol_version: u32,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    #[serde(skip)]
    pub manifest_dir: Option<PathBuf>,
}

impl PluginManifest {
    /// Reads data only. It never launches a program.
    pub fn load(path: &Path) -> Result<Self, PluginError> {
        use std::io::Read;

        let file = fs::File::open(path)?;
        let mut bytes = Vec::new();
        file.take(MAX_MANIFEST_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(PluginError::Manifest("manifest exceeds 64 KiB".into()));
        }
        let mut manifest: Self = serde_json::from_slice(&bytes)?;
        manifest.manifest_dir = Some(
            path.canonicalize()?
                .parent()
                .ok_or_else(|| PluginError::Manifest("manifest has no parent".into()))?
                .to_path_buf(),
        );
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), PluginError> {
        if !valid_id(&self.id) {
            return Err(PluginError::Manifest("id must contain 1–96 ASCII letters, digits, '.', '_' or '-'".into()));
        }
        if !valid_title(&self.name) {
            return Err(PluginError::Manifest("name must contain 1–128 non-control characters".into()));
        }
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(PluginError::Manifest(format!(
                "unsupported protocol version {}", self.protocol_version
            )));
        }
        if self.program.is_empty() || self.program.len() > 4096 || self.program.contains('\0') {
            return Err(PluginError::Manifest("invalid executable name".into()));
        }
        if self.args.len() > 64
            || self.args.iter().any(|arg| arg.len() > 4096 || arg.contains('\0'))
        {
            return Err(PluginError::Manifest("invalid or excessive arguments".into()));
        }
        let mut seen = HashSet::new();
        if self.capabilities.iter().any(|cap| !seen.insert(*cap)) {
            return Err(PluginError::Manifest("duplicate capability".into()));
        }
        Ok(())
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}

fn valid_title(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= 128
        && !value.chars().any(char::is_control)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginCommand {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginOutput {
    #[serde(default)]
    pub replacement: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Initialized {
    protocol_version: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Commands {
    commands: Vec<PluginCommand>,
}

type WireReply = Result<Value, String>;

/// A process connection with bounded I/O and a fixed response deadline.
///
/// Creating this value executes the manifest's program. A UI must first show
/// the program, arguments and capabilities and receive an explicit launch action.
pub struct PluginHost {
    manifest: PluginManifest,
    child: Option<Child>,
    input: Option<SyncSender<Vec<u8>>>,
    replies: Receiver<WireReply>,
    overflowed: Arc<AtomicBool>,
    commands: Vec<PluginCommand>,
    next_id: u64,
    timeout: Duration,
}

impl PluginHost {
    /// Launch only after an explicit user action; run this on a worker thread.
    pub fn start(manifest: PluginManifest) -> Result<Self, PluginError> {
        Self::start_with_timeout(manifest, REQUEST_TIMEOUT)
    }

    fn start_with_timeout(manifest: PluginManifest, timeout: Duration) -> Result<Self, PluginError> {
        manifest.validate()?;
        let mut command = Command::new(&manifest.program);
        command
            .args(&manifest.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(directory) = &manifest.manifest_dir {
            command.current_dir(directory);
        }
        let mut child = command.spawn()?;
        let Some(stdin) = child.stdin.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(PluginError::Protocol("missing process stdin".into()));
        };
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(PluginError::Protocol("missing process stdout".into()));
        };
        let (input_tx, input_rx) = mpsc::sync_channel::<Vec<u8>>(1);
        let (reply_tx, reply_rx) = mpsc::sync_channel::<WireReply>(8);
        let overflowed = Arc::new(AtomicBool::new(false));
        let writer_reply_tx = reply_tx.clone();
        let writer_overflowed = Arc::clone(&overflowed);
        // Never write to a plugin's pipe on the caller thread: a plugin that
        // stops reading must not bypass the response timeout.
        thread::spawn(move || {
            let mut stdin = stdin;
            while let Ok(bytes) = input_rx.recv() {
                if let Err(error) = stdin.write_all(&bytes).and_then(|()| stdin.flush()) {
                    send_reply(&writer_reply_tx, Err(error.to_string()), &writer_overflowed);
                    break;
                }
            }
        });
        let reader_overflowed = Arc::clone(&overflowed);
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let reply = read_line(&mut reader)
                    .and_then(|line| serde_json::from_slice(&line).map_err(|e| e.to_string()));
                let ended = reply.is_err();
                if !send_reply(&reply_tx, reply, &reader_overflowed) || ended {
                    break;
                }
            }
        });
        let mut host = Self {
            manifest,
            child: Some(child),
            input: Some(input_tx),
            replies: reply_rx,
            overflowed,
            commands: Vec::new(),
            next_id: 1,
            timeout,
        };
        let initialized: Initialized = serde_json::from_value(host.request(
            "initialize",
            json!({"protocol_version":PROTOCOL_VERSION,"capabilities":host.manifest.capabilities}),
        )?)?;
        if initialized.protocol_version != PROTOCOL_VERSION {
            return Err(PluginError::Protocol("plugin selected an unsupported version".into()));
        }
        let commands: Commands = serde_json::from_value(host.request("commands/list", json!({}))?)?;
        let mut seen = HashSet::new();
        if commands.commands.len() > MAX_COMMANDS
            || commands.commands.iter().any(|item| {
                !valid_id(&item.id) || !valid_title(&item.title) || !seen.insert(item.id.clone())
            })
        {
            return Err(PluginError::Protocol("invalid, duplicate or excessive commands".into()));
        }
        host.commands = commands.commands;
        Ok(host)
    }

    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    pub fn commands(&self) -> &[PluginCommand] {
        &self.commands
    }

    pub fn is_running(&mut self) -> bool {
        self.child
            .as_mut()
            .and_then(|child| child.try_wait().ok())
            .is_some_and(|status| status.is_none())
    }

    /// Passes only this explicitly supplied selection, if its capability is granted.
    /// Returns a proposed edit; the host never applies it to a file or editor.
    pub fn invoke(
        &mut self,
        command: &str,
        selection: Option<&str>,
    ) -> Result<PluginOutput, PluginError> {
        if !self.commands.iter().any(|item| item.id == command) {
            return Err(PluginError::UnknownCommand(command.into()));
        }
        let mut params = json!({"command": command});
        if self.manifest.capabilities.contains(&Capability::SelectionRead)
            && let Some(selection) = selection
        {
            if selection.len() > MAX_SELECTION_BYTES {
                return Err(PluginError::Protocol("selection exceeds 256 KiB".into()));
            }
            params["selection"] = Value::String(selection.into());
        }
        let output = self
            .request("commands/invoke", params)
            .and_then(|value| serde_json::from_value::<PluginOutput>(value).map_err(PluginError::from));
        match output {
            Ok(output)
                if output.replacement.is_none()
                    || self.manifest.capabilities.contains(&Capability::SelectionReplace) =>
            {
                Ok(output)
            }
            Ok(_) => {
                let _ = self.stop();
                Err(PluginError::Protocol("plugin returned a replacement without permission".into()))
            }
            Err(error) => {
                let _ = self.stop();
                Err(error)
            }
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, PluginError> {
        let outcome = self.request_inner(method, params);
        if outcome.is_err() {
            let _ = self.stop();
        }
        outcome
    }

    fn request_inner(&mut self, method: &str, params: Value) -> Result<Value, PluginError> {
        let input = self.input.as_ref().ok_or(PluginError::NotRunning)?;
        if self.overflowed.load(Ordering::Acquire) {
            return Err(PluginError::Protocol("plugin emitted excessive output".into()));
        }
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1)
            .ok_or_else(|| PluginError::Protocol("request id exhausted".into()))?;
        let mut bytes = serde_json::to_vec(&json!({
            "jsonrpc":"2.0", "id":id, "method":method, "params":params
        }))?;
        bytes.push(b'\n');
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(PluginError::Protocol("request exceeds 1 MiB".into()));
        }
        input.try_send(bytes).map_err(|error| match error {
            TrySendError::Full(_) => PluginError::Protocol("plugin is not reading input".into()),
            TrySendError::Disconnected(_) => PluginError::NotRunning,
        })?;
        let response = self.replies.recv_timeout(self.timeout).map_err(|error| match error {
            mpsc::RecvTimeoutError::Timeout => PluginError::Timeout,
            mpsc::RecvTimeoutError::Disconnected => PluginError::NotRunning,
        })?.map_err(PluginError::Protocol)?;
        if self.overflowed.load(Ordering::Acquire) {
            return Err(PluginError::Protocol("plugin emitted excessive output".into()));
        }
        let object = response.as_object()
            .ok_or_else(|| PluginError::Protocol("response must be an object".into()))?;
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
            || object.get("id").and_then(Value::as_u64) != Some(id)
        {
            return Err(PluginError::Protocol("unexpected response version or id".into()));
        }
        match (object.get("result"), object.get("error")) {
            (Some(result), None) => Ok(result.clone()),
            (None, Some(error)) => {
                let message = error.get("message").and_then(Value::as_str).unwrap_or("plugin error");
                Err(PluginError::Protocol(message.chars().take(1024).collect()))
            }
            _ => Err(PluginError::Protocol("response must contain exactly one result or error".into())),
        }
    }

    pub fn stop(&mut self) -> Result<(), PluginError> {
        self.input.take();
        if let Some(child) = self.child.as_mut() {
            if !matches!(child.try_wait(), Ok(Some(_))) {
                // Keep the handle if cleanup fails so Drop can make another
                // attempt. A process can also exit between try_wait and kill.
                if let Err(error) = child.kill()
                    && !matches!(child.try_wait(), Ok(Some(_)))
                {
                    return Err(PluginError::Io(error));
                }
            }
            child.wait()?;
        }
        self.child.take();
        Ok(())
    }
}

impl Drop for PluginHost {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn send_reply(sender: &SyncSender<WireReply>, reply: WireReply, overflowed: &AtomicBool) -> bool {
    match sender.try_send(reply) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => {
            overflowed.store(true, Ordering::Release);
            false
        }
        Err(TrySendError::Disconnected(_)) => false,
    }
}

/// Read a bounded line without allowing `read_until` to allocate without limit.
fn read_line(reader: &mut impl BufRead) -> Result<Vec<u8>, String> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf().map_err(|error| error.to_string())?;
        if available.is_empty() {
            return Err("plugin closed stdout before a complete response".into());
        }
        let end = available.iter().position(|byte| *byte == b'\n');
        let count = end.map_or(available.len(), |position| position + 1);
        if line.len() + count > MAX_MESSAGE_BYTES {
            return Err("response exceeds 1 MiB".into());
        }
        line.extend_from_slice(&available[..count]);
        reader.consume(count);
        if end.is_some() {
            return Ok(line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::time::Instant;

    fn manifest() -> PluginManifest {
        PluginManifest {
            id: "diamond.test".into(),
            name: "Test plugin".into(),
            protocol_version: 1,
            program: "python3".into(),
            args: Vec::new(),
            capabilities: vec![Capability::SelectionRead, Capability::SelectionReplace],
            manifest_dir: None,
        }
    }

    fn python_manifest(body: &str) -> PluginManifest {
        let mut manifest = manifest();
        manifest.args = vec!["-u".into(), "-c".into(), body.into()];
        manifest
    }

    const PYTHON_PREFIX: &str = r#"import json, sys
for line in sys.stdin:
    q = json.loads(line)
    if q['method'] == 'initialize':
        result = {'protocol_version': 1}
    elif q['method'] == 'commands/list':
        result = {'commands': [{'id': 'upper', 'title': 'Uppercase'}]}
    else:
        result = {'replacement': q['params'].get('selection', '').upper()}
    print(json.dumps({'jsonrpc': '2.0', 'id': q['id'], 'result': result}), flush=True)
"#;

    #[test]
    fn rejects_bad_versions_duplicate_capabilities_and_unknown_fields() {
        let mut item = manifest();
        item.protocol_version = 2;
        assert!(item.validate().is_err());
        item.protocol_version = 1;
        item.capabilities.push(Capability::SelectionRead);
        assert!(item.validate().is_err());
        let mut data = serde_json::to_value(manifest()).unwrap();
        data["shell"] = json!(true);
        assert!(serde_json::from_value::<PluginManifest>(data).is_err());
        assert!(serde_json::from_str::<Capability>("\"filesystem_write\"").is_err());
    }

    #[test]
    fn loading_manifest_does_not_execute_program() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("manifest.json");
        let mut item = manifest();
        item.program = "this-program-does-not-exist-diamond-test".into();
        fs::write(&path, serde_json::to_vec(&item).unwrap()).unwrap();
        let loaded = PluginManifest::load(&path).unwrap();
        assert_eq!(loaded.manifest_dir, Some(directory.path().canonicalize().unwrap()));
        assert!(PluginHost::start(loaded).is_err());
    }

    #[test]
    fn bounds_lines_even_without_a_newline() {
        let bytes = vec![b'x'; MAX_MESSAGE_BYTES + 1];
        assert!(read_line(&mut Cursor::new(bytes)).unwrap_err().contains("exceeds"));
        assert!(read_line(&mut Cursor::new(b"{}")).is_err());
        assert_eq!(read_line(&mut Cursor::new(b"{}\n")).unwrap(), b"{}\n");
    }

    #[test]
    fn external_plugin_transforms_unicode_and_unknown_commands_are_local_errors() {
        let mut host = PluginHost::start(python_manifest(PYTHON_PREFIX)).unwrap();
        assert_eq!(host.commands()[0].id, "upper");
        assert!(matches!(host.invoke("absent", None), Err(PluginError::UnknownCommand(_))));
        assert_eq!(host.invoke("upper", Some("Привет 🦀")).unwrap().replacement.as_deref(), Some("ПРИВЕТ 🦀"));
        host.stop().unwrap();
        assert!(!host.is_running());
        host.stop().unwrap();
    }

    #[test]
    fn does_not_share_selection_without_read_capability() {
        let mut item = python_manifest(PYTHON_PREFIX);
        item.capabilities = vec![Capability::SelectionReplace];
        let mut host = PluginHost::start(item).unwrap();
        assert_eq!(host.invoke("upper", Some("secret")).unwrap().replacement.as_deref(), Some(""));
    }

    #[test]
    fn rejects_unpermitted_edits_and_stops_plugin() {
        let mut item = python_manifest(PYTHON_PREFIX);
        item.capabilities = vec![Capability::SelectionRead];
        let mut host = PluginHost::start(item).unwrap();
        assert!(host.invoke("upper", Some("private")).is_err());
        assert!(!host.is_running());
    }

    #[test]
    fn hung_plugin_times_out_and_is_reaped() {
        let started = Instant::now();
        let result = PluginHost::start_with_timeout(
            python_manifest("import time; time.sleep(10)"),
            Duration::from_millis(150),
        );
        assert!(matches!(result, Err(PluginError::Timeout)));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn unexpected_response_id_aborts_startup() {
        let item = python_manifest("import json,sys; q=json.loads(sys.stdin.readline()); print(json.dumps({'jsonrpc':'2.0','id':999,'result':{'protocol_version':1}}),flush=True)");
        assert!(matches!(PluginHost::start(item), Err(PluginError::Protocol(_))));
    }

    #[test]
    fn blocked_stdin_cannot_bypass_the_request_timeout() {
        let body = r#"import json,sys,time
for result in [{'protocol_version':1},{'commands':[{'id':'upper','title':'Uppercase'}]}]:
    q=json.loads(sys.stdin.readline())
    print(json.dumps({'jsonrpc':'2.0','id':q['id'],'result':result}),flush=True)
time.sleep(10)
"#;
        let mut host = PluginHost::start_with_timeout(
            python_manifest(body),
            Duration::from_millis(150),
        ).unwrap();
        let selection = "a".repeat(MAX_SELECTION_BYTES);
        let started = Instant::now();
        assert!(matches!(host.invoke("upper", Some(&selection)), Err(PluginError::Timeout)));
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(!host.is_running());
    }
}
