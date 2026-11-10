#!/bin/bash
# Boot the Windows test VM headless and wait until SSH answers.
#
#   tools/live/windows/start.sh
#
# Nothing appears on this Mac's screen. To look at the VM's display, take a screenshot with
# `tools/live/windows/screenshot.sh`, or connect a VNC viewer to 127.0.0.1:5915.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
source "$here/common.sh"

[[ -f "$disk" ]] || { echo "No VM at $vm_dir. Create it with tools/live/windows/create.sh." >&2; exit 1; }

if ! vm_running; then
    launch_vm
fi
wait_for_ssh 600
echo "The VM is up: tools/live/windows/ssh.sh <command>"
