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

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
versions="46"
filter=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --gnome) versions="$2"; shift 2 ;;
        *) filter="$1"; shift ;;
    esac
done
declare -A bases=([46]=ubuntu:24.04 [47]=fedora:41 [48]=fedora:42 [49]=fedora:43)

echo "Building the tests…"
docker build --quiet --tag snappy-gnome-build -f "$here/Dockerfile.build" "$here" >/dev/null
docker run --rm --init \
    --volume "$root:/work" \
    --volume snappy-cargo-registry:/usr/local/cargo/registry \
    --volume snappy-cargo-git:/usr/local/cargo/git \
    --volume snappy-gnome-target:/target \
    --workdir /work snappy-gnome-build bash -c '
        set -euo pipefail
        cargo build --release -p snappy-live --bin snappy-fixture
        tests="$(cargo test --release -p snappy-gnome --no-run --message-format=json 2>/dev/null \
            | grep -o "\"executable\":\"[^\"]*\"" | cut -d\" -f4)"
        mkdir -p /work/target/live/gnome/tests
        rm -f /work/target/live/gnome/tests/*
        cp $tests /target/release/snappy-fixture /work/target/live/gnome/tests/
        chown -R "$(stat -c %u:%g /work)" /work/target/live'

status=0
IFS=, read -ra wanted <<<"$versions"
for version in "${wanted[@]}"; do
    base="${bases[$version]:-}"
    [[ -n "$base" ]] || { echo "No image for GNOME $version; choose from ${!bases[*]}." >&2; exit 2; }
    echo "GNOME $version:"
    docker build --quiet --tag "snappy-gnome:$version" --build-arg "BASE=$base" "$here" >/dev/null
    docker run --rm --init --volume "$root:/work:ro" --env SNAPPY_GNOME_PREINSTALL=1 \
        "snappy-gnome:$version" /work/tools/live/gnome/gnome-session.sh bash -c '
            export SNAPPY_GNOME_TEST=1 SNAPPY_GNOME_FIXTURE=/work/target/live/gnome/tests/snappy-fixture
            status=0
            for test in /work/target/live/gnome/tests/*; do
                [[ "$(basename "$test")" == snappy-fixture ]] && continue
                "$test" --test-threads=1 '"${filter:+"$filter"}"' || status=1
            done
            exit $status' || status=$?
done
exit "$status"
