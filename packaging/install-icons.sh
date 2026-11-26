#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Called by package() with the staging prefix and the owner's chosen SVG.
set -euo pipefail

diamond_pkgroot=${1:?package staging directory required}
diamond_svg=${2:?source SVG required}
diamond_icon_id=io.diamond.Diamond
command -v rsvg-convert >/dev/null

install -Dm644 "$diamond_svg" "$diamond_pkgroot/usr/share/icons/hicolor/scalable/apps/$diamond_icon_id.svg"
ln -s "$diamond_icon_id.svg" "$diamond_pkgroot/usr/share/icons/hicolor/scalable/apps/diamond-notes.svg"

for diamond_size in 16 24 32 48 64 128 256 512; do
    diamond_icon_dir="$diamond_pkgroot/usr/share/icons/hicolor/${diamond_size}x${diamond_size}/apps"
    install -dm755 "$diamond_icon_dir"
    rsvg-convert --width "$diamond_size" --height "$diamond_size" \
        --output "$diamond_icon_dir/$diamond_icon_id.png" "$diamond_svg"
    chmod 644 "$diamond_icon_dir/$diamond_icon_id.png"
    ln -s "$diamond_icon_id.png" "$diamond_icon_dir/diamond-notes.png"
done

# Compatibility for launchers that fall back to /usr/share/pixmaps.
install -Dm644 "$diamond_pkgroot/usr/share/icons/hicolor/256x256/apps/$diamond_icon_id.png" \
    "$diamond_pkgroot/usr/share/pixmaps/$diamond_icon_id.png"
ln -s "$diamond_icon_id.png" "$diamond_pkgroot/usr/share/pixmaps/diamond-notes.png"
