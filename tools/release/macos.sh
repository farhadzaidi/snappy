#!/bin/bash
# Build dist/release/Snappy-<version>-macos-universal.dmg: a universal Snappy.app, built by
# tools/macos/bundle.sh, beside a link to /Applications.
#
# Signing, from the first that applies:
#   - MACOS_CERTIFICATE_P12 (base64) and MACOS_CERTIFICATE_PASSWORD: a Developer ID
#     Application certificate, imported into a keychain of its own for this run. With
#     APPLE_TEAM_ID set, the identity must belong to that team.
#   - SNAPPY_SIGN_IDENTITY: an identity already in your keychains.
#   - Otherwise ad hoc. Such a build only runs after the user approves it in System
#     Settings, and macOS asks for Accessibility again after every update.
# With APP_STORE_CONNECT_KEY_ID, APP_STORE_CONNECT_ISSUER_ID, and APP_STORE_CONNECT_KEY_P8
# (base64) also set, a signed build is notarized, and the app and the disk image are stapled.
#
# Prints `signing=<developer-id|identity|ad-hoc>` and `notarized=<yes|no>` last, for CI.
set -euo pipefail

# shellcheck source=tools/release/common.sh
source "$(dirname "$0")/common.sh"
cd "$release_root"

version="$(snappy_version)"
app="$release_root/dist/Snappy.app"
dmg="$release_out/Snappy-$version-macos-universal.dmg"
work="$(mktemp -d)"
keychain=""
original_keychains=()

cleanup() {
    if [[ -n "$keychain" ]]; then
        # macOS's own bash 3.2 calls an empty array unset.
        security list-keychains -d user -s ${original_keychains[@]+"${original_keychains[@]}"} || true
        security delete-keychain "$keychain" || true
    fi
    rm -rf "$work"
}
trap cleanup EXIT

# Imports the Developer ID certificate into a temporary keychain and signs with it.
import_certificate() {
    keychain="$work/release.keychain-db"
    local password
    password="$(uuidgen)"
    base64 --decode <<<"$MACOS_CERTIFICATE_P12" >"$work/certificate.p12"
    security create-keychain -p "$password" "$keychain"
    # No automatic locking during a long build.
    security set-keychain-settings "$keychain"
    security unlock-keychain -p "$password" "$keychain"
    security import "$work/certificate.p12" -k "$keychain" -P "$MACOS_CERTIFICATE_PASSWORD" \
        -T /usr/bin/codesign >/dev/null
    rm "$work/certificate.p12"
    # Let codesign use the key without asking.
    security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$keychain" >/dev/null
    while IFS= read -r listed; do
        original_keychains+=("$(tr -d '" ' <<<"$listed")")
    done < <(security list-keychains -d user)
    security list-keychains -d user -s "$keychain" "${original_keychains[@]}"

    SNAPPY_SIGN_IDENTITY="$(security find-identity -p codesigning "$keychain" |
        awk -v team="(${APPLE_TEAM_ID:-})" '
            !found && /"Developer ID Application: / && (team == "()" || index($0, team)) { print $2; found = 1 }')"
    [[ -n "$SNAPPY_SIGN_IDENTITY" ]] ||
        die "MACOS_CERTIFICATE_P12 holds no Developer ID Application identity${APPLE_TEAM_ID:+ for team $APPLE_TEAM_ID}"
}

# A string field of notarytool's one-line JSON output.
json_field() {
    sed -n "s/.*\"$1\" *: *\"\([^\"]*\)\".*/\1/p" "$2"
}

# Submits a file for notarization and waits for the verdict, printing Apple's log on failure.
notarize() {
    local file="$1" result status id
    result="$work/notarization.json"
    xcrun notarytool submit "$file" --key "$work/api-key.p8" --key-id "$APP_STORE_CONNECT_KEY_ID" \
        --issuer "$APP_STORE_CONNECT_ISSUER_ID" --wait --timeout 1h --output-format json >"$result" || true
    status="$(json_field status "$result")"
    if [[ "$status" != Accepted ]]; then
        cat "$result" >&2
        id="$(json_field id "$result")"
        if [[ -n "$id" ]]; then
            xcrun notarytool log "$id" --key "$work/api-key.p8" --key-id "$APP_STORE_CONNECT_KEY_ID" \
                --issuer "$APP_STORE_CONNECT_ISSUER_ID" >&2 || true
        fi
        die "notarization of $(basename "$file") ended as ${status:-unknown}"
    fi
}

signing=ad-hoc
if [[ -n "${MACOS_CERTIFICATE_P12:-}" ]]; then
    [[ -n "${MACOS_CERTIFICATE_PASSWORD:-}" ]] || die "MACOS_CERTIFICATE_P12 is set without MACOS_CERTIFICATE_PASSWORD"
    import_certificate
    signing=developer-id
elif [[ -n "${SNAPPY_SIGN_IDENTITY:-}" && "$SNAPPY_SIGN_IDENTITY" != - ]]; then
    signing=identity
else
    # Never the local development identity bundle.sh would otherwise pick up.
    SNAPPY_SIGN_IDENTITY=-
fi
export SNAPPY_SIGN_IDENTITY

notarizing=0
if [[ "$signing" != ad-hoc && -n "${APP_STORE_CONNECT_KEY_ID:-}" && -n "${APP_STORE_CONNECT_ISSUER_ID:-}" &&
    -n "${APP_STORE_CONNECT_KEY_P8:-}" ]]; then
    notarizing=1
    base64 --decode <<<"$APP_STORE_CONNECT_KEY_P8" >"$work/api-key.p8"
fi
if [[ "$signing" != ad-hoc ]]; then
    export SNAPPY_SIGN_TIMESTAMP=1
fi

tools/macos/bundle.sh --universal
lipo "$app/Contents/MacOS/snappy" -verify_arch arm64 x86_64
lipo "$app/Contents/Helpers/Snappy App.app/Contents/MacOS/snappy-app" -verify_arch arm64 x86_64

if ((notarizing)); then
    # Notarize the app on its own first, so it's stapled before it goes in the disk image and
    # opens offline once copied out of it.
    ditto -c -k --keepParent "$app" "$work/Snappy.zip"
    notarize "$work/Snappy.zip"
    xcrun stapler staple "$app"
    xcrun stapler validate "$app"
fi

# The disk image: the app and a link to /Applications to drag it onto.
staging="$work/dmg"
mkdir -p "$staging" "$release_out"
ditto "$app" "$staging/Snappy.app"
ln -s /Applications "$staging/Applications"
rm -f "$dmg"
# hdiutil now and then fails with "Resource busy" on build machines; a retry gets past it.
for attempt in 1 2 3; do
    if hdiutil create -volname Snappy -srcfolder "$staging" -fs HFS+ -format UDZO -ov "$dmg"; then
        break
    fi
    ((attempt < 3)) || die "hdiutil couldn't create the disk image"
    sleep 5
done

if [[ "$signing" != ad-hoc ]]; then
    codesign --force --timestamp --sign "$SNAPPY_SIGN_IDENTITY" "$dmg"
fi
if ((notarizing)); then
    notarize "$dmg"
    xcrun stapler staple "$dmg"
    xcrun stapler validate "$dmg"
    spctl --assess --type open --context context:primary-signature --verbose "$dmg"
    spctl --assess --type execute --verbose "$app"
fi

echo "Built $dmg"
echo "signing=$signing"
echo "notarized=$( ((notarizing)) && echo yes || echo no)"
