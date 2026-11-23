# diamond-vault

Filesystem and metadata boundary for an ordinary folder of files. All public file operations accept relative paths beneath a canonical vault root. Absolute paths, `..`, empty paths, symlinks and special files are rejected. Directory enumeration skips symlinks, `.git`, `.hg`, `.svn`, `.diamond`, `node_modules`, `target` and temporary Diamond files.

Reads return bytes without text conversion. Files larger than 32 MiB cannot be opened through this bounded alpha API. Markdown indexing considers UTF-8 `.md` files up to 2 MiB, with a 64 MiB aggregate source budget, and enumeration is bounded to 50,000 entries and 64 directory levels. Skipped non-UTF-8 and oversized Markdown files are recorded in `VaultIndex.skipped`.

`save` compares disk bytes to the version loaded by the caller, writes and fsyncs a same-directory temporary file, checks the expected bytes again, preserves permissions and atomically renames. `create` uses a no-clobber commit. Save errors never discard the caller's in-memory state. Parent folders must already exist; `create_dir` creates one validated folder.

This prevents ordinary stale-buffer overwrites, but portable filesystem APIs cannot eliminate a concurrent change between the last comparison and rename, or hostile directory replacement. Vaults should not be concurrently manipulated by untrusted processes. No cross-process advisory lock or recovery/version journal is implemented yet.

The rebuildable index includes titles, headings, wikilinks, tags, tasks, simple full-text search and backlinks. It deliberately uses a bounded line scanner for the initial Markdown subset; top-level frontmatter, fenced code and indented code are excluded from metadata extraction. Nested block containers and HTML are not parsed as a full Markdown AST. Extensionless wikilinks match sibling or vault-relative paths, then unique note stems. It does not execute note contents or plugins.

`remove(relative, directory)` performs explicit permanent deletion after checking the path and entry kind; the desktop asks for confirmation and warns before discarding affected unsaved tabs. The vault root cannot be deleted through this API. Recursive removal does not follow nested symlinks. Deletion has no undo/trash implementation.

`rename_file(relative, name)` accepts one normal filename component and rejects directories, traversal, symlinks and destination collisions. A same-directory hard link is created with no-clobber semantics, then the original name is removed; on unlink failure the new link is removed on a best-effort basis. A crash between operations may leave both names, so this is not advertised as an atomic rename. Directory sync after a successful commit is best effort; the new path is still returned so session state follows the committed filesystem change.

0.7 `move_entry` moves regular files or directories with Linux NOREPLACE rename;
`rename_file` also supports directories. Existing destinations, descendant moves,
root/traversal and symlinks remain rejected. The app updates descendants of open
tabs after a directory move. Nested task metadata now uses the semantic Markdown
parser, excluding code blocks and including tasks beyond four spaces of indent.
`read_bounded` is public so host image policy can enforce its smaller limit before
allocation. Markdown remains authoritative and unrelated links are not rewritten.
