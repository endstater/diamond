# License scope

Diamond code is GPL-3.0-only; see LICENSE and NOTICE.
Dependencies, icons and fonts retain their own licenses listed below.

# Bundled icon assets

The SVG files in `assets/lucide/` are from [Lucide](https://github.com/lucide-icons/lucide).
Lucide is licensed under ISC; inherited Feather icons carry the MIT notice
included in the upstream license. The complete notice is retained in
`assets/lucide/LICENSE`, embedded in the application Settings, and installed by
the Arch package. `assets/lucide/provenance.json` records upstream blob IDs.

Only the selected local SVGs are bundled. They are rasterized once and cached;
the running application does not fetch icons or fonts from the network.

The project owner's supplied `diamond2.svg` is included unchanged at the root and as `assets/diamond.svg`; it is the embedded application brand, separate from Lucide. A local `diamond2.svg` beside the binary can override it at startup.

0.5 uses Syntect 5.2.0 (grammar/theme data bundled by that crate), image 0.25.8 and ureq 2.12.1 through Cargo. Their upstream license notices and dependency metadata remain supplied by the corresponding crates: https://github.com/trishume/syntect, https://github.com/image-rs/image, https://github.com/algesten/ureq. No webview/browser runtime is embedded for image display.

0.6 uses latex-rust 2.1.1 (MIT OR Apache-2.0) with its embedded STIX Two Math font (SIL OFL 1.1), without the optional egui backend. The selected engine MIT notice and complete STIX license are retained in assets/licenses/LATEX-RUST-MIT.txt and assets/licenses/STIX-OFL.txt and installed by the Arch package. Upstream source: https://github.com/jscarr64/latex-rust/tree/v2.1.1. Only the bundled trusted math font is used.

## 0.7 additions

- Two-face 0.5.2 — MIT OR Apache-2.0; embedded grammars have their own licenses.
  The complete compiled acknowledgements are available in Settings → Syntax licenses
  (`two_face::acknowledgement::listing().to_md()`). Source: https://codeberg.org/CosmicHarper/two-face.
- Syntect upgraded to 5.3.0, same pure Rust fancy regex backend.
- Swash 0.2.6 — Apache-2.0 OR MIT, https://github.com/dfrg/swash.
- unicode-segmentation 1.12.0 — MIT OR Apache-2.0, https://github.com/unicode-rs/unicode-segmentation.
- rustix — Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT; native no-replace rename.
- wait-timeout 0.2.1 — MIT OR Apache-2.0; bounded git subprocess lifetime.
- base64 0.22.1 — MIT OR Apache-2.0; plugin preview wire data.

Color emoji are rendered from a user's installed font; no emoji font or artwork
is bundled in this snapshot. Noto Color Emoji is an optional system dependency
under its own font license. The updated diamond.svg is supplied by the project owner.

## 0.7.4 YAML properties

- serde_yaml_ng 0.10.0 — MIT; https://github.com/acatton/serde-yaml-ng.
  YAML is parsed as data; custom YAML tags do not execute code. The dependency's
  own license and unsafe-libyaml dependency notices are supplied by Cargo.

## 0.7.6 native diagrams

- mermaid-rs-renderer 0.3.1 — MIT; https://github.com/1jehuang/mermaid-rs-renderer/tree/v0.3.1.
  The complete notice is in assets/licenses/MERMAID-RS-RENDERER-MIT.txt and installed by both Arch recipes.
  Used as a library with default CLI/PNG features disabled. Its dependency
  notices are supplied by Cargo; no upstream browser/Node assets are bundled.
- Existing resvg 0.45.1 now enables text/system-font support for diagram labels.
  System fonts keep their own licenses; no additional font files are bundled.
