#!/bin/bash
# Render the Homebrew cask and the winget manifests for one release, from the templates in
# packaging/ and the release's SHA256SUMS.
#
#   tools/release/render-packages.sh <version> <SHA256SUMS> <outdir>
#
# <version> has no leading "v", as in 0.1.0. The output is laid out as each channel wants it:
#   <outdir>/homebrew/Casks/snappy.rb
#   <outdir>/winget/manifests/f/FarhadZaidi/Snappy/<version>/*.yaml
set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "usage: $0 <version> <SHA256SUMS> <outdir>" >&2
    exit 2
fi
version="$1"
sums="$2"
out="$3"
packaging="$(cd "$(dirname "$0")/../.." && pwd)/packaging"

# Package managers get stable releases only.
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "The version must look like 1.2.3, without a leading v: $version" >&2
    exit 1
fi
if [[ ! -f "$sums" ]]; then
    echo "No SHA256SUMS at $sums" >&2
    exit 1
fi

# Print the SHA-256 that SHA256SUMS lists for a file. Lines are "<hash>  <name>", or
# "<hash> *<name>" from sha256sum's binary mode.
sum_of() {
    local name="$1" hash
    hash="$(awk -v name="$name" '{ file = $2; sub(/^\*/, "", file); if (file == name) { print tolower($1); exit } }' "$sums")"
    if [[ ! "$hash" =~ ^[0-9a-f]{64}$ ]]; then
        echo "SHA256SUMS has no SHA-256 for $name" >&2
        return 1
    fi
    echo "$hash"
}

dmg="$(sum_of "Snappy-$version-macos-universal.dmg")"
msi_x64="$(sum_of "Snappy-$version-windows-x64.msi")"
msi_arm64="$(sum_of "Snappy-$version-windows-arm64.msi")"

# Fill in a template, and fail if a placeholder is left, so a new one can't ship unfilled.
render() {
    local template="$1" target="$2"
    mkdir -p "$(dirname "$target")"
    sed -e "s/@VERSION@/$version/g" \
        -e "s/@SHA256_MACOS_DMG@/$dmg/g" \
        -e "s/@SHA256_WINDOWS_X64_MSI@/$msi_x64/g" \
        -e "s/@SHA256_WINDOWS_ARM64_MSI@/$msi_arm64/g" \
        "$template" >"$target"
    if grep -n '@[A-Z0-9_]*@' "$target" >&2; then
        echo "$template has a placeholder this script doesn't fill" >&2
        exit 1
    fi
    echo "$target"
}

render "$packaging/homebrew/snappy.rb.template" "$out/homebrew/Casks/snappy.rb"

winget="$out/winget/manifests/f/FarhadZaidi/Snappy/$version"
for template in "$packaging"/winget/*.yaml.template; do
    render "$template" "$winget/$(basename "$template" .template)"
done
