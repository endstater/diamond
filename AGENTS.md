# Diamond — instructions for agents

Read `docs/ARCHITECTURE.md`, `docs/STATUS.md` and the nearest crate README before changing code.

Diamond targets Arch Linux. The default app is Rust + egui/eframe; no browser runtime is permitted in its dependency graph. Excalidraw is an optional, separately built application using the system WebKitGTK engine.

## Boundaries

- `diamond-core`: settings and shared types; no GUI or filesystem policy.
- `diamond-document`: semantic Markdown document, lossless untouched blocks, character-index edits, formatting and undo. No egui dependency.
- `diamond-vault`: safe paths, conflict-aware atomic saves, vault metadata/search/link index. No egui.
- `diamond-editor`: egui adapters over the document model and native raw editor.
- `diamond-app`: desktop orchestration and visual design. No business rules in widgets.

## Invariants

1. Never silently overwrite an external edit. Compare bytes against the loaded version before saving.
2. Never decode arbitrary files with replacement characters and write them back. UTF-8 text is editable; other bytes use an explicit hexadecimal view/edit path.
3. Source bytes of an untouched Markdown document must be identical on save. Unknown constructs survive unchanged.
4. Public editor positions count Unicode scalar values, filesystem/source spans count bytes. Conversion must be explicit. Grapheme-aware navigation is the GUI's responsibility.
5. Keep the semantic document authoritative. Formatted editing is not syntax recoloring or a preview pane. Markdown markers are serialized from semantic blocks and marks.
6. Undo and redo include formatting and block structure, not only text.
7. Never claim full Obsidian or VS Code parity in UI, docs or release notes without updating the acceptance matrix.
8. Opening a vault or note must not execute scripts, shell snippets, plugins or remote content.

## Validation

Run `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings`. Use `scripts/check.sh` on Arch. The Excalidraw helper has its own manifest and validation. Document unrun checks and environment constraints in `docs/VALIDATION.md`.

Prefer tests for document round trips, Unicode transactions, binary integrity and filesystem conflicts. Do not add tests that only assert widget labels.
