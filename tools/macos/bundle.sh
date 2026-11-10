#!/bin/bash
# Build Snappy.app into dist/. Signs with $SNAPPY_SIGN_IDENTITY when set, else with the
# identity from signing-identity.sh when it exists, else ad hoc. Ad-hoc builds change
# identity on every build, so macOS asks for Accessibility again.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
here="$root/tools/macos"
app="$root/dist/Snappy.app"
helper="$app/Contents/Helpers/Snappy Settings.app"

cd "$root"
pnpm -C apps/settings/ui build
cargo build --release -p snappy -p snappy-settings

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Library/LaunchAgents" "$helper/Contents/MacOS"
cp "$here/Snappy.plist" "$app/Contents/Info.plist"
cp "$here/local.snappy.agent.plist" "$app/Contents/Library/LaunchAgents/"
cp target/release/snappy "$app/Contents/MacOS/snappy"
cp "$here/SnappySettings.plist" "$helper/Contents/Info.plist"
cp target/release/snappy-settings "$helper/Contents/MacOS/snappy-settings"

source "$here/signing-env.sh"
identity="${SNAPPY_SIGN_IDENTITY:-$(snappy_signing_identity)}"
identity="${identity:--}"
codesign --force --options runtime --sign "$identity" "$helper"
codesign --force --options runtime --sign "$identity" "$app"
codesign --verify --strict "$app"
echo "Built $app"
