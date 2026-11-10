#!/bin/bash
# Inside a GNOME test container, as root: run suite.sh in a fresh GNOME session, then hand
# the results to the owner of the repository folder.
#
#   in-container.sh <results folder> [scenario name prefix]
set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
results="$1"
only="${2:-}"
scratch="$(mktemp -d)"
chmod 777 "$scratch"

"$here/gnome-session.sh" "$here/suite.sh" /work/target/live/gnome/bin "$scratch" "$only"
status=$?

cp -r "$scratch/." "$results/"
chown -R "$(stat -c %u:%g /work)" "$results"
exit "$status"
