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

The source editor includes native selection, clipboard/IME, grammar-based highlighting, and cached case-aware and regex find/replace with highlighting, navigation and Escape to close. Search results use Unicode scalar ranges. It is not an LSP client, a full language grammar, or a large-file virtualized editor. Formatted undo/redo covers the document model, including formatting and block changes. Source typing uses bounded insertion/deletion history; Hex typing undo is provided by egui.

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

0.7.2 retains the caret projection and native galley between unchanged frames,
invalidating geometry on revision/reveal/width/font/theme/find/image changes.
Only visible blocks, formulas, emoji and table cells are painted. Table widths
and up to 512 cell galleys are retained per projected table. Math has an LRU
bounded to 128 entries / 64 MiB of raster data; emoji segmentation uses a bounded
snapshot cache. The blank separator before a table no longer receives table
height, and only the table object glyph reserves that height. Quote bars end at
the last content character, excluding trailing mapped line endings. Formula
objects use centered vertical alignment and padded advances. A leading complete
braced array followed by a supported top-level relation is composed as centered
boxes to avoid the engine's first-row array baseline. Ordinary formulas retain
the single-render path; source bytes remain unchanged. Window focus is checked
at both entry and exit before requesting a reveal redraw.


0.7.5 renders YAML properties as a caret-aware block in the same document flow.
The caret inside reveals original YAML; outside it shows cached rows and tag
chips. Clicking the block reveals source, while tags navigate to search. There
is no constructor, form or separate property panel. Rendering never rewrites
YAML comments/formatting. Native source transactions retain editing/history and
source-mode galley caching remains intact.


0.7.6 renders closed mermaid fences as source-anchored diagram objects outside
the caret. A bounded lazy worker uses mermaid-rs-renderer 0.3.1 SVG and resvg text
support, with no browser/Node. Only visible objects request work; source/theme
textures are cached and completion wakes the GUI. Invalid/oversized diagrams
stay clickable for source editing. See docs/MERMAID.md for limits.


## 0.7.7 corrections

Soft and hard paragraph breaks share semantic/source-map newline handling, so
adjacent bold lines with two-space or backslash breaks no longer fall back to
raw text. Source bytes and Unicode/CRLF anchors remain unchanged.
Search-match jumps no longer request editor focus. Direct document/object jumps
still activate editing; typing, Enter and Shift+Enter keep the find field active.
Find/replace uses two aligned grid rows: all find controls stay together and the
Replace all button occupies their combined width. A horizontal scroll fallback
handles unusually narrow editor widths. Saved uses saturated lime, with a darker
variant for light-theme contrast.

Rust regression cases cover break formats/source round trips, inactive projection
and successive find-query letters/Enter in Live, Source and Hex. These cases are
unrun without Rust/Cargo; native focus/geometry and compilation remain unverified.


## 0.7.8 source responsiveness and Live boundaries

Source grammar work runs on one lazy background thread, with per-line tokens and
parse/highlight checkpoints every 32 lines. It resumes before the first changed
line and reuses a common suffix once both states converge. One pending request per
editor coalesces rapid input, running work checks cancellation between lines, and
only a current result affects layout. The worker sleeps on a condition variable;
there is no timer/polling. Until ready, Unicode-safe remapping retains unchanged
colors around the edit. File/language/theme changes invalidate grammar state;
fonts/wrapping/find only invalidate layout. Source has no synchronous grammar call.

At most eight completed source checkpoints are retained, each bounded to 512 KiB /
8192 lines / 16 KiB per line; larger input remains plain editable text. Text layout
itself still uses egui's whole-buffer editor; this is not a virtualized editor.
The existing Live fence highlighter is unchanged in this revision. LSP integration
and per-language server settings are not implemented.

Source Undo/Redo stores insert/delete fragments and scalar selections, bounded to
256 transactions / 8 MiB including Redo. Native callbacks provide edits directly;
there is no whole-document diff on ordinary typing. Find Replace uses one compact
prefix/suffix difference. IME preedit changes form one committed transaction;
cancellation discards pending edits and restores the preedit snapshot. Egui's
transient per-call text copies remain, but native undo snapshots are cleared before
and after the source widget. Hex and Live history semantics remain independent.

Source Enter calls the lightweight container continuation helper instead of
constructing/parsing a semantic document. Dirty checks compare current bytes
without copying Source. DIAMOND_PROFILE=1 logs actual source layout/grammar work;
DIAMOND_SYNTAX=0 provides a Source-only diagnostic baseline without grammar work.

Live rebuilds inside native editing callbacks inherit an unchanged Mermaid
object's measured height, so the same pass does not shrink it to its placeholder.
A single-character deletion at a collapsed table/diagram/property boundary reveals
its source without changing document bytes/history. Explicit whole-object
selection still permits deletion. Regression cases are included but unrun here.


## 0.7.9 search layout and Markdown clipboard

Find and replacement inputs have explicit 180-pixel minimum widths and use the
remaining editor width outside grid column sizing. Two non-wrapping rows retain
aligned inputs and control-group-width Replace all; padding/row gaps increase.
The left-sidebar toggle is beside the logo on the left of the header.
Live Copy uses source-preserving whole blocks and balanced semantic Markdown
fragments for partial inline selections. Full-document Copy preserves original
bytes, including CRLF. Code headers have a native copy-content control on the
right; table exterior corners use a small radius while inner cell joins stay
square. Copy operations do not change source or history.

The repository adds compiled-output ignore patterns and a signature-based cleaner
with preview as default and explicit --delete. Media/fonts, symlinks, Git internals,
installed dependency folders and vaults are preserved. The updater does not run
the cleaner. Rust compilation/native clipboard and geometry remain unverified
without Cargo; portable cleanup tests pass independently of native validation.
