#!/bin/bash
# Regenerate the README's screenshots, and drafts for the store listings, all in containers.
#
#   tools/screenshots/capture.sh            # the desktop, then the app
#   tools/screenshots/capture.sh desktop    # Snappy's zones over a headless GNOME desktop
#   tools/screenshots/capture.sh app        # Snappy's window, against the fake daemon
#
# The desktop: Snappy built as for the live suite, running in a headless GNOME 49 Shell
# (tools/live/gnome) with real apps, captured by GNOME itself. The app: the UI in
# Playwright's container (as tools/e2e.sh runs it), which also turns every capture into the
# README's WebP images in assets/screenshots/. Everything else lands in target/screenshots/:
# the raw captures, and PNG drafts sized for the Microsoft Store and the Mac App Store.
#
# Needs Docker, `pnpm -C apps/app/ui install`, and the WebAssembly tools from CONTRIBUTING.md.
set -euo pipefail

source "$(dirname "$0")/../live/gnome/common.sh"
screenshots="$root/tools/screenshots"
what="${1:-all}"
gnome_version=49

desktop() {
    echo "Building Snappy…"
    build_in_container '
        cargo build --release -p snappy
        mkdir -p /work/target/live/screenshots
        cp /target/release/snappy /work/target/live/screenshots/'

    echo "Capturing the desktop in GNOME $gnome_version…"
    local base image=snappy-screenshots-gnome
    base="$(gnome_image "$gnome_version")"
    docker build --quiet --tag "$image" --build-arg "BASE=$base" "$screenshots/gnome" >/dev/null
    rm -rf "$root/target/screenshots/desktop"
    # The host name is what the terminal's prompt shows.
    docker run --rm --init --hostname desktop \
        --volume "$root:/work" \
        --env SNAPPY_GNOME_MONITORS=1 \
        --env SNAPPY_GNOME_PREINSTALL=1 \
        "$image" /work/tools/screenshots/gnome/in-container.sh \
        /work/target/live/screenshots/snappy /work/target/screenshots/desktop
}

app() {
    echo "Building the app's WebAssembly core…"
    pnpm -C "$root/apps/app/ui" wasm >/dev/null

    echo "Capturing the app…"
    local ui=apps/app/ui version pnpm_version
    version="$(node -p "require('$root/$ui/package.json').devDependencies['@playwright/test']")"
    pnpm_version="$(node -p "require('$root/$ui/package.json').packageManager.split('@')[1]")"
    mkdir -p "$root/target/screenshots" "$root/assets/screenshots"
    # The container runs as root; hand what it writes back to whoever ran this, Vite's caches
    # included, or the next `vitest` on this machine can't write its own.
    docker run --rm --init --ipc=host \
        -v "$root:/work" -w "/work/$ui" \
        -e SNAPPY_UI_PORT -e OWNER="$(id -u):$(id -g)" -e PNPM="pnpm@$pnpm_version" \
        "mcr.microsoft.com/playwright:v$version-noble" \
        bash -c 'npx -y "$PNPM" exec playwright test --config playwright.screenshots.config.ts
            status=$?
            chown -R "$OWNER" /work/target/screenshots /work/assets/screenshots test-results \
                node_modules/.vite node_modules/.vite-temp 2>/dev/null
            exit $status'
}

case "$what" in
    all) desktop; app ;;
    desktop) desktop ;;
    app) app ;;
    *) echo "Usage: $0 [all|desktop|app]" >&2; exit 2 ;;
esac
echo "README images: $root/assets/screenshots"
echo "Store drafts and raw captures: $root/target/screenshots"
