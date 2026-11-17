#!/bin/bash
# Inside the screenshot image, as root: record hero.sh's frames in a fresh headless GNOME
# session, turn them into the animation, and hand both to the owner of the repository folder.
#
#   in-container.sh <snappy executable> <repository bundle> <frames folder> <animation>
set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
frames="$3"
animation="$4"
scratch="$(mktemp -d)"
chmod 777 "$scratch"

"$here/../../live/gnome/gnome-session.sh" "$here/hero.sh" "$1" "$2" "$scratch"
status=$?
if [[ "$status" == 0 ]]; then
    "$here/animate.sh" "$scratch" "$animation"
    status=$?
fi

mkdir -p "$frames"
cp "$scratch"/* "$frames/"
owner="$(stat -c %u:%g /work)"
chown -R "$owner" "$frames"
[[ -f "$animation" ]] && chown "$owner" "$animation"
exit "$status"
