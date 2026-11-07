#!/bin/bash
# The full check from AGENTS.md.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
if [[ -f apps/settings/ui/package.json ]]; then
    pnpm -C apps/settings/ui check
fi
