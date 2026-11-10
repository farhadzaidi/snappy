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

source "$(dirname "$0")/common.sh"
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

echo "Building the binaries…"
build_in_container '
    cargo build --release -p snappy -p snappy-live
    mkdir -p /work/target/live/gnome/bin
    cp /target/release/snappy /target/release/snappy-live /target/release/snappy-fixture \
        /work/target/live/gnome/bin/'

status=0
IFS=, read -ra wanted <<<"$versions"
for version in "${wanted[@]}"; do
    image="$(gnome_image "$version")" || exit 2
    echo "GNOME $version:"
    out="target/live/gnome/$version"
    rm -rf "${root:?}/$out"
    mkdir -p "$root/$out"
    docker run --rm --init \
        --volume "$root:/work" \
        --env "SNAPPY_GNOME_DYNAMIC_WORKSPACES=$dynamic" \
        "$image" /work/tools/live/gnome/in-container.sh "/work/$out" "$only" \
        || status=$?
    echo "Results: $root/$out"
done
exit "$status"
