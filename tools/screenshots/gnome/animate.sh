#!/bin/bash
# Turn hero.sh's frames into a looping animated WebP, each frame scaled down, with the
# pointer drawn where it was: GNOME's screenshots leave it out.
#
#   animate.sh <folder with frames.tsv> <output.webp> [width]
set -euo pipefail

frames="$1"
output="$2"
width="${3:-1280}"

# Lossy WebP quality, and the encoder's effort (0 to 6).
QUALITY=80
EFFORT=6

# An arrow pointer, as points from its tip, at the screen's own size.
POINTER="0,0 0,34 9,26 15,40 21,37 15,24 26,24"

scratch="$(mktemp -d)"
args=(-loop 0 -lossy -q "$QUALITY" -m "$EFFORT")
while IFS=$'\t' read -r file x y duration; do
    magick "$frames/$file" -alpha off \
        -fill white -stroke '#111' -strokewidth 2 -draw "translate $x,$y polygon $POINTER" \
        -resize "${width}x" "$scratch/$file"
    args+=(-d "$duration" "$scratch/$file")
done <"$frames/frames.tsv"

img2webp "${args[@]}" -o "$output" >/dev/null
rm -r "$scratch"
