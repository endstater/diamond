# Diamond architecture

Decision date: 2026-11-19. Platform: Arch Linux (Wayland and X11).

This architecture was written before implementation. The initial implementation is an alpha foundation. `STATUS.md` distinguishes working features from the remaining Obsidian/VS Code scope.

## Product and dependency decisions

The application is native Rust, using pinned egui/eframe 0.33.0 with the OpenGL renderer, native windowing, clipboard and IME. Browser engines are absent from the default workspace. Markdown remains ordinary `.md` files in an ordinary folder. A cache is never the source of truth.

The default UI has a narrow activity rail, resizable vault tree, document tabs,
a centered editor, an optional inspector and a readable status bar. Surfaces use black/gray shades with configurable solid pastel accents and contrasting selected text. Controls share an 8/12/16
spacing grid and larger rounded corners. Icons use native vector geometry rather
than font glyphs; the diamond and window icon use the project owner’s supplied SVG. Animations
are short and time-based; reduced motion disables interpolation.

## Dependency direction

```mermaid
flowchart TD
  App["diamond-app · desktop"] --> Editor["diamond-editor · egui adapters"]
  App --> Vault["diamond-vault · files and index"]
  Editor --> Document["diamond-document · semantic editing"]
  Vault --> Document
  App --> Core["diamond-core · settings"]
```

The optional Excalidraw helper lives outside this workspace and runs as a child process. This avoids mixing GTK and winit event loops and avoids linking WebKitGTK into the normal app.

## Native Markdown editing

The document owns a sequence of semantic blocks and inline marks. A caret-aware
projection supplies visible text to egui's native editing machinery. Inactive
supported blocks show semantic text; active blocks reveal their real Markdown
syntax, allowing users to type or erase heading/emphasis markers directly.
There are no formatting buttons. Source-map boundaries translate projection
positions to serialized-source scalar positions, and source transactions reparse
into the authoritative semantic model while retaining undo/redo. Reveal/collapse
remaps the native cursor and selection instead of inserting invisible characters.
Block kinds determine heading/list/quote/code layout and marks determine runs.

The egui `TextBuffer` adapter translates character-index edits into document transactions. One editing surface provides continuous selection, native clipboard and IME. A styled layout is a view of the model, not a second editable copy of Markdown. History belongs to the model, so formatting, task changes and block changes undo together.

The first engine handles a deliberately bounded subset. A real Markdown parser identifies supported structures; constructs the engine cannot faithfully represent remain raw blocks. Untouched blocks retain original bytes, whitespace and newline style. Editing a supported block may canonicalize that block only. Complex tables, nested lists, embeds and extended Markdown need explicit schema work and acceptance tests before being called WYSIWYG-supported. A user can always switch the same document to source mode.

Do not implement hidden zero-width delimiters, draw a preview over an editable source buffer, or create an independent TextEdit for every paragraph. Investigate upstream `egui_richedit` after dependency/API validation; do not rely on a cached `latest` documentation URL for an unverified release.

## Raw/code editing

UTF-8 files use native egui TextEdit with syntax highlighting, line numbers, selection, clipboard, search, replacement and undo. This is the first stage of an independent code editor, not a Monaco wrapper. Binary/non-UTF-8 files use a hexadecimal buffer; committing requires valid hex. No lossy text conversion is permitted.

LSP is a later process boundary speaking JSON-RPC with UTF-16 position conversion. Large files need a rope/piece table and viewport-based layout before advertising VS Code performance. Multicaret, completion, diagnostics, folding and debugger integration are separately tracked. Do not add all of Monaco's scope to a widget file.

## Vault and persistence

Vault files are addressed by validated relative paths beneath a canonical root. Traversal and symlink escape must fail. Disk reads and save outcomes are explicit errors. A save compares the loaded bytes with current disk bytes, writes a temporary file in the same directory, fsyncs it and atomically renames it; failures keep the in-memory document dirty. A last-moment external-write race remains a documented limitation until platform locking/version recovery is added.

The initial searchable index records Markdown title/headings, wikilinks, tags, task completion and backlinks. It can be rebuilt from files. Indexing has bounded file sizes and ignores internal metadata, VCS directories and symlinks. Background workers should publish immutable snapshots through channels; the UI must not acquire a vault-wide write lock.

Tab sessions keep original bytes separate from current editor state. Only `.md`
(case-insensitive) can use formatted mode. Those notes autosave after a 600 ms
typing pause; closing/switching/quitting flushes them first. A conflict latches an
error and keeps the dirty buffer instead of silently retrying. Source and hex
modes retain manual saving and Save/Discard/Cancel. IME preedit is excluded from
autosave and uncommitted composition is cancelled on tab deactivation before
flushing accepted text. Settings are persisted separately in XDG config,
including task progress and reduced motion.

## Excalidraw

The optional helper bundles the upstream React component and its fonts/assets locally. WebKitGTK is the Linux engine; it is not Chromium, but is still a substantial system dependency. No claim of a tiny browser engine is made. The helper only opens explicit `.excalidraw` documents. Rust owns file access, byte conflict checking and atomic save. JS communicates scene JSON through a narrow IPC bridge. Navigation is restricted to the bundled app; no arbitrary webpage is opened inside the bridge.

## Extension policy and parity

Diamond has its own plugin system. Plugins may be written in any language and
communicate with the host through a versioned JSON-over-stdio process protocol.
There is no requirement to implement Obsidian's JavaScript API, CSS themes or
another editor's extension format. The initial protocol supports command
discovery, user-approved text input and proposed replacement text. The GUI shows
the executable, arguments and protocol capabilities before an explicit launch.
Execution and deadlines run on a worker; proposed edits require another user
action and are checked against the originating vault, tab and document bytes.
Protocol capabilities are not an OS sandbox: the plugin process runs with the
user's normal operating-system rights. Loading a vault never runs a plugin.

Sync and Publish are excluded from the product requirements. They are not
planned services or missing parity items. The acceptance matrix tracks the
remaining editor scope explicitly.

## References verified during design

- egui TextEdit and custom layout: https://docs.rs/egui/0.33.0/egui/widgets/text_edit/struct.TextEdit.html
- Native code editor example: https://github.com/emilk/egui/blob/main/crates/egui_demo_lib/src/demo/code_editor.rs
- Semantic rich edit candidate: https://docs.rs/egui_richedit/latest/egui_richedit/
- Markdown offsets: https://docs.rs/pulldown-cmark/latest/pulldown_cmark/struct.OffsetIter.html
- WebKitGTK: https://webkitgtk.org/reference/webkitgtk/stable/
- Excalidraw integration: https://docs.excalidraw.com/docs/@excalidraw/excalidraw/integration

## 0.3 desktop services

Appearance preferences choose Dark/Light/System and accent colors; the default remains neutral. Settings is a normal tab and saves after a debounce. Optional fc-list discovery runs once on a worker; selected fonts are read only after user preference selection. XDG FileChooser uses ashpd/async-io on a worker, with typed cancellation and manual-path fallback on failure. No GTK or web runtime is added. Lucide SVGs are vendored with upstream notices and rasterized to cached textures; user diamond2.svg is discovered at startup.

Filesystem updates use native notify handles for the canonical root and allowed directories, excluding ignored trees and symlinks. Events set bounded atomic flags and request repaint; a one-shot settling deadline and post-registration snapshots rebuild the tree/index. There is no periodic filesystem poll or PollWatcher fallback. Only one index request is active, with a pending follow-up bit. Search compilation and results, line numbers and dirty-state comparisons are cached. Edit-history snapshots are lazy on mutation instead of per idle frame.

## 0.4 document and interface additions

Links carry semantic scalar label ranges and a destination; live editing changes source and reparses, while legacy semantic mutations on link-bearing blocks are declined instead of dropping destinations. Horizontal rules have empty semantic text and native geometric decorations. Task toggles replace the single checkbox source character and use the same history. Unsupported raw syntax keeps its source visible with ordinary text styling.

Language is a persisted core enum with static translations; English is the default. UI and editor font families are separate. Fontconfig is consulted on installation of font preferences, with a bounded local font read and Chinese fallback (noto-fonts-cjk recommended), not during each frame. Tree filtering collects matching ancestors once per frame, avoiding a full descendant scan per folder.

## 0.5 editing and resources

Incomplete list lines are represented independently and blank lines retain exact separators, so unrelated items keep their semantic representation. Caret lookup never falls back across empty gaps or EOF newlines; an unclosed code fence is the explicit exception while its content is being typed. Window focus is included in editing/reveal decisions, and the final repaint predicate uses the same policy as the next frame. No focus-loss repaint timer is introduced.

Tables hold scalar cell ranges, cell text and column alignments; table separators are structural anchors in one native TextEdit. Standalone images are semantic objects with native reserved row height and an alt label. Both retain original source and reveal actual Markdown when edited. Code/table backgrounds are placed in an earlier painter slot under semantic content; no preview is painted over editable source.

A host-supplied image byte loader owns safe filesystem and HTTP policies. The document parser never accesses resources. Local images load only when visible, remote targets only after a load click. Decoding is bounded, runs on at most two worker threads across editors, and wakes the UI on completion. The cache stores downsized textures, not full-resolution decoded images. There is no periodic network/filesystem request. Syntect grammars use the fancy-regex backend without libonig; highlighted runs have a bounded cache keyed by text, language and theme.

## 0.6 nested containers, math and references

Containers preserve indentation, quote depth and a prefix boundary for every physical line. Source maps parse only its content suffix and restore byte offsets, while untouched prefixes remain stored verbatim. Continuation returns an explicit scalar-range replacement; one source transaction owns both the marker and newline for undo.

Mathematical LaTeX and footnotes are structural semantic atoms with complete source spans; their rendered glyphs occupy reserved space in the same native TextEdit, not an independent preview over editable Markdown. The math engine cannot run shell commands or fetch fonts. Numbered footnote references resolve to in-document source positions. Code-language headers/gaps also have explicit structural projection/source anchors.

Task controls keep stable IDs and hit regions across reveal changes, take focus for pointer interaction, and retain the existing document-level toggle transaction. The renderer's later frames do not depend on a disappearing decoration to complete a click.

## 0.7 document views and resource boundaries

Protocol v2 preserves v1 and adds typed view descriptors, bounded PNG pages,
diagnostics, source scalar anchors, page-window and navigation events. The app
owns tab source buffers, autosave/conflict checks and egui split UI. A single
plugin worker processes one request at a time, with an edit debounce and revision
checks; binary page decode also runs there. Future PDF/Typst engines live outside
the core. Installing prepared files from a folder or strict GitHub repository URL
is separate from launching; no install/build hooks run. See PLUGINS.md.

Image tabs no longer build HEX eagerly or parse SVG as Markdown. Image byte reads
are capped at 16 MiB, decode dimensions at 4096, and cached thumbnails at 1200.
Invalid/oversized resources draw a bounded hatched square and a background label.
The source/editor model remains the owner of bytes; view switches do not mark the
file edited. HEX is explicit and cannot silently replace unreadable content.

Collapsed tables reserve one mapped structural row and paint native cell controls
inside their own horizontal scroll viewport. Table operations replace just the
source table while preserving inline Markdown, outside bytes, EOL and Undo.
Formula objects use a measured real glyph advance instead of relying on trailing
letter spacing. Native color emoji overlay shaped color glyphs without changing
text bytes or scalar cursor positions. System color fonts are optional.

File/directory moves use Linux renameat2 NOREPLACE via rustix, validated paths and
updated open-tab paths/resource loaders. No link rewriting is performed.
The single-file CLI path creates no sibling enumeration/index/watcher or recent
vault entry. Normal directory launch retains the vault/watch path.

0.7.1 fixes view registration state ownership (`PluginHost.views`, not the
commands-list DTO) and keeps commands replies compatible with v1. SVG previews
accept UTF-8 BOM/comments/XML declarations and scale large vector viewports to
a bounded 1200-pixel raster; raster input limits and disabled external SVG image
resolvers remain unchanged. First-party licensing is GPL-3.0-only.

## 0.7.2 redraw and transaction costs

The model supplies stable revision tokens and shared immutable source maps;
Undo states are shared rather than recursively duplicated by checkpoints.
Validated native editing callbacks are grouped directly in model history instead
of reparsing a computed whole-document diff. The editor keeps one projection and
one galley per current layout key. Width, fonts/DPI, theme, search, reveal, source
revision and image geometry invalidate the appropriate derived data. Painting
is restricted to the viewport. Derived caches have explicit bounds; they do not
change source semantics, byte positions, serialization or save conflict policy.

The vault index worker receives a shared previous snapshot and reuses unchanged
notes after identity/size/mtime/ctime checks; text is shared between snapshots.
Changed notes are read/validated again. No filesystem polling loop is added.


## YAML properties and tag links (0.7.5)

The document model parses/caches YAML metadata with serde_yaml_ng 0.10.0.
Metadata remains a lossless source block. Inactive frontmatter becomes a single
source-anchored property object in the continuous editor projection; active
frontmatter reveals original YAML with syntax highlighting. No separate form,
editable shadow mapping, field commit action or YAML serializer is used to edit
properties. The native source transaction path owns typing, selection, Undo/Redo
and autosave. Invalid/unsupported headers remain editable source.

Rendered rows and wrapped tag geometry are cached per projection/style. The same
cached height reserves space in native layout and paints the object, preventing
wrap discrepancies from overlapping the following paragraph. Clicking the block
requests source focus; chips route semantic tag links to exact/hierarchical vault
search. No YAML content is executed and no new polling clock is introduced.


## Native diagrams and process launch (0.7.6)

Mermaid remains an ordinary semantic Code block with language mermaid. A closed,
inactive fence projects to one mapped diagram atom; active/source mode retains
literal Markdown. One lazily started worker performs native parse/layout/SVG and
bounded rasterization. The context cache stores source/theme results, and only
visible objects submit jobs. No JS engine, plugin execution, remote render service
or periodic polling is added. Rust Mermaid compatibility is bounded by its native
backend; errors never replace the source. See MERMAID.md.

The GUI's entry point parses CLI paths/flags before initializing fonts/windows.
By default it re-executes a child, which calls safe rustix setsid and acknowledges
over a startup Unix socket. The parent then exits; stdout/stderr are log files,
not terminal handles. Foreground diagnostics and info commands remain explicit.
This avoids pre_exec hooks or forking a live GUI/thread pool. See LAUNCH.md.


## 0.7.8 source responsiveness and Live boundary corrections

The previous Source path re-highlighted the entire file synchronously on each
text change. Markdown Enter additionally parsed a full Document for continuation.
These are code-confirmed costs, not a measured attribution of the user's latency.
Source highlighting now runs on one lazy, sleeping worker with line checkpoints,
suffix-state convergence, cancellation/coalescing and revision-checked results.
Source Undo uses bounded changed fragments rather than persistent full-text native
snapshots. IME commit/cancel and explicit replacements share this history. Ordinary
Enter uses a lightweight container helper and dirty checks avoid byte copies.

Live text mutations retain unchanged Mermaid object geometry in the same layout
pass. A single Backspace/Delete at a collapsed table/diagram/property boundary
reveals source and preserves bytes; deliberate whole-object selection can delete
it. Source language coverage is retained; LSP and Live fence highlighting have not
been replaced. See SOURCE_PERFORMANCE.md for limits and diagnostic commands.

Rust regression tests cover Unicode/CRLF undo, redo branching/IME/budgets,
900-line incremental highlighting compared with a fresh parse, multiline state
changes, source continuation, same-pass Mermaid height and table boundary safety.
Cargo/rustc are absent: compilation, Rust tests, clippy/formatting, native input,
CPU and timing measurements have not been run. No speedup percentage is claimed.


## 0.7.11 fonts, header and Arch package identity

UI/Live/Source font selectors include a focused text search inside the dropdown.
Matching choices update on each letter; only selecting a face or pressing Enter
commits a font. Search result indices are cached between query/list changes.
Source exposes only Fontconfig MONO/CHARCELL faces and always uses the Monospace
family, independently of the Live font. Preferences retain collection indices;
missing fields in older settings default to built-in monospace for Source.

Font families retain bundled fallback faces and add cached system fallback faces
for box drawing, symbols, CJK, Arabic and Devanagari. Actual charset coverage is
checked before adding each system face. Fontconfig discovery/file reads happen
on font installation, not typing/layout. System fallbacks are cached once and
bounded to 96 MiB; unsupported WOFF/WOFF2 paths are excluded. This is a finite
family chain, not a guarantee of every Unicode glyph when no loaded face covers
it. The Arch package includes Fontconfig/DejaVu; CJK/color fonts remain optional.

The header now shows a tracked uppercase DIAMOND wordmark, with a dedicated bold
system face when available and no Personal workspace subtitle. The diamond is
back at the left corner. Left-panel toggle is at the top of the activity rail;
both panel toggles use only gray colors in all hover/focus/open/pressed states.
The SVG override tip is removed from settings and translations.

Arch package and installed executable are diamond-notes; Cargo still builds
 target/release/diamond. The launcher uses diamond-notes and the same stable app
ID/icon. Generic diamond binary/icon paths are not installed, and no provides,
replaces or conflicts claim is made against the unrelated diamond package.
If an older package of this editor was installed under diamond, remove that old
editor package before installing the renamed package; see packaging/README.md.
Rust build/tests/native rendering and font-picker focus remain unverified here.
