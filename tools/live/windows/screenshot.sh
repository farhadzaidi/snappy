#!/bin/bash
# Save what the Windows test VM's display shows as a PNG, without opening any window.
#
#   tools/live/windows/screenshot.sh [file, default ~/VMs/snappy-windows/screen.png]
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
source "$here/common.sh"

file="${1:-$vm_dir/screen.png}"
case "$file" in
    /*) ;;
    *) file="$PWD/$file" ;;
esac

vm_running || { echo "The VM isn't running." >&2; exit 1; }
screenshot "$file"
echo "$file"
