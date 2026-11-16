# Sourced by the container suites: builds their binaries in one shared container, so they
# run in every test image, newer systems included.

build_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$build_dir/../../.." && pwd)"

# Runs a bash script in the build container, with the repository at /work and cargo's output
# under /target, then hands target/live to the owner of the repository folder.
build_in_container() {
    docker build --quiet --tag snappy-live-build "$build_dir" >/dev/null
    docker run --rm --init \
        --volume "$root:/work" \
        --volume snappy-cargo-registry:/usr/local/cargo/registry \
        --volume snappy-cargo-git:/usr/local/cargo/git \
        --volume snappy-live-target:/target \
        --workdir /work snappy-live-build bash -c "
            set -euo pipefail
            $1
            chown -R \"\$(stat -c %u:%g /work)\" /work/target/live"
}
