#!/bin/bash
# Run the app's end-to-end tests in Playwright's own container, in Chromium and WebKit. Some
# tests measure how text lays out, so they run with the container's fonts on every machine,
# CI included, rather than whatever fonts the host has.
#
#   tools/e2e.sh                          # every test, in both browsers
#   tools/e2e.sh --project=chromium       # any other arguments go to `playwright test`
#
# Needs Docker, and `pnpm -C apps/app/ui install` run first.
set -euo pipefail
cd "$(dirname "$0")/.."

ui=apps/app/ui
# The image's version must match the Playwright the tests are written against.
version="$(node -p "require('./$ui/package.json').devDependencies['@playwright/test']")"
pnpm_version="$(node -p "require('./$ui/package.json').packageManager.split('@')[1]")"

# The container runs as root; hand its results back to whoever ran this.
exec docker run --rm --init --ipc=host \
    -v "$PWD:/work" -w "/work/$ui" \
    -e CI -e SNAPPY_UI_PORT -e OWNER="$(id -u):$(id -g)" -e PNPM="pnpm@$pnpm_version" \
    "mcr.microsoft.com/playwright:v$version-noble" \
    bash -c 'npx -y "$PNPM" exec playwright test "$@"
        status=$?
        chown -R "$OWNER" test-results playwright-report 2>/dev/null
        exit $status' bash "$@"
