#!/bin/bash
# The full check from AGENTS.md.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
# The GNOME Shell extensions are plain JavaScript modules; GNOME Shell is the only place they
# run, so their syntax is checked here.
for file in apps/gnome-extension/*.js tools/live/gnome/test-extension/*.js; do
    node --input-type=module --check <"$file" || { echo "in $file" >&2; exit 1; }
done
# The KWin script's files start with QML's `.import` and `.pragma` lines, which aren't
# JavaScript; the rest is checked as a classic script.
for file in apps/kwin-script/*.js; do
    sed 's/^\.\(import\|pragma\) .*//' "$file" | node --check || { echo "in $file" >&2; exit 1; }
done
if [[ -f apps/app/ui/package.json ]]; then
    pnpm -C apps/app/ui check
fi
