#!/bin/bash
# Run the live scenarios against Snappy on GNOME Shell under Wayland, in throwaway headless
# GNOME Shells in containers, never on this machine's desktop.
#
#   tools/live/gnome/live.sh [--gnome 46,47,48,49] [--static-workspaces] [scenario name prefix]
#
# Builds the daemon, runner, and fixture app on Ubuntu 24.04, then for each GNOME version
# (46 by default) starts a fresh GNOME Shell (gnome-session.sh) and runs suite.sh in it:
# the daemon's first launch installs its extension, a restart of GNOME Shell stands in for
# logging out and back in, and the scenarios run once with Wayland windows and once with
# Xwayland ones. Results land in target/live/gnome/<version>/.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
versions="46"
dynamic=true
only=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --gnome) versions="$2"; shift 2 ;;
        --static-workspaces) dynamic=false; shift ;;
        *) only="$1"; shift ;;
    esac
done

declare -A bases=([46]=ubuntu:24.04 [47]=fedora:41 [48]=fedora:42 [49]=fedora:43)

echo "Building the binaries…"
docker build --quiet --tag snappy-gnome-build -f "$here/Dockerfile.build" "$here" >/dev/null
docker run --rm --init \
    --volume "$root:/work" \
    --volume snappy-cargo-registry:/usr/local/cargo/registry \
    --volume snappy-cargo-git:/usr/local/cargo/git \
    --volume snappy-gnome-target:/target \
    --workdir /work snappy-gnome-build bash -c '
        set -euo pipefail
        cargo build --release -p snappy -p snappy-live
        mkdir -p /work/target/live/gnome/bin
        cp /target/release/snappy /target/release/snappy-live /target/release/snappy-fixture \
            /work/target/live/gnome/bin/
        chown -R "$(stat -c %u:%g /work)" /work/target/live'

status=0
IFS=, read -ra wanted <<<"$versions"
for version in "${wanted[@]}"; do
    base="${bases[$version]:-}"
    if [[ -z "$base" ]]; then
        echo "No image for GNOME $version; choose from ${!bases[*]}." >&2
        exit 2
    fi
    echo "GNOME $version:"
    docker build --quiet --tag "snappy-gnome:$version" --build-arg "BASE=$base" "$here" >/dev/null
    out="target/live/gnome/$version"
    rm -rf "${root:?}/$out"
    mkdir -p "$root/$out"
    docker run --rm --init \
        --volume "$root:/work" \
        --env "SNAPPY_GNOME_DYNAMIC_WORKSPACES=$dynamic" \
        "snappy-gnome:$version" /work/tools/live/gnome/in-container.sh "/work/$out" "$only" \
        || status=$?
    echo "Results: $root/$out"
done
exit "$status"
