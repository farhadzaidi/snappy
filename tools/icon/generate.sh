#!/bin/bash
# Renders every platform's icon files from assets/icon/snappy.svg, and the menu bar and top
# bar glyphs from assets/icon/snappy-glyph.svg. Run it after changing either SVG and commit
# what it writes, so builds need none of its tools. Needs rsvg-convert (librsvg), macOS's
# iconutil, and Node.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
icon="$root/assets/icon"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

render() {
    rsvg-convert --width "$1" --height "$1" --output "$2" "$icon/snappy.svg"
}

# macOS: both bundles' .icns, from an iconset at each size and its @2x.
iconset="$work/snappy.iconset"
mkdir "$iconset"
for size in 16 32 128 256 512; do
    render "$size" "$iconset/icon_${size}x${size}.png"
    render $((size * 2)) "$iconset/icon_${size}x${size}@2x.png"
done
iconutil --convert icns --output "$icon/snappy.icns" "$iconset"

# Windows: the app's .ico, and the same images as the resource the daemon links.
windows=()
for size in 16 24 32 48 64 256; do
    render "$size" "$work/windows-$size.png"
    windows+=("$work/windows-$size.png")
done
node "$root/tools/icon/windows.mjs" "$icon/snappy.ico" "$icon/snappy.res" "${windows[@]}"

# The MSIX package's logos, at each scale Windows picks from; MakePri indexes them by these names.
# Tiles hold the icon at two thirds of their height on a transparent background. The unplated
# app list sizes keep the taskbar and Start from drawing the icon on an accent-colored plate.
msix="$root/packaging/msix/Assets"
mkdir -p "$msix"
place() { # page width, page height, icon size, output
    rsvg-convert --width "$3" --height "$3" --page-width "$1" --page-height "$2" \
        --left $((($1 - $3) / 2)) --top $((($2 - $3) / 2)) --output "$4" "$icon/snappy.svg"
}
at() { echo $(($1 * scale / 100)); } # a size at the current scale
for scale in 100 200 400; do
    render "$(at 44)" "$msix/Square44x44Logo.scale-$scale.png"
    render "$(at 50)" "$msix/StoreLogo.scale-$scale.png"
    place "$(at 150)" "$(at 150)" "$(at 100)" "$msix/Square150x150Logo.scale-$scale.png"
    place "$(at 310)" "$(at 150)" "$(at 100)" "$msix/Wide310x150Logo.scale-$scale.png"
done
for size in 16 24 32 48 256; do
    render "$size" "$msix/Square44x44Logo.targetsize-${size}_altform-unplated.png"
done

# The app's icon on Linux, which Tauri embeds.
render 256 "$icon/snappy.png"

# The macOS menu bar glyph, a vector PDF the daemon embeds and marks as a template image. Its
# page is the glyph's size in points, and the fixed date keeps the file the same each run.
SOURCE_DATE_EPOCH=0 rsvg-convert --format pdf --dpi-x 72 --dpi-y 72 \
    --output "$icon/snappy-glyph.pdf" "$icon/snappy-glyph.svg"

# The GNOME top bar glyph. GNOME Shell recolors a symbolic icon to match the top bar, so it's the
# glyph itself, in the gray symbolic icons are drawn in.
sed -e 's/#000000/#bebebe/' \
    -e 's|<!--.*-->|<!-- Written by tools/icon/generate.sh from assets/icon/snappy-glyph.svg. -->|' \
    "$icon/snappy-glyph.svg" >"$root/crates/gnome/extension/icons/snappy-symbolic.svg"

echo "Wrote $icon, the MSIX package's logos, and the GNOME extension's icon"
