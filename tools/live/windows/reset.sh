#!/bin/bash
# Return the Windows test VM to the state create.sh left it in, discarding every change since:
# the disk snapshot taken right after the install, and the firmware variables saved with it.
#
#   tools/live/windows/reset.sh
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
source "$here/common.sh"

"$here/stop.sh"
qemu-img snapshot -a "$installed_snapshot" "$disk"
cp "$installed_efi_vars" "$efi_vars"
