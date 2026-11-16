#!/bin/bash
# Lint the macOS and Windows adapters, with the daemon and the live harness, from any machine:
# the code that only compiles for those systems is type-checked and linted for them, without
# linking. Run beside tools/check.sh when a change reaches those adapters.
#
#   tools/cross-check.sh [macos|windows]
#
# Needs `rustup target add aarch64-apple-darwin aarch64-pc-windows-msvc` and
# `cargo install --locked cargo-xwin`, which downloads the Windows SDK headers on first use.
set -euo pipefail
cd "$(dirname "$0")/.."

which="${1:-all}"
crates=(-p snappy -p snappy-live)

if [[ "$which" == all || "$which" == macos ]]; then
    echo "macOS…"
    cargo clippy --target aarch64-apple-darwin -p snappy-macos "${crates[@]}" --all-targets -- -D warnings
    echo "macOS, the Mac App Store build…"
    cargo clippy --target aarch64-apple-darwin -p snappy-macos -p snappy --features snappy/mac-app-store \
        --all-targets -- -D warnings
fi
if [[ "$which" == all || "$which" == windows ]]; then
    echo "Windows…"
    XWIN_ACCEPT_LICENSE=1 cargo xwin clippy --target aarch64-pc-windows-msvc -p snappy-windows "${crates[@]}" \
        --all-targets -- -D warnings
fi
