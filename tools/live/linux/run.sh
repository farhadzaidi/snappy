#!/bin/bash
# Run a command in the Linux development container with the repository mounted at /work.
# Build output goes to a named volume, so the host's target/ is never touched.
#
#   tools/live/linux/run.sh cargo test -p snappy-x11
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
image="snappy-linux-dev"

docker build --quiet --tag "$image" "$here" >/dev/null

tty_flags=()
if [[ -t 0 && -t 1 ]]; then
    tty_flags=(--interactive --tty)
fi

exec docker run --rm --init ${tty_flags[@]+"${tty_flags[@]}"} \
    --volume "$repo:/work" \
    --volume snappy-cargo-registry:/usr/local/cargo/registry \
    --volume snappy-cargo-git:/usr/local/cargo/git \
    --volume snappy-linux-target:/target \
    --workdir /work \
    "$image" "$@"
