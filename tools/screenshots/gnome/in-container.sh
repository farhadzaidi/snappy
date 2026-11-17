#!/bin/bash
# Inside the screenshot image, as root: run scene.sh in a fresh headless GNOME session, then
# hand its screenshots to the owner of the repository folder.
#
#   in-container.sh <snappy executable> <results folder>
set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
results="$2"
scratch="$(mktemp -d)"
chmod 777 "$scratch"

/work/tools/live/gnome/gnome-session.sh "$here/scene.sh" "$1" "$scratch"
status=$?

mkdir -p "$results"
cp "$scratch"/*.png "$results/"
chown -R "$(stat -c %u:%g /work)" "$results"
exit "$status"
