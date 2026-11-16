# Shared by test.sh and live.sh: where the KDE tests run, how their binaries are built, and how
# a command runs in a sealed-off KWin session.
#
# The machine's own KWin runs in Bubblewrap when the machine has KWin 6 and Bubblewrap;
# otherwise, or with --container, the KWin test image runs in Docker, which works on any
# Docker host, arm64 included.

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$here/../build/build.sh"
out="$root/target/live/kwin/out"

container=false
if [[ "${1:-}" == --container ]] || ! command -v kwin_wayland >/dev/null || ! command -v bwrap >/dev/null; then
    container=true
fi

# Where the binaries under test are, as the session sees them: copied into the repository,
# the only folder either session can see.
if $container; then
    bin="/work/target/live/kwin/bin"
else
    bin="$root/target/live/kwin/bin"
fi

# Builds the daemon, the live runner and fixture, and the KWin adapter's test binary, which
# lands as `kwin-tests` beside them.
build_binaries() {
    if $container; then
        build_in_container '
            cargo build -p snappy -p snappy-live --bin snappy --bin snappy-live --bin snappy-fixture
            # Compiler messages go to stderr; the JSON on stdout names the test binary.
            tests="$(cargo test -p snappy-kwin --test kwin --no-run --message-format=json |
                grep -o "\"executable\":\"[^\"]*\"" | cut -d\" -f4 | tail -1)"
            mkdir -p /work/target/live/kwin/bin
            cp /target/debug/snappy /target/debug/snappy-live /target/debug/snappy-fixture /work/target/live/kwin/bin/
            cp "$tests" /work/target/live/kwin/bin/kwin-tests'
    else
        export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-8}"
        (cd "$root" && cargo build -p snappy -p snappy-live --bin snappy --bin snappy-live --bin snappy-fixture)
        local tests built="${CARGO_TARGET_DIR:-$root/target}/debug"
        tests="$(cd "$root" && cargo test -p snappy-kwin --test kwin --no-run --message-format=json |
            grep -o '"executable":"[^"]*"' | cut -d'"' -f4 | tail -1)"
        mkdir -p "$bin"
        cp "$built/snappy" "$built/snappy-live" "$built/snappy-fixture" "$bin/"
        cp "$tests" "$bin/kwin-tests"
    fi
}

# Runs a command in a fresh sealed-off KWin session (kwin-session.sh). In the container the
# session runs as root, which D-Bus needs there, and the results are handed back after.
in_session() {
    mkdir -p "$out"
    if $container; then
        docker build --quiet --tag snappy-kwin "$here" >/dev/null
        # kwin_wayland carries the file capability cap_sys_nice, which it can only run with.
        docker run --rm --init --cap-add SYS_NICE --env OWNER="$(id -u):$(id -g)" \
            --volume "$root:/work:ro" --volume "$out:/work/target/live/kwin/out" \
            ${SNAPPY_KWIN_TEST:+--env SNAPPY_KWIN_TEST="$SNAPPY_KWIN_TEST"} \
            snappy-kwin bash -c '
                status=0
                /work/tools/live/kwin/kwin-session.sh "$@" || status=$?
                chown -R "$OWNER" /work/target/live/kwin/out
                exit "$status"' _ "$@"
    else
        "$here/kwin-session.sh" "$@"
    fi
}
