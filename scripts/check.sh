#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

# A development snapshot may not yet have a lockfile. Resolve it once on a
# network-enabled builder, then keep it with the source for subsequent runs.
if [[ ! -f Cargo.lock ]]; then
  cargo generate-lockfile
fi

cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings

# Check the resolved native dependency graph. web-sys/wasm-bindgen may appear
# as target-specific transitive packages; neither embeds a browser engine.
cargo tree --workspace --locked --edges normal --prefix none --format '{p}' |
  python3 -c '
import sys

forbidden = {
    "cef", "cef-sys", "chromium", "electron", "headless_chrome",
    "webkit2gtk", "webkit2gtk-sys", "webkit6", "webkit6-sys", "wry",
}
packages = {line.split()[0] for line in sys.stdin if line.strip()}
found = sorted(name for name in packages
               if name in forbidden or name == "tauri" or name.startswith("tauri-"))
if found:
    sys.exit("Browser runtime in the native workspace: " + ", ".join(found))
print("Native dependency graph: no forbidden browser runtime packages")
'
