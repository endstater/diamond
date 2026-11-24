# Current snapshot: 0.7.2

0.7.2 targets redraw/typing/indexing cost, fixes table-gap and quote bounds,
centers the supported braced-array/relation form and adds HEX accent entry.
See PERFORMANCE.md for the source audit and optional timing mode. No native
CPU measurements or Rust compilation were possible in this environment.

0.7.1 corrects PluginHost view storage and the v1/v2 commands response schema,
changes first-party licensing to GPL-3.0-only, replaces the public README with a
short English introduction and improves native SVG recognition/scaling.

Source implementation adds v2 document-view plugins and a native split host,
folder/GitHub installation, bounded native image tabs/lazy HEX, structural table
menus and independent horizontal scrolling, file/directory moves and rename,
single-file launch mode, extra syntax grammars, native color emoji, updated
appearance and a local-checkout Arch package recipe. See README and PLUGINS.md.

This snapshot has not been compiled here. Python v1/v2 examples, source structure,
lexical balance, shell syntax and updater backup/rollback are checked separately;
none are a substitute for Rust type/borrow checking or a desktop acceptance run.
PDF/Typst engines are not included; one plugin process and one native preview are
active at a time. Precise Typst mappings, PDF selection and zoom remain plugin/host
extension work. Native source/preview synchronization uses supplied anchors.

The following records describe earlier snapshots and their supported scope.

# Implementation status

Diamond 0.6.0 is a source alpha for a native Rust desktop application on Arch Linux. Sync and Publish are excluded from the product scope.

The workspace now contains six implemented crates: shared settings, a semantic
Markdown engine, conflict-aware vault storage and search, native editor adapters,
the plugin host, and the desktop application. It includes a sample vault,
a Python plugin, source checks and local Arch packaging.

The desktop code contains file sessions, Markdown/source/hex modes, formatting
and task commands, tabs, dirty-close decisions, a command palette, background
indexing, headings/backlinks, XDG settings and explicit plugin launch/review.
The 0.2 revision adds a neutral grayscale interface, native vector icons, larger
radii and consistent spacing, a larger status bar, caret-revealed Markdown
syntax instead of formatting buttons, and conflict-aware debounced autosave for
formatted `.md` notes. Other extensions cannot enter formatted mode. IME
composition is excluded from persistence and cancelled safely on deactivation.

Plugins stop on a worker before an orderly application exit. Proposed edits
are rejected if the originating vault or document bytes have changed.

The user confirmed that the prior 0.1 snapshot compiled successfully on their
system. This 0.6.0 revision has not been compiled or run in this environment.
This snapshot is not a tested release or a claim of full Obsidian
or VS Code parity. See the acceptance matrix for the supported subset and
the remaining Markdown, Excalidraw, language-server and large-file work.

Plugins may use any language. The host communicates through a documented protocol rather than requiring compatibility with an existing plugin API. Opening a vault does not run its plugins or scripts.

See VALIDATION.md for checks actually performed in the build environment.

The 0.3 source adds bundled Lucide icons and user SVG lookup, full-height editor layout, a real Settings tab (theme/accent/system fonts/editor options), case/regex find with highlighting and immediate keyboard focus, event-driven filtered native filesystem watches, recent-vault switching and asynchronous XDG FileChooser with manual fallback. Texture/search/dirty caches and lazy edit checkpoints reduce idle work. See UPDATE-0.3.md and VALIDATION.md; no new native runtime verification is claimed.

The 0.3.1 hotfix disables blocking swap interval for native Wayland, preserving the default X11 behavior and adding DIAMOND_VSYNC on/off overrides plus one startup diagnostic line. The user's stack and hidden-window reproduction match the documented glutin blocking behavior; their XWayland test did not reproduce the warning. Runtime verification of the fix remains outstanding. Only diamond-app is version-bumped; library packages remain 0.3.0.

## 0.4.0 source update

Adds semantic inline links/wikilinks and rules, source-preserving round task toggles, normal styling for raw blocks, full-width clickable search cards, collapsed folder navigation, confirmed file/folder deletion, full-width tab strip, static English/Russian/Simplified Chinese translations (English default), independent interface fonts/size and system CJK fallback. The attached project logo is bundled unchanged. pulldown-cmark is pinned to 0.13.6 for wikilink fixes; egui/eframe remain 0.33.0. This revision has not been compiled or run here.

## 0.5.0 source update

Fixes independent list parsing across blanks/incomplete lines, empty-line reveal and a window-focus repaint loop. Adds committed-on-release sliders, no-clobber file rename with buffer retargeting, removes the redundant path row, and implements semantic GFM tables, standalone native images and shared cached syntax grammars for source/fences. Local images load automatically when visible; HTTP(S) requires a load click. Added regression tests include a headless unfocused-frame repaint check. None of the Rust tests, native rendering, real network image loading or Hyprland CPU behavior have been run in this environment.

## 0.6.0 source update

Adds nested container metadata/source maps, list/quote Enter continuation, mathematical-LaTeX objects, first-use numbered footnotes and in-document navigation, explicit code headings/gaps and stable larger task controls. GitHub source files were compared read-only against endstater/diamond at 1a495d05518e439f7df73a3d7609ad6b6f2c98ce. No remote writes, commits, PRs or pushes were performed. Delivery is a local update patch. Compilation and GUI runtime remain unverified here.


## 0.7.3 source update

Adds a 200 FPS frame-start ceiling without an idle timer; disables perpetual
caret/loading animations, bounds task progress to a finite transition and
compensates application deadline scheduling for egui's frame-time subtraction.
Optional logs distinguish frames from layout passes and show repaint sources.
Corrects source sharing types in vault metadata. Diamond licensing is described
as GPL-3.0-only; third-party notices are retained. This revision has not been
compiled or run on a native desktop here; CPU effects require user verification.


## 0.7.4 source update

Adds YAML property controls (text, number, ISO date, checkbox and string list),
removable/searchable tag chips and accented normal-click inline tags. Values
remain in Markdown with Undo/Redo and Live autosave. Saved YAML and inline tags
share exact/hierarchical search. Adds retained source galleys, backlink snapshots
and YAML-header reuse on body edits. Source-level verification only: compilation,
native GUI tests and further CPU improvement remain unverified here.


## 0.7.5 source update

Replaces the property constructor with one caret-aware frontmatter object in the
continuous Live editor. Original YAML is editable while the caret is inside;
outside it renders property rows/tag chips. Comments and formatting stay intact.
The constructor module and separate property mutation/serialization API are
removed. Invalid YAML remains source-visible. Native tests/build remain unrun.
