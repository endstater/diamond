# Terminal-independent launch

```sh
diamond .
diamond ~/vault_1
diamond ~/notes/idea.md
```

GUI launches start a re-executed child in its own POSIX session. The parent waits
for a one-byte startup acknowledgement after setsid, then exits. Closing the
terminal after its prompt returns does not send the GUI the terminal's hangup.
No shell command, nohup helper, polling thread or process supervisor is used.
The child inherits the working directory, original OS path arguments and display
environment; relative paths, spaces, Unicode and `--` literal paths are retained.

Detached stdout/stderr append to:

```
$XDG_STATE_HOME/diamond/diamond.log
# default: ~/.local/state/diamond/diamond.log
```

Use foreground mode for direct diagnostics, debugger sessions or scripts that
need to wait for the GUI:

```sh
diamond --foreground .
cargo run --release -p diamond-app -- --foreground .
DIAMOND_PROFILE=1 diamond --foreground . 2>diamond-profile.log
```

DIAMOND_PROFILE=1 also implicitly keeps the foreground behavior for compatibility
with previous profiling commands. --help, --version and --syntax-licenses print
without starting/detaching the GUI. There is no change to vault indexing rules:
a folder adds/opens a vault, a file opens the existing light editor.

The startup acknowledgement reports session creation, not successful window/GPU
initialization. If the GUI later fails to start, inspect the log or use foreground
mode. Native session/terminal survival tests are still pending without a built
binary and desktop environment here.
