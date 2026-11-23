# Plugin installation and native document views (protocol v2)

Plugins remain independent executable processes using newline-delimited JSON-RPC
on stdin/stdout. No browser runtime is embedded. The original v1 commands still
work; use `protocol_version: 2` for document views. See the runnable integration
fixture in `examples/plugins/native-preview` and the v1 uppercase example.

## Install without running

In the Plugins panel, paste `https://github.com/OWNER/REPOSITORY` or choose a
folder containing `manifest.json`. The installer copies a prepared plugin to
`$XDG_DATA_HOME/diamond/plugins/ID` (default `~/.local/share/diamond/plugins/ID`).
It never builds packages, runs installation scripts, initializes submodules,
executes hooks or launches the plugin. GitHub installs need `git` and network
access; private repositories requiring interactive authentication are not supported.
The repository root must contain the manifest. Runtime dependencies and executable
artifacts must already be available (e.g. `python3`, or an included native binary).
A native binary should use a relative program such as `./bin/plugin`.

Installed files are bounded to 10,000 entries / 256 MiB / 64 MiB per file / 32 levels;
symlinks and special files are rejected. Git clone itself runs in a worker with a
120-second timeout; the transfer's disk size is not sandbox-limited. Installing an
existing ID is rejected. Remove that installed directory manually to replace it.

Select an installed plugin to review and explicitly launch it. Capabilities limit
what the host passes through this protocol, **not the process's OS permissions**.
Starting a plugin does not automatically give it a vault snapshot. Selecting a
registered view explicitly gives it the active file's absolute path and document ID.

## Register views

The v2 initialize reply echoes version 2. `commands/list` remains required and
may return an empty list. With capability `document_views`, the host calls
`views/list` and expects:

```json
{"views":[{"id":"typst","title":"Typst preview","extensions":["typ"],"editable":true}]}
```

Up to 32 descriptors are allowed. A read-only descriptor occupies the document
area; an editable descriptor uses the native source editor and a right preview.
The user opens a matching file and selects the view in the Plugins panel. Host
source editing, conflict-aware autosave and close/save handling stay in Diamond.
Use `document_read` to receive the explicitly selected document's current UTF-8
source; without it `text` is null. The process can use `file_path` to read a
selected PDF or resolve Typst includes under its ordinary OS permissions.

## Updates and events

The host sends `views/update` with:

```json
{"document":{"view":"typst","document_id":"opaque","revision":4,"file_path":"/home/me/note.typ","text":"Hello"},"event":{"kind":"cursor","offset":2}}
```

Source is limited to 512 KiB. `document_id` distinguishes editor instances/paths;
`revision` changes with document edits. Work runs on a plugin worker, with at most
one request in flight and a 250 ms edit debounce. Navigation events reuse that
revision. Reply within 30 seconds; commands retain their three-second deadline.
Stale replies are discarded; no frame is a source edit. Plugins should retain
compiler state across updates and avoid recompiling on navigation-only events.

Event may be null or one of:

- `cursor`: Unicode scalar `offset` in source;
- `click`: zero-based `page` and normalized `x`, `y` in the rendered page;
- `scroll`: page and normalized `y`;
- `request_pages`: `start`, `count`, and raster `scale` for page-window navigation;
- `viewport`: `width`, `height` in pixels (reserved for host integrations);
- `close`: lifecycle event (reserved; process shutdown currently ends all views).

## Preview result

```json
{"revision":4,"first_page":0,"page_count":1,"pages":[{"id":"p0","png_base64":"..."}],"anchors":[{"offset":0,"page":0,"x":0.1,"y":0.2}],"diagnostics":[],"cursor":null}
```

Pages are native PNG textures: no arbitrary widget code or HTML. Per-frame limits:
16 pages, 768 KiB base64 per page, 8192 source anchors, 256 diagnostics and **1 MiB
for the entire JSON-RPC line**. PNG byte/dimension/allocation limits also apply;
the worker decodes only bounded pages, thumbnailing them to 1200 pixels.
For large PDFs, return a small page window with `first_page` and `page_count`;
request_pages moves that window. Frame page indexes and anchor page indexes are
absolute, zero-based; `page_count` cannot exceed 100,000.

Diagnostics have `message`, `start`, `end` (source scalar offsets); a
 diagnostics-only frame keeps the previous successful preview. Optional `cursor`
requests source navigation. There is no automatic plugin replacement/write API
in v2: source edits remain native. V1 selection replacement still requires review.

The host synchronizes source-cursor positions and preview anchors, and forwards
preview clicks and wheel navigation. Source viewport scrolling is also forwarded
as a nearest visible source offset. This is an anchor-based approximation;
a Typst plugin can use its engine's source mappings for precise click navigation.
PDF anchors may be omitted for a viewer without a source editor.

## Authoring a PDF/Typst plugin

A PDF worker can wrap PDFium or another document library and return visible PNG
pages, adding page-window support for large documents. A Typst worker can retain
a compilation world and use typst-render for pages and typst-ide for source maps.
These engines are not shipped by Diamond 0.7.0. The new protocol, native host
surfaces and example supply the integration points; engines, rich PDF selection,
zoom and incremental page updates remain plugin/future-host work.
