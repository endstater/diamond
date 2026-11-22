# Diamond plugin protocol v1

Plugins are independent programs: Python, Rust, Go, JavaScript or another language can implement the same line-delimited JSON protocol. No browser engine or language runtime is linked into Diamond for plugins.

`PluginManifest::load(path)` only reads and validates JSON. `PluginHost::start(manifest)` **executes a program** and must only follow an explicit user action that shows its command and requested capabilities. Do not discover or launch plugins while opening a vault. The caller must run startup and invocation on a worker thread; both wait for a bounded timeout, but they are blocking APIs.

Capabilities govern information and edits supplied through the host API. They are **not an operating-system sandbox**: the launched process runs with the user's ordinary OS permissions, including access to files and networking. Install and explicitly launch only programs you trust. The protocol supplies only text the user explicitly chooses to share; it does not implicitly supply a vault path or background context. The current application offers an explicit checkbox to share the entire active UTF-8 document and previews a returned replacement before applying it.

## Manifest

```json
{
  "id": "diamond.example.uppercase",
  "name": "Uppercase selection",
  "protocol_version": 1,
  "program": "python3",
  "args": ["plugin.py"],
  "capabilities": ["selection_read", "selection_replace"]
}
```

`program` is launched directly, without a shell. A bare executable is resolved by the OS `PATH`; a relative executable path and relative arguments use the manifest's directory. Loading a manifest resolves that directory; programmatic manifests use the caller's current directory. Commands and arguments are shown as separate fields by the caller rather than rendered as a shell command. Duplicate or unknown capabilities and unsupported versions are rejected.

`selection_read` permits explicitly supplied text (up to 256 KiB) in an invocation. The protocol field is named `selection`; the caller may supply a selected range or an entire document, provided the UI clearly identifies the scope and the user chooses to share it. `selection_replace` permits a returned replacement for that text. Without the read capability, supplied text is omitted. Without the replace capability, returning a replacement is a protocol error. Plugins cannot request further host resources in v1. The caller applies returned edits only to the selection/document version used by the request, after checking that the document has not changed.

## Wire protocol

Write one UTF-8 JSON object followed by `\n`, flush after every response, and use stderr for logs. Every request has `jsonrpc: "2.0"`, a numeric `id`, `method`, and `params`. Respond with the same `jsonrpc` and `id`, and exactly one of `result` or `error`. Notifications, host callbacks and parallel requests are not supported in v1.

1. `initialize`, params `{"protocol_version":1,"capabilities":["selection_read","selection_replace"]}`; result `{"protocol_version":1}`.
2. `commands/list`, params `{}`; result `{"commands":[{"id":"uppercase","title":"Uppercase selection"}]}`. Command identifiers must be unique.
3. `commands/invoke`, params `{"command":"uppercase","selection":"Hello"}`; result `{"replacement":"HELLO","message":"Selection converted"}`. `selection` is absent unless granted and explicitly supplied. Either result field can be absent or null.

An error response uses `{"error":{"code":-32601,"message":"Unknown method"}}` instead of `result`. The maximum line/request/response is 1 MiB, there are at most 128 commands, and each request has a three-second timeout. Invalid JSON, EOF, unexpected ids, excess output and timeout abort the connection and kill/reap the process. Stderr is discarded by this host to avoid an unread pipe. Host drop also kills/reaps the process. Descendant processes are outside this simple host's supervision; plugins must not daemonize.

## Rust API

```rust,ignore
let manifest = PluginManifest::load(path)?; // safe to inspect before launch
// Show program, args and capabilities and obtain the user's explicit launch action.
let mut host = PluginHost::start(manifest)?; // do this on a worker
let commands = host.commands();
let output = host.invoke("uppercase", Some("Hello"))?;
host.stop()?;
```

`PluginOutput` has `replacement: Option<String>` and `message: Option<String>`. Nothing is automatically written to disk or applied to an editor. See `examples/plugins/uppercase` for a runnable implementation using only Python's standard library.
