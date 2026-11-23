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
