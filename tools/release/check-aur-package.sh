#!/bin/bash
# Check a rendered AUR package inside an archlinux:base container, and write its .SRCINFO next
# to the PKGBUILD:
#
#   docker run --rm -v "$PWD:/work" -w /work archlinux:base \
#       tools/release/check-aur-package.sh target/packages/aur/snappy-app-bin [--download]
#
# --download also fetches the release's tarball for this machine's architecture and checks it
# against the PKGBUILD's checksum, so use it only once the release is published.
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 || ( $# -eq 2 && "$2" != "--download" ) ]]; then
    echo "usage: $0 <package folder> [--download]" >&2
    exit 2
fi
package="$(cd "$1" && pwd)"
download="${2:-}"

pacman -Sy --noconfirm --needed base-devel namcap >/dev/null

# makepkg refuses to run as root, so the checks run as a user of their own on a copy.
id builder >/dev/null 2>&1 || useradd --create-home builder
build="$(mktemp -d)"
cp "$package/PKGBUILD" "$build/"
chown -R builder "$build"
as_builder() {
    (cd "$build" && setpriv --reuid=builder --regid=builder --init-groups -- "$@")
}

echo "== namcap"
as_builder namcap PKGBUILD
as_builder makepkg --printsrcinfo >"$package/.SRCINFO"
# The folder is usually mounted from the host; leave the file to whoever owns the PKGBUILD.
chown --reference="$package/PKGBUILD" "$package/.SRCINFO"
echo "== .SRCINFO"
cat "$package/.SRCINFO"
if [[ "$download" == "--download" ]]; then
    echo "== Checking the release's tarball"
    as_builder makepkg --verifysource --noconfirm
fi
