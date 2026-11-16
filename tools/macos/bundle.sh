#!/bin/bash
# Build Snappy.app into dist/. Signs with $SNAPPY_SIGN_IDENTITY when set, else with the
# identity from signing-identity.sh when it exists, else ad hoc. Ad-hoc builds change
# identity on every build, so macOS asks for Accessibility again.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
here="$root/tools/macos"
# The bundles' property lists and the launch agent.
packaging="$root/packaging/macos"
app="$root/dist/Snappy.app"
helper="$app/Contents/Helpers/Snappy App.app"

cd "$root"
pnpm -C apps/app/ui build
cargo build --release -p snappy -p snappy-app

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$app/Contents/Library/LaunchAgents" \
    "$helper/Contents/MacOS" "$helper/Contents/Resources"
cp "$packaging/Snappy.plist" "$app/Contents/Info.plist"
cp assets/icon/snappy.icns "$app/Contents/Resources/Snappy.icns"
cp "$packaging/io.github.farhadzaidi.Snappy.Agent.plist" "$app/Contents/Library/LaunchAgents/"
cp target/release/snappy "$app/Contents/MacOS/snappy"
cp "$packaging/SnappyApp.plist" "$helper/Contents/Info.plist"
cp assets/icon/snappy.icns "$helper/Contents/Resources/Snappy.icns"
cp target/release/snappy-app "$helper/Contents/MacOS/snappy-app"

source "$here/signing-env.sh"
identity="${SNAPPY_SIGN_IDENTITY:-$(snappy_signing_identity)}"
identity="${identity:--}"
codesign --force --options runtime --sign "$identity" "$helper"
codesign --force --options runtime --sign "$identity" "$app"
codesign --verify --strict "$app"
echo "Built $app"
