#!/bin/bash
# Run the live scenarios against the real daemon in a headless KWin sealed off from this
# machine's desktop (kwin-session.sh), with the fixture's windows on Wayland and again
# through Xwayland. Never runs against the desktop.
#
#   tools/live/kwin/live.sh [--container] [scenario name prefix]
#
# Builds the daemon, the runner, and the fixture app, and runs them in the machine's own KWin
# in Bubblewrap or in the KWin test image in Docker (common.sh). Results land in
# target/live/kwin/out/live/<wayland|xwayland>/.
set -euo pipefail

source "$(dirname "$0")/common.sh"
[[ "${1:-}" == --container ]] && shift

echo "Building the daemon, the runner, and the fixture…"
build_binaries

in_session bash -c '
    bin="$1"
    out="$SNAPPY_KWIN_OUT/live"
    mkdir -p "$out"
    status=0
    for display in wayland x11; do
        name="$display"
        if [[ "$display" == x11 ]]; then
            name=xwayland
            # KWin started Xwayland for its own children; a client from outside finds it here.
            export DISPLAY="$(grep -ao "Xwayland .* :[0-9]*" "$SNAPPY_KWIN_OUT/kwin.log" | grep -o ":[0-9]*$" | tail -1)"
            export DISPLAY="${DISPLAY:-:0}"
        fi
        echo "== $name windows"
        SNAPPY_LIVE_SANDBOX=1 SNAPPY_LIVE_FIXTURE_DISPLAY="$display" "$bin/snappy-live" --app "$bin/snappy" \
            --fixture "$bin/snappy-fixture" --out "$out/$name" ${2:+--only "$2"} || status=1
    done
    exit "$status"' _ "$bin" "${1:-}"
