# Properties and tags

Type a YAML block at the very beginning of a Markdown note:

```yaml
---
tags:
  - матан
  - идз
  - двойной-интеграл
вариант: 19
дедлайн: 2026-11-16
статус: решение готово
status: Todo
---
```

In **Live** mode, frontmatter behaves like a document block:

- While the caret is inside, the original YAML and delimiters are visible, with
  YAML syntax highlighting. Type, delete, select, copy and undo normally.
- Move the caret to a line after the closing `---` to render the properties.
- Click the rendered block, or move the caret into it with the keyboard, to edit
  the source again. Tag chips open tag search instead of editing.

There is no property constructor, form, Add button or separate panel. To create,
rename or remove a property/tag, edit its YAML text. Everything remains in the
same note, source transaction history and conflict-aware Live autosave. Source
mode always shows the literal YAML.

Comments, quotes, indentation, CRLF/BOM and body bytes are preserved; rendering
never serializes the mapping back into the file. Invalid, non-mapping, unclosed
or oversized frontmatter stays visible as editable source. The render/parser
limit is 64 KiB; complex values are data, never executable content. Only metadata
at the beginning of the note is rendered as properties, not fenced YAML examples.

## Tags

`#матан`, `#двойной-интеграл` and `#work/linux` in supported Markdown text are
accented and open vault search with a normal click. Ctrl is not required for tags;
ordinary Markdown links retain Ctrl+click. Rendered YAML chips and tags in the
right inspector also open search. Source mode keeps text editing behavior.

YAML `tags` and inline tags share the same index. Search `tag:матан` (or `#матан`)
to match that tag and its children, such as `#матан/идз`, but not `#матанализ`.
Matching is case-insensitive; Unicode letters, numbers, underscore, hyphen and
slash are supported, and a tag must contain a letter or underscore. Tags in code,
escaped hashes, URL fragments and existing link labels are excluded.

Search uses saved notes and updates after normal autosave/filesystem events.
Single-file light mode has no vault index. Raw/unsupported Markdown stays
source-visible rather than receiving approximate tag click coordinates.
