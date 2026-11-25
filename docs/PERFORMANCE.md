# Performance audit — 0.7.2

These are source-level findings and changes, not measured CPU percentages.
Rust/Cargo and a running Diamond desktop are unavailable in the preparation
environment; compilation, native regression tests and real process CPU profiling
still need to run on the target machine.

| Previous work | Change |
| --- | --- |
| Rebuild/parse source maps at several caret/projection points every frame | One shared map per model revision; binary block lookup |
| Deep-clone the full Undo history for each checkpoint | Share immutable past states through Arc |
| Serialize, diff and parse the whole note again when grouping callbacks/IME | Group validated history directly, keeping the current parsed state |
| Rebuild projection and per-character layout on redraw | Retain projection and native galley; invalidate on actual source/view changes |
| Render all formulas, including offscreen ones, with a 24-entry FIFO | Cull to visible blocks; use a 128-entry / 64 MiB LRU |
| Measure table columns and rebuild visible cell layouts repeatedly | Cache table measurements and up to 512 cell galleys; visit visible rows/cells |
| Segment emoji for both layout and painting on each frame | Bounded text-run cache, ASCII fast path and visible-run painting |
| Re-read/parse every note after a vault change | Reuse unchanged note metadata/source after identity/size/mtime/ctime checks |
| Build task-count vectors every frame | Cached nested task counts per model state |
| Construct drag payloads and paint clipped tree rows | Allocate payload only on drag start; skip offscreen paint |
| Reveal checks could disagree when the widget kept focus in an inactive window | Require actual window focus in both phases; stop inactive spinner/progress animation |

## Polling audit

VaultWatch uses OS notifications through RecommendedWatcher. Plugin request and
stdio loops block on channels/reads. GUI try_recv calls only drain completed
work during an existing frame: they do not wake the app periodically. Filesystem
coalescing (80 ms), autosave, preferences and notification expiry use event-driven,
one-shot deadlines. They are not recurring scans. Cursor blinking, active
spinners, animations, user input and a worker completion may legitimately repaint.
Directory enumeration still runs after vault events; it is not a periodic job.

## Optional timing log

From the project root, after a successful release build:

```sh
DIAMOND_PROFILE=1 ./target/release/diamond 2>diamond-profile.log
```

Every five seconds *when an update occurs*, the log can report `updates`,
`updates_per_sec`, `gui_mean_ms` and `gui_max_ms`. These are GUI-update timings,
not total process CPU or guaranteed presentation FPS. It logs no note contents
and schedules no timer/repaint. Silence while idle can mean the app is asleep.
Without the environment variable, the profiler is disabled.

Compare the same vault and note before/after in these conditions: focused idle,
typing, scrolling, moving the pointer, and another window/workspace focused.
Include a note with more than 24 formulas, a tall/wide table and nested quotes.
Inspect process CPU separately in a system monitor; GUI timings exclude workers,
driver work and the compositor. No fixed percentage improvement is claimed.

## Layout checks

- An empty line between a heading and a table keeps ordinary line height;
  the table height belongs only to its placeholder glyph, not its newline.
- A quote bar stops at its final text glyph and does not cover the next paragraph.
- Tall formulas reserve their box height; adjacent paint uses native advances.
- A complete leading `\left.\begin{array}...\end{array}\right\}` followed by
  `\Rightarrow`, `\Longrightarrow`, `\implies` or `=` uses two centered boxes.
  Other formula internals are still typeset by latex-rust, not a full TeX engine.
- The HEX accent field accepts #RRGGBB and applies on Enter/focus loss.

Run `cargo fmt --all`, `cargo test --workspace` and
`cargo clippy --workspace --all-targets -- -D warnings` on the target machine.

## 0.7.3: frame ceiling and idle work

The user's 0.7.2 trace reported 430–772 App::update calls/second with nonblocking
Wayland swap. These calls are not necessarily presented frames: egui can perform
multiple layout passes per frame. The trace does not identify a single cause
of the continuous redraws. Native CPU measurements for this update are pending.

- A frame pacer runs in eframe 0.33's raw_input_hook, once per native frame.
  Requested frame starts are separated by at least 5 ms (200 FPS ceiling).
  It sleeps only for the remaining portion of those 5 ms, without busy spinning,
  periodic wakes or catch-up frames. It does not schedule a redraw. OS scheduling
  can lower the rate. This is a small bounded GUI-thread wait, not a custom event
  loop or a blocking Wayland buffer swap. Window events can wait up to 5 ms.
- The caret is steady. Loading indicators are static; worker completion wakes
  the GUI. These widgets no longer create an idle blink/spinner clock.
- Task progress uses a finite 250 ms transition, scheduled about every 16 ms.
  It returns the exact final value and stops requesting frames at the endpoint.
  Unfocused/reduced-motion windows show the final value immediately.
- egui 0.33 subtracts predicted_dt from delayed repaint requests. Application
  one-shot deadlines now compensate for this subtraction to avoid premature
  wakes repeatedly checking an autosave, preference-save, watcher debounce,
  toast expiry or preview debounce deadline. This is not periodic polling.
- Existing finite hover/scroll transitions and input/expose/worker/filesystem
  events may still request frames. An idle editor has no intentional periodic
  repaint. This needs confirmation on the user's Hyprland setup.

The optional log now separates native `frames_per_sec` from layout
`updates_per_sec`, counts focused frames and input/pointer events, and prints
up to eight repaint source locations. Layout passes may exceed 200/second even
when frame starts stay below 200/second. Frame counts are GUI frames, not a
measurement of compositor presentation. Logs contain no note or typed text.
No profiling work is enabled without DIAMOND_PROFILE=1.

After building 0.7.3, compare focused idle, unfocused idle, typing and scrolling.
If continuous updates remain, the `[diamond repaint]` lines help locate the
remaining requester rather than guessing from CPU usage alone.


## 0.7.4: work avoided on unchanged frames

The user reports CPU usage fell to approximately 20% after 0.7.3. This is their
measurement, not a locally reproduced benchmark; idle/action state was not
specified. No further percentage reduction is claimed without a new trace.

- Source mode now retains its native galley, skipping grammar/job construction,
  emoji decoration and find highlighting on unchanged redraws. The cache checks
  text and relevant layout/search/font/DPI/theme state; one additional source
  snapshot is limited to 2 MiB per editor. Text comparison remains linear.
- The inspector retains backlink indices for the active path/index generation.
  It no longer scans/resolves every vault wikilink on each GUI frame.
- YAML properties cache by document revision and retain the same parsed header
  across body-only edits when its original bytes still match. YAML parsing does not happen per repaint. The 64 KiB header limit bounds editing work.
- Tags are extracted during parsing/index updates, not by scanning source every
  frame. Tag queries are normalized once per search recomputation. No new polling,
  worker, animation clock or filesystem watcher is added.

The 200 FPS ceiling remains. If 20% CPU occurs while idle, collect a fresh
DIAMOND_PROFILE log including repaint sources; cache changes alone do not prove
that all event/repaint loops have stopped. Large-file parsing, text comparison
and initial full layout are still synchronous and not incremental.


## 0.7.5: property block simplification

Removed the property form and mapping-serialization mutation path. Property
editing is the existing source-edit transaction flow, and comments are preserved.
Inactive properties reserve one native object row; cached row/tag galleys render
only when visible. Active YAML reuses the existing syntax cache. Layout and paint
share the same reserved height even when text/tag rows wrap. No timer or worker
was added. Source, backlink and YAML caches and the 200 FPS ceiling are retained.
Native performance and interaction remain unverified in this environment.


## 0.7.6 diagrams

Only visible inactive Mermaid objects enqueue parse/layout/raster work. One lazy
worker blocks on a bounded channel; completed textures are cached by source/theme.
No spinner/repaint loop keeps a diagram waiting frame alive. Font discovery runs
once on the worker. Input, graph, SVG and raster sizes are bounded; texture cache
cost is capped at 64 MiB plus small source/metadata entries. Diagram completion
invalidates geometry only if the reserved height changes.

Background process startup runs once before GUI initialization, with one socket
acknowledgement and a ten-second failure timeout. There is no supervisor/polling
thread after launch. The 200 FPS ceiling remains. Neither native CPU percentages
nor end-to-end detachment have been measured in this environment.
