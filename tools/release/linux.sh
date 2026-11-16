#!/bin/bash
# Build Snappy's Linux packages for this machine's architecture into dist/release/:
#
#   snappy-app_<version>_<amd64|arm64>.deb
#   snappy-app-<version>-1.<x86_64|aarch64>.rpm
#   snappy-app-<version>-linux-<x86_64|aarch64>.tar.gz
#
# All three hold the same tree, laid out as under /usr. The package is `snappy-app` because
# `snappy` is taken in the distributions; the commands are still `snappy` and `snappy-app`.
# Needs dpkg-deb and rpmbuild besides the build tools. --no-build packages binaries that are
# already in the target folder.
set -euo pipefail

# shellcheck source=tools/release/common.sh
source "$(dirname "$0")/common.sh"
cd "$release_root"

build=1
[[ "${1:-}" == --no-build ]] && build=0

version="$(snappy_version)"
arch="$(uname -m)"
case "$arch" in
x86_64) deb_arch=amd64 ;;
aarch64) deb_arch=arm64 ;;
*) die "no Linux packages for $arch" ;;
esac
command -v dpkg-deb >/dev/null || die "needs dpkg-deb (the dpkg package)"
command -v rpmbuild >/dev/null || die "needs rpmbuild (the rpm package on Debian and Ubuntu)"
[[ -f LICENSE.md ]] || die "needs LICENSE.md at the root of the repository"
# An SPDX expression for the rpm's License tag, from Cargo.toml when it has one.
license="$(workspace_field license)"
license="${license:-LicenseRef-Snappy}"

if ((build)); then
    pnpm -C apps/app/ui build
    cargo build --release --locked -p snappy -p snappy-app
fi
binaries="$release_root/target/release"

name="snappy-app-$version-linux-$arch"
work="$release_root/target/release-linux"
tree="$work/$name"
rm -rf "$work"
mkdir -p "$work" "$release_out"

# The tree, as it lands under /usr.
install -Dm755 "$binaries/snappy" "$tree/usr/bin/snappy"
install -Dm755 "$binaries/snappy-app" "$tree/usr/bin/snappy-app"
install -Dm644 packaging/linux/io.github.farhadzaidi.Snappy.desktop \
    "$tree/usr/share/applications/io.github.farhadzaidi.Snappy.desktop"
install -Dm644 assets/icon/snappy.svg "$tree/usr/share/icons/hicolor/scalable/apps/io.github.farhadzaidi.Snappy.svg"
install -Dm644 assets/icon/snappy.png "$tree/usr/share/icons/hicolor/256x256/apps/io.github.farhadzaidi.Snappy.png"
install -Dm644 LICENSE.md "$tree/usr/share/licenses/snappy-app/LICENSE.md"
find "$tree" -type d -exec chmod 755 {} +

# The tarball: the tree under one top folder.
tar --create --gzip --file "$release_out/$name.tar.gz" --directory "$work" \
    --sort=name --owner=0 --group=0 --numeric-owner "$name"

# The deb: the tree plus a control file. xz rather than dpkg's zstd default, which older
# Debian releases can't unpack.
deb="$work/deb"
mkdir -p "$deb"
cp -a "$tree/usr" "$deb/"
mkdir -m755 "$deb/DEBIAN"
sed -e "s/@VERSION@/$version/" -e "s/@ARCH@/$deb_arch/" \
    -e "s/@INSTALLED_SIZE@/$(du -sk "$deb/usr" | cut -f1)/" packaging/linux/control >"$deb/DEBIAN/control"
(cd "$deb" && find usr -type f -exec md5sum {} + | sort -k2) >"$deb/DEBIAN/md5sums"
chmod 644 "$deb/DEBIAN/control" "$deb/DEBIAN/md5sums"
dpkg-deb --root-owner-group -Zxz --build "$deb" "$release_out/snappy-app_${version}_$deb_arch.deb"

# The rpm, from the same tree.
rpmbuild -bb packaging/linux/snappy-app.spec \
    --target "$arch" \
    --define "_topdir $work/rpm" \
    --define "_rpmdir $release_out" \
    --define "_rpmfilename %%{NAME}-%%{VERSION}-%%{RELEASE}.%%{ARCH}.rpm" \
    --define "snappy_version $version" \
    --define "snappy_license $license" \
    --define "snappy_tree $tree"

echo "Built in $release_out:"
ls -l "$release_out"
