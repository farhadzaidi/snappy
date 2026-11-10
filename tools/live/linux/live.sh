#!/bin/bash
# Run the live scenarios against the X11 daemon in the Linux development container,
# never on this machine's desktop.
#
#   tools/live/linux/live.sh [scenario name prefix]
#
# In the container it builds the daemon, the runner, and the fixture app; starts the
# headless X session (x11-session.sh); runs the scenarios; and copies the results,
# screenshots, the daemon's output, and its debug log to target/live/linux/out.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
results="target/live/linux/out"

if [[ "${1:-}" != --in-container ]]; then
    rm -rf "$root/$results"
    mkdir -p "$root/$results"
    status=0
    "$here/run.sh" tools/live/linux/live.sh --in-container "${1:-}" || status=$?
    echo "Results: $root/$results/results.json"
    exit "$status"
fi

# Inside the container, at the repository root. A target folder of its own keeps other
# builds in the shared volume from replacing these binaries mid-run.
only="${2:-}"
export CARGO_TARGET_DIR=/target/live
cargo build --release -p snappy -p snappy-live

bin="$(mktemp -d)"
out="$(mktemp -d)/out"
for program in snappy snappy-live snappy-fixture; do
    cp "$CARGO_TARGET_DIR/release/$program" "$bin/"
done

status=0
"$here/x11-session.sh" "$bin/snappy-live" --app "$bin/snappy" --fixture "$bin/snappy-fixture" \
    --out "$out" ${only:+--only "$only"} || status=$?

cd "$out"
tar -cf - $(ls -d results.json *.png home/daemon.log home/logs 2>/dev/null) | tar -C "/work/$results" -xf - || true
exit "$status"
