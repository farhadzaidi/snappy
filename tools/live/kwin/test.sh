#!/bin/bash
# Run the KWin adapter's tests, its integration tests included, against a real headless
# KWin sealed off from this machine's desktop (kwin-session.sh), never against the desktop.
#
#   tools/live/kwin/test.sh [--container] [test name filter]
#
# Builds the daemon, the test binary, and the fixture app, then runs the tests one at a time
# in a fresh KWin with no compositing and no GPU: the machine's own KWin in Bubblewrap, or the
# KWin test image in Docker (common.sh). The daemon's files and output from the end-to-end
# tests land in target/live/kwin/out/daemon/.
set -euo pipefail

source "$(dirname "$0")/common.sh"
[[ "${1:-}" == --container ]] && shift

echo "Building the tests…"
build_binaries

SNAPPY_KWIN_TEST=1 in_session bash -c '
    export SNAPPY_KWIN_FIXTURE="$1/snappy-fixture" SNAPPY_KWIN_DAEMON="$1/snappy"
    "$1/kwin-tests" --test-threads=1 ${2:+"$2"}' _ "$bin" "${1:-}"
