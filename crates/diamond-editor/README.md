# diamond-editor

Native egui editing adapters. `EditorBuffer::for_path` enables live Markdown only
for `.md` (case-insensitive); other UTF-8 extensions use source mode and cannot
switch to formatted mode. Binary bytes start in hexadecimal mode.

Live Markdown uses one continuous `TextEdit`. The active caret block reveals
real source syntax; supported inactive blocks show semantic text and marks.
Explicit scalar source maps anchor cursor and selection through reveal/collapse.
Typing and removing heading/emphasis delimiters uses source transactions that
reparse into the authoritative document and share its undo/redo history. There
are no formatting buttons, invisible marker characters or separate preview.
Unsupported/ambiguously mapped blocks keep their source visible for safe edits.

`EditorBuffer::from_bytes` never performs lossy UTF-8 conversion. Non-UTF-8 bytes start in hexadecimal mode. `bytes()` rejects invalid hex, so an invalid edit cannot be silently committed. Switching modes first validates the current buffer and keeps it intact on error. Switching from edited source back to formatted mode parses the new source and starts a new semantic undo history.

The source editor includes native selection, clipboard/IME, an elementary lexical highlighter, and cached case-aware and regex find/replace with highlighting, navigation and Escape to close. Search results use Unicode scalar ranges. It is not an LSP client, a full language grammar, or a large-file virtualized editor. Formatted undo/redo covers the document model, including formatting and block changes. Source/hex typing undo is provided by egui.

Public API: `EditorBuffer::{from_bytes, bytes, mode, set_mode, is_binary, document, source_text, find_all, replace_all, ui, ui_with_options}`; `EditorResponse` reports changes and errors to the desktop host. The host owns file paths, save conflict detection, and original-byte tracking.

Hosts should construct file buffers with `for_path`, observe
`EditorResponse::composing`, and call `cancel_composition` before deactivating a
tab. Cancelling restores the preedit snapshot rather than accepting an IME
candidate. The session layer owns autosave, debounce and conflict handling.

The host allocates a full-height UI rectangle; scroll areas do not shrink to short content. `set_view_options` selects font, wrapping and source line numbers. `revision_key` invalidates host dirty caches. Search uses source scalar positions and supports up to 10,000 highlighted matches; document edits refresh results without moving the caret to the first match. SVG helpers rasterize local bundled Lucide icons once into context texture caches. A live edit checkpoint is created on the first mutation, not on idle frames.

0.4 adds circular clickable task controls backed by `Document::toggle_task`, native horizontal-rule decorations, semantic link underlining and Ctrl+click navigation reported through `EditorResponse::open_link`. Raw blocks no longer receive code styling. The host supplies `set_language` (English by default); static translations cover editor controls. Revealed-block scalar lookup uses binary search rather than scanning the whole paragraph for each character.

0.5 treats empty trailing lines as separate caret locations rather than falling back to the previous block. Window focus and TextEdit focus use the same policy, removing a continuous-repaint path when the window is unfocused. Revealed list items do not also paint a duplicate bullet/task marker; ordered decorations use the original item number.

GFM tables render column padding/alignment and grid geometry in the continuous semantic projection. Standalone image objects reserve native layout space and paint decoded textures with the alt label; source appears when edited. The host supplies a byte loader; local resources are requested when visible, HTTP(S) only after an explicit load click. Workers decode PNG/JPEG/GIF/WebP with byte/dimension limits and notify through request_repaint on completion. No polling timer is added. Source and fenced code share bounded cached Syntect grammar highlighting; TypeScript falls back to JavaScript.

0.6 adds Enter continuation in Live and .md Source, indentation/quote geometry, numbered footnote objects with Ctrl+click navigation, and cached native mathematical-LaTeX. The math renderer pins latex-rust 2.1.1 to preserve Rust 1.88 compatibility, uses only its embedded STIX font, disables its optional egui backend, and rasterizes path-only SVG through the existing resvg. Formula objects reserve actual native glyph width/line height; source appears on editing. Unsupported math shows an error object without changing the source. Cache: 24 entries, input at most 8192 bytes / 64 brace groups, bounded raster size.

Code and footnote headings occupy explicit structural projection rows. Code frames start at their header, below a native gap, so they cannot extend into preceding paragraphs. Task hit rectangles remain registered even if the native editor changes reveal state between pointer press and release; controls request focus, use a pointing-hand cursor, and paint a larger solid checked circle with a contrasting tick. No repaint timer is introduced.

0.7 adds an Image default mode and deferred HEX encoding; bounded decode errors
draw a hatched native placeholder. Table cell menus perform document-owned source
transactions; collapsed tables have independent horizontal scrolling and a source
entry point on double-click. Formula reservations use measured glyph advances;
array shorthand is normalized for rendering only. Syntect 5.3 uses Two-face's
extended grammar set with the same fancy regex backend. Native color emoji use
Swash and an optional system color font; source bytes/positions stay unchanged.
Source/caret viewport offsets and bounded PNG decode are exported for the plugin
host, without embedding compiler or filesystem policy into this crate.

0.7.1 fixes view registration state ownership (`PluginHost.views`, not the
commands-list DTO) and keeps commands replies compatible with v1. SVG previews
accept UTF-8 BOM/comments/XML declarations and scale large vector viewports to
a bounded 1200-pixel raster; raster input limits and disabled external SVG image
resolvers remain unchanged. First-party licensing is GPL-3.0-only.
