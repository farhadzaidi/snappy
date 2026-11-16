#!/bin/bash
# Build Snappy.app into dist/. Signs with $SNAPPY_SIGN_IDENTITY when set, else with the
# identity from signing-identity.sh when it exists, else ad hoc. Ad-hoc builds change
# identity on every build, so macOS asks for Accessibility again. SNAPPY_SIGN_TIMESTAMP=1
# adds the secure timestamp notarization needs.
#
#   tools/macos/bundle.sh              # for this Mac's architecture
#   tools/macos/bundle.sh --universal  # for Apple silicon and Intel, as releases are
set -euo pipefail

universal=0
[[ "${1:-}" == --universal ]] && universal=1

root="$(cd "$(dirname "$0")/../.." && pwd)"
here="$root/tools/macos"
# The bundles' property lists and the launch agent.
packaging="$root/packaging/macos"
app="$root/dist/Snappy.app"
helper="$app/Contents/Helpers/Snappy App.app"

cd "$root"
pnpm -C apps/app/ui build
if ((universal)); then
    targets=(aarch64-apple-darwin x86_64-apple-darwin)
    for target in "${targets[@]}"; do
        cargo build --release --locked --target "$target" -p snappy -p snappy-app
    done
    binaries=target/universal-apple-darwin/release
    mkdir -p "$binaries"
    for program in snappy snappy-app; do
        lipo -create -output "$binaries/$program" \
            "target/${targets[0]}/release/$program" "target/${targets[1]}/release/$program"
    done
else
    cargo build --release -p snappy -p snappy-app
    binaries=target/release
fi

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$app/Contents/Library/LaunchAgents" \
    "$helper/Contents/MacOS" "$helper/Contents/Resources"
cp "$packaging/Snappy.plist" "$app/Contents/Info.plist"
cp assets/icon/snappy.icns "$app/Contents/Resources/Snappy.icns"
cp "$packaging/io.github.farhadzaidi.Snappy.Agent.plist" "$app/Contents/Library/LaunchAgents/"
cp "$binaries/snappy" "$app/Contents/MacOS/snappy"
cp "$packaging/SnappyApp.plist" "$helper/Contents/Info.plist"
cp assets/icon/snappy.icns "$helper/Contents/Resources/Snappy.icns"
cp "$binaries/snappy-app" "$helper/Contents/MacOS/snappy-app"

source "$here/signing-env.sh"
identity="${SNAPPY_SIGN_IDENTITY:-$(snappy_signing_identity)}"
identity="${identity:--}"
signing=(--force --options runtime --sign "$identity")
# Notarization requires a secure timestamp, which takes a call to Apple's server.
[[ "${SNAPPY_SIGN_TIMESTAMP:-}" == 1 ]] && signing+=(--timestamp)
codesign "${signing[@]}" "$helper"
codesign "${signing[@]}" "$app"
codesign --verify --strict "$app"
echo "Built $app"
