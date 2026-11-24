<p align="center">
  <img src="assets/diamond.svg" alt="Diamond logo" width="96" />
</p>

# Diamond

A native, local-first workspace for Markdown notes, source files and ideas. Built with Rust and egui for Linux; your notes stay ordinary files.

Inspired by the facets and perfection of a cut diamond, the project brings useful tools into one polished workspace. Open source makes its inner workings crystal-clear: inspect the code, shape the tools, and own your files.

## Features

- Caret-aware YAML properties: type source, render when the caret leaves.
- Clickable Unicode tags with exact and nested-tag search.
- Live Markdown with autosave, nested lists and tasks, quotes, tables, math and footnotes.
- Wikilinks, backlinks, headings, and case-sensitive or regex search.
- Native image previews, including SVG; source and hexadecimal editing.
- Multiple vaults, collapsible folders, rename and drag-and-drop moves.
- Syntax highlighting for Rust, Python, TypeScript, Typst and more.
- Plugins in any language, native split previews, and installation from GitHub or a folder.
- Dark/light/system themes, custom fonts, HEX accent input, scroll speed and color emoji.
- English, Russian and Chinese interfaces; a lightweight single-file mode.

## Run from source

Requires Rust 1.88+ and the Linux graphics libraries. From the project root:

```sh
cargo run --release -p diamond-app
```

## Install on Arch Linux

Build a package from this checkout, including its application launcher and icon:

```sh
sudo pacman -S --needed base-devel rust cargo pkgconf
cd packaging
makepkg -si -p PKGBUILD.local
```

Then open a vault or a single file:

```sh
diamond .
diamond ~/vault_1
diamond ~/notes/idea.md
```

Color emoji need a system color font, such as `noto-fonts-emoji`.

See [properties and tags](docs/PROPERTIES.md) for editing and search.

Alpha software. See [validation](docs/VALIDATION.md) and the [plugin API](docs/PLUGINS.md). PDF/Typst engines are not bundled; one plugin process is active at a time.

## License

[GNU GPL v3 only](LICENSE) (`GPL-3.0-only`). Third-party components retain their [own licenses](THIRD_PARTY_LICENSES.md).
