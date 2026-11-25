# Source input responsiveness (0.7.8)

The previous path highlighted every line synchronously whenever any text changed.
Markdown Enter also constructed a complete Document/SourceMap just to continue a
list/quote. No native performance measurement was possible in the preparation
environment, so their exact share of the reported latency is unknown.

## Implementation

- One lazy worker for all Source tabs/languages; a condition variable sleeps when
  no requests exist. Completion requests a redraw; there is no idle poll clock.
- Pending edits replace earlier requests for the same tab. Running jobs cancel
  between lines. Old results never become the current absolute token map.
- Tokens are cached per physical line, with parse/highlight state every 32 lines.
  Work resumes before the edit and stops when an unchanged suffix has the same
  parser and highlight state. Inserting newlines can reuse shifted suffix lines.
- Eight completed buffer caches; at most 512 KiB, 8192 lines and 16 KiB per line.
  Beyond these limits, Source remains editable with plain text colors.
- During work, unchanged prefix/suffix tokens are remapped on Unicode boundaries.
  Only the changed region falls back to plain color until a current result arrives.
- Source Undo stores actual callback insertions/deletions and caret selections,
  capped at 256 transactions / 8 MiB including Redo. Selection replacement and
  native multi-operation edits form one transaction per GUI pass. IME commits
  form one transaction; cancelled IME does not create an undo point.
- Egui still makes transient text copies and lays out the whole buffer. The text
  widget is not virtualized. Live document history/fence highlighting are separate.
- Source Enter uses lightweight container/fence inspection rather than semantic
  document parsing. Dirty checks borrow current text instead of allocating bytes.

## Reproduce and diagnose

Use the same release build, note, Source mode, font, wrapping and window size.
Open the 900-line note and repeat the same six Enter/delete/type operations.

```sh
DIAMOND_PROFILE=1 ./target/release/diamond --foreground /path/to/note.md 2>diamond-source.log
```

Logs include source `layout_ms`, `syntax_ms`, bytes and `lines_reparsed`, only when
actual work happens. They do not include document contents. Compare with:

```sh
DIAMOND_PROFILE=1 DIAMOND_SYNTAX=0 ./target/release/diamond --foreground /path/to/note.md 2>diamond-plain.log
```

The second command disables Source grammar coloring for diagnosis only. If both
are slow, compare source layout times with whole-GUI profiling rather than assume
syntax is the remaining cause. An edit inside a multiline construct may correctly
invalidate the suffix; line checkpoints are not a fixed time budget for one regex.

LSP is not required to edit or highlight a note. Per-language executables, server
lifecycle, incremental UTF-16 synchronization and asynchronous semantic-token
replies are a separate feature; no language server is started by opening files.
