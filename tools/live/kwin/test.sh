#!/bin/bash
# Run the KWin adapter's tests, its integration tests included, against a real headless
# KWin sealed off from this machine's desktop (kwin-session.sh), never against the desktop.
#
#   tools/live/kwin/test.sh [test name filter]
#
# Builds the daemon, the test binary, and the fixture app here, then runs the tests one at a
# time in a fresh KWin with no compositing and no GPU. The daemon's files and output from
# the end-to-end test land in target/live/kwin/out/daemon/.
set -euo pipefail

root="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$root"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-8}"

echo "Building the tests…"
cargo build -p snappy -p snappy-live --bin snappy --bin snappy-fixture
# Compiler messages go to stderr and stay visible; the JSON on stdout names the binary.
test_binary="$(cargo test -p snappy-kwin --test kwin --no-run --message-format=json |
    grep -o '"executable":"[^"]*"' | cut -d'"' -f4 | tail -1)"

SNAPPY_KWIN_TEST=1 tools/live/kwin/kwin-session.sh bash -c '
    export SNAPPY_KWIN_FIXTURE="$PWD/target/debug/snappy-fixture" SNAPPY_KWIN_DAEMON="$PWD/target/debug/snappy"
    "$1" --test-threads=1 ${2:+"$2"}' _ "$test_binary" "${1:-}"
