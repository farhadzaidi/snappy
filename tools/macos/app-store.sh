#!/bin/bash
# Build Snappy for the Mac App Store: the `mac-app-store` build, universal, sandboxed, and
# packaged for App Store Connect. docs/releasing/mac-app-store.md explains the setup.
#
#   tools/macos/app-store.sh signed            Sign for the App Store and build the package.
#   tools/macos/app-store.sh unsigned          Sign ad hoc with the sandbox on, to try the
#                                              build on this Mac or check it without secrets.
#   tools/macos/app-store.sh upload <package>  Validate the package and send it to App Store
#                                              Connect.
#
# `signed` reads:
#   APPLE_TEAM_ID            the team that owns the app's App ID
#   MAS_PROVISION_PROFILE    the Mac App Store provisioning profile for io.github.farhadzaidi.Snappy
#   MAS_APP_IDENTITY         the signing identity, "Apple Distribution" unless set
#   MAS_INSTALLER_IDENTITY   the package's identity, "3rd Party Mac Developer Installer" unless
#                            set, the name Mac Installer Distribution certificates carry
#   MAS_KEYCHAIN             the keychain holding both identities, if not the default ones
# Both build modes read BUILD_NUMBER, the bundle version App Store Connect sees, which must
# be higher than that of every build uploaded before. It's the version unless set.
#
# `upload` reads APP_STORE_CONNECT_KEY_ID and APP_STORE_CONNECT_ISSUER_ID, and expects the
# key at ~/.appstoreconnect/private_keys/AuthKey_<key id>.p8, where altool looks for it.
#
# Builds go to dist/app-store/: Snappy.app and Snappy-<version>.pkg.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
packaging="$root/packaging/macos"
out="$root/dist/app-store"
work="$root/target/app-store"
app="$out/Snappy.app"
helper="$app/Contents/Helpers/Snappy App.app"
# The daemon's entitlements with the team filled in.
resolved="$work/Snappy.entitlements"
targets=(aarch64-apple-darwin x86_64-apple-darwin)
plist_buddy=/usr/libexec/PlistBuddy
# Passed as ${keychain[@]+"${keychain[@]}"}: macOS's bash 3.2 calls an empty array unbound.
keychain=()
if [[ -n "${MAS_KEYCHAIN:-}" ]]; then
    keychain=(--keychain "$MAS_KEYCHAIN")
fi

usage() {
    sed -n '4,9p' "$0" | sed 's/^# \{0,1\}//' >&2
    exit 2
}

# Snappy's version, from Cargo.
version() {
    cargo metadata --no-deps --format-version 1 --manifest-path "$root/Cargo.toml" |
        python3 -c 'import json, sys
print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "snappy"))'
}

# Build the daemon and the app for both architectures, joined into universal binaries in
# target/app-store/.
build_binaries() {
    # The binaries say they run on the release the bundles name, as App Store Connect checks.
    MACOSX_DEPLOYMENT_TARGET="$("$plist_buddy" -c "Print :LSMinimumSystemVersion" "$packaging/Snappy.plist")"
    export MACOSX_DEPLOYMENT_TARGET
    rustup target add "${targets[@]}"
    pnpm -C "$root/apps/app/ui" build
    for target in "${targets[@]}"; do
        cargo build --manifest-path "$root/Cargo.toml" --release --locked --target "$target" \
            -p snappy -p snappy-app --features snappy/mac-app-store
    done
    mkdir -p "$work"
    for binary in snappy snappy-app; do
        lipo -create -output "$work/$binary" \
            "$root/target/${targets[0]}/release/$binary" "$root/target/${targets[1]}/release/$binary"
    done
}

# Lay out Snappy.app as tools/macos/bundle.sh does, without the launch agent: this build
# registers the app itself to open at login.
assemble() {
    local version="$1" build="$2"
    rm -rf "$app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$helper/Contents/MacOS" \
        "$helper/Contents/Resources"
    cp "$packaging/Snappy.plist" "$app/Contents/Info.plist"
    "$plist_buddy" -c "Merge $packaging/AppStoreInfo.plist" "$app/Contents/Info.plist"
    cp "$packaging/SnappyApp.plist" "$helper/Contents/Info.plist"
    for plist in "$app/Contents/Info.plist" "$helper/Contents/Info.plist"; do
        "$plist_buddy" -c "Set :CFBundleShortVersionString $version" -c "Set :CFBundleVersion $build" "$plist"
    done
    cp "$root/assets/icon/snappy.icns" "$app/Contents/Resources/Snappy.icns"
    cp "$root/assets/icon/snappy.icns" "$helper/Contents/Resources/Snappy.icns"
    cp "$work/snappy" "$app/Contents/MacOS/snappy"
    cp "$work/snappy-app" "$helper/Contents/MacOS/snappy-app"
}

# Fill in the team. Ad hoc signatures leave out the two entitlements only a provisioning
# profile can grant, or macOS wouldn't start the app.
resolve_entitlements() {
    local team="$1"
    mkdir -p "$work"
    sed "s/\$(TEAM_ID)/$team/g" "$packaging/AppStore.entitlements" >"$resolved"
    if [[ -z "$team" ]]; then
        "$plist_buddy" -c "Delete :com.apple.application-identifier" \
            -c "Delete :com.apple.developer.team-identifier" "$resolved"
    fi
}

# Sign the helper, then the app around it. The daemon starts the app as its child, which
# inherits the daemon's sandbox, so the helper's entitlements may hold nothing else.
sign() {
    local identity="$1"
    codesign --force --sign "$identity" ${keychain[@]+"${keychain[@]}"} \
        --entitlements "$packaging/AppStoreApp.entitlements" "$helper"
    codesign --force --sign "$identity" ${keychain[@]+"${keychain[@]}"} --entitlements "$resolved" "$app"
    codesign --verify --strict --deep "$app"
    codesign --display --entitlements - "$app"
    codesign --display --entitlements - "$helper"
}

build() {
    local mode="$1" version build package
    version="$(version)"
    build="${BUILD_NUMBER:-$version}"
    package="$out/Snappy-$version.pkg"
    if [[ "$mode" == signed ]]; then
        : "${APPLE_TEAM_ID:?set APPLE_TEAM_ID to the team that owns the App ID}"
        : "${MAS_PROVISION_PROFILE:?set MAS_PROVISION_PROFILE to the Mac App Store provisioning profile}"
    fi
    build_binaries
    assemble "$version" "$build"
    if [[ "$mode" == signed ]]; then
        cp "$MAS_PROVISION_PROFILE" "$app/Contents/embedded.provisionprofile"
        resolve_entitlements "$APPLE_TEAM_ID"
        sign "${MAS_APP_IDENTITY:-Apple Distribution}"
        productbuild --component "$app" /Applications \
            --sign "${MAS_INSTALLER_IDENTITY:-3rd Party Mac Developer Installer}" ${keychain[@]+"${keychain[@]}"} "$package"
        pkgutil --check-signature "$package"
    else
        resolve_entitlements ""
        sign -
        productbuild --component "$app" /Applications "$package"
    fi
    echo "Built $app"
    echo "Built $package (version $version, build $build)"
}

upload() {
    local package="$1"
    : "${APP_STORE_CONNECT_KEY_ID:?set APP_STORE_CONNECT_KEY_ID}"
    : "${APP_STORE_CONNECT_ISSUER_ID:?set APP_STORE_CONNECT_ISSUER_ID}"
    local key="$HOME/.appstoreconnect/private_keys/AuthKey_$APP_STORE_CONNECT_KEY_ID.p8"
    if [[ ! -f "$key" ]]; then
        echo "No App Store Connect key at $key" >&2
        exit 1
    fi
    local auth=(--type macos --apiKey "$APP_STORE_CONNECT_KEY_ID" --apiIssuer "$APP_STORE_CONNECT_ISSUER_ID")
    xcrun altool --validate-app --file "$package" "${auth[@]}"
    xcrun altool --upload-app --file "$package" "${auth[@]}"
}

case "${1:-}" in
    signed | unsigned) build "$1" ;;
    upload)
        [[ $# -eq 2 ]] || usage
        upload "$2"
        ;;
    *) usage ;;
esac
