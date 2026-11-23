# diamond-app

Native desktop orchestration for Diamond. Run `cargo run -p diamond-app -- /path/to/vault`.

`session.rs` owns file sessions, conflict-aware saves and dirty close decisions;
widgets dispatch these operations. Indexing and plugin processes use workers.
The application stores preferences in `$XDG_CONFIG_HOME/diamond/settings.json`
(or `$HOME/.config/diamond/settings.json`). A vault never supplies executable
configuration. Plugin manifests are loaded only through an explicit user action;
launch requires a separate confirmation and runs on a worker.

The interface supports native Markdown/source/hex editing, vault search, tags,
headings/backlinks, tabs, command palette, keyboard shortcuts and explicit dirty
document decisions. Detailed coverage and limitations are in `docs/ACCEPTANCE.md`.

The desktop uses a neutral black/gray palette, native vector icons, rounded
controls and an 8/12/16-point spacing rhythm. The status bar is 40 points high
with 14-point text. Markdown uses a centered readable column; the inspector
temporarily hides in compact windows to leave enough editing space.

Formatted `.md` notes autosave after a 600 ms pause. Their header and command
palette omit the manual Save action; Ctrl+S can still flush explicitly. Source
and hex modes retain manual saving. The status bar distinguishes pending,
saved and failed saves. Conflicts retain the buffer and stop automatic retries.
Closing a tab, changing vault or quitting first flushes eligible Markdown;
remaining unsaved changes use the existing explicit close decision. IME
preedit is never saved as committed text.

Settings is an ordinary document-area tab. It supports Dark/Light/System, accent colors, fc-list font selection and editor layout options. The status bar has no duplicate toast/close widget. `watch.rs` uses native nonrecursive watches only for admitted vault directories, updates registrations after filesystem events and coalesces event bursts without periodic scans. `portal.rs` runs XDG FileChooser on a worker and distinguishes cancellation from unavailable portals. Recent vault switches use the same dirty-buffer policy. `diamond2.svg` is read once on startup from project/binary locations. Lucide SVGs and their full license are bundled locally.

0.3.1 selects a nonblocking swap interval on native Wayland to avoid documented hidden-window EGL waits; X11 retains vsync. DIAMOND_VSYNC=on/off overrides the policy for diagnosis, and stderr prints a single startup configuration line. No renderer/dependency upgrade or repaint timer is added. Runtime CPU/ANR behavior still needs verification on the target system.

0.4 defaults the interface to English, with Russian and Simplified Chinese settings. UI font and text size are independent from editor preferences; Fontconfig supplies a system CJK fallback. Vault folders start collapsed, click to expand/collapse and offer right-click delete with confirmation. Search cards have equal width and a full-card hit area, and the tab-strip frame fills the available width. The supplied user SVG is embedded.

0.5 defers slider preference writes until pointer release, removes the extra path row beneath tabs, and adds a rename dialog. Sessions retarget open buffers without discarding unsaved content; renaming away from .md also removes Live mode. resources.rs supplies safe vault-relative image reads and explicitly requested HTTP(S) fetches with finite time/byte limits. Image decoding runs in the editor worker.

0.7 orchestrates prepared-plugin installation and protocol v2 native split views.
Plugin rendering and PNG decode run off the GUI thread; document revisions guard
stale frames. Editable view sources opt into existing conflict-aware autosave.
Directory moves retarget open buffers and image loaders. File CLI arguments use a
light session without directory enumeration or a watcher/index. Notifications
expire after three seconds with a one-shot repaint deadline, not a polling loop.
See packaging/PKGBUILD.local for an Arch package built from the current checkout.
