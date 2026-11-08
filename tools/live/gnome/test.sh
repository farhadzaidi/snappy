#!/bin/bash
# Run the GNOME adapter's tests, its integration tests included, against real headless GNOME
# Shells in containers, never on this machine's desktop.
#
#   tools/live/gnome/test.sh [--gnome 46,47,48,49] [test name filter]
#
# Builds the test binary and the fixture app on Ubuntu 24.04, then for each GNOME version
# (46 by default) runs the tests one at a time in a fresh GNOME Shell with Snappy's extension
# and the test helper loaded.
set -euo pipefail

source "$(dirname "$0")/common.sh"
versions="46"
filter=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --gnome) versions="$2"; shift 2 ;;
        *) filter="$1"; shift ;;
    esac
done

echo "Building the tests…"
build_in_container '
    cargo build --release -p snappy-live --bin snappy-fixture
    # Compiler messages go to stderr and stay visible; the JSON on stdout names the binaries.
    mapfile -t tests < <(cargo test --release -p snappy-gnome --no-run --message-format=json \
        | grep -o "\"executable\":\"[^\"]*\"" | cut -d\" -f4)
    mkdir -p /work/target/live/gnome/tests
    rm -f /work/target/live/gnome/tests/*
    cp "${tests[@]}" /target/release/snappy-fixture /work/target/live/gnome/tests/'

status=0
IFS=, read -ra wanted <<<"$versions"
for version in "${wanted[@]}"; do
    image="$(gnome_image "$version")" || exit 2
    echo "GNOME $version:"
    docker run --rm --init --volume "$root:/work:ro" --env SNAPPY_GNOME_PREINSTALL=1 \
        "$image" /work/tools/live/gnome/gnome-session.sh bash -c '
            export SNAPPY_GNOME_TEST=1 SNAPPY_GNOME_FIXTURE=/work/target/live/gnome/tests/snappy-fixture
            status=0
            for test in /work/target/live/gnome/tests/*; do
                [[ "$(basename "$test")" == snappy-fixture ]] && continue
                "$test" --test-threads=1 ${1:+"$1"} || status=1
            done
            exit $status' _ "$filter" || status=$?
done
exit "$status"
