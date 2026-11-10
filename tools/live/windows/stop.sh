#!/bin/bash
# Shut the Windows test VM down cleanly, or power it off if it doesn't stop within two minutes.
#
#   tools/live/windows/stop.sh
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
source "$here/common.sh"

vm_running || exit 0

qmp '{"execute": "system_powerdown"}' >/dev/null
if ! wait_for_exit 120; then
    qmp '{"execute": "quit"}' >/dev/null
    wait_for_exit 30
fi
