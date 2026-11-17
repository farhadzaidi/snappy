#!/bin/bash
# Runs uninstall.sh against each way Snappy is installed on Linux, in throwaway containers,
# never on this machine: the .deb on Ubuntu, the .rpm on Fedora, and copies of the tarball in
# ~/.local and /usr/local. Each run checks the package or copy is gone, Snappy's files with it
# (or kept with --keep-settings), and that a second run finds nothing to remove.
#
#   tools/uninstall/test.sh
#
# The daemon is built in the live suites' build container. The packages hold it beside a
# stand-in for the app, which the script never runs, and are installed without their
# dependencies, which the script never needs. Output goes to target/live/uninstall/.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=tools/live/build/build.sh
source "$here/../live/build/build.sh"
root="$(cd "$here/../.." && pwd)"
out="$root/target/live/uninstall"
mkdir -p "$out"

echo "Building the daemon…"
build_in_container '
    cargo build -p snappy --bin snappy
    mkdir -p /work/target/live/uninstall/bin
    cp /target/debug/snappy /work/target/live/uninstall/bin/'

# Inside each container, as root: lays out the packages' tree from the repository at /work,
# adds a user who may use sudo, and runs the checks as that user.
read -r -d '' prepare <<'PREPARE' || true
set -euo pipefail
tree=/tmp/tree/usr
install -Dm755 /work/target/live/uninstall/bin/snappy "$tree/bin/snappy"
printf '#!/bin/sh\nexit 1\n' >/tmp/stand-in
install -Dm755 /tmp/stand-in "$tree/bin/snappy-app"
install -Dm644 /work/packaging/linux/io.github.farhadzaidi.Snappy.desktop \
    "$tree/share/applications/io.github.farhadzaidi.Snappy.desktop"
install -Dm644 /work/assets/icon/snappy.svg "$tree/share/icons/hicolor/scalable/apps/io.github.farhadzaidi.Snappy.svg"
install -Dm644 /work/assets/icon/snappy.png "$tree/share/icons/hicolor/256x256/apps/io.github.farhadzaidi.Snappy.png"
install -Dm644 /work/LICENSE.md "$tree/share/licenses/snappy-app/LICENSE.md"
useradd --create-home person
mkdir -p /etc/sudoers.d
echo 'person ALL=(ALL) NOPASSWD: ALL' >/etc/sudoers.d/person
PREPARE

# As the user: Snappy's files as a run leaves them, then the script, then the checks.
read -r -d '' check <<'CHECK' || true
set -uo pipefail
fail() { echo "FAIL $*"; exit 1; }
leave_files() {
    mkdir -p ~/.config/snappy ~/.local/state/snappy/logs ~/.config/autostart \
        ~/.local/share/gnome-shell/extensions/snappy@farhadzaidi.github.io \
        ~/.local/share/io.github.farhadzaidi.Snappy.App
    echo '{}' >~/.config/snappy/settings.json
    echo '{}' >~/.local/state/snappy/logs/drops.json
    printf '[Desktop Entry]\nExec="/usr/bin/snappy" --background\n' >~/.config/autostart/snappy.desktop
    echo '{}' >~/.local/share/gnome-shell/extensions/snappy@farhadzaidi.github.io/metadata.json
}
gone() {
    local path
    for path in "$@"; do [[ ! -e "$path" ]] || fail "$path is still there"; done
}
leave_files
echo "== without a terminal to ask in"
bash /work/tools/uninstall/uninstall.sh </dev/null && fail "it went on without asking"
[[ -e ~/.config/snappy/settings.json ]] || fail "it removed something without asking"

echo "== uninstalling, keeping the settings"
bash /work/tools/uninstall/uninstall.sh --yes --keep-settings || fail "it didn't finish"
[[ -e ~/.config/snappy/settings.json ]] || fail "the settings are gone"
gone ~/.local/state/snappy ~/.config/autostart/snappy.desktop \
    ~/.local/share/gnome-shell/extensions/snappy@farhadzaidi.github.io ~/.local/share/io.github.farhadzaidi.Snappy.App
installed && fail "Snappy is still installed"

echo "== again, with nothing installed"
said="$(bash /work/tools/uninstall/uninstall.sh --yes)" || fail "the second run failed: $said"
echo "$said"
grep -q "Snappy isn't installed." <<<"$said" || fail "the second run didn't say Snappy isn't installed"
gone ~/.config/snappy
echo "PASS"
CHECK

# Runs the checks in `image` for one way of installing: `install` installs it as root, and
# `installed`, as the user, says whether it's still there.
run() {
    local name="$1" image="$2" install="$3" installed="$4"
    echo "== $name, in $image"
    if docker run --rm --volume "$root:/work:ro" "$image" bash -c "
        $prepare
        $install
        su person --shell /bin/bash --command $(printf %q "installed() { $installed; }; $check")" >"$out/$name.log" 2>&1; then
        echo "pass $name"
    else
        echo "FAIL $name; see $out/$name.log"
        status=1
    fi
}

version="$(grep -m1 '^version' "$root/Cargo.toml" | cut -d'"' -f2)"
status=0

run deb ubuntu:24.04 "
    apt-get update -qq && apt-get install -y -qq sudo >/dev/null
    mkdir -p /tmp/deb/DEBIAN && cp -a /tmp/tree/usr /tmp/deb/
    sed -e 's/@VERSION@/$version/' -e 's/@ARCH@/amd64/' -e 's/@INSTALLED_SIZE@/1/' /work/packaging/linux/control >/tmp/deb/DEBIAN/control
    dpkg-deb --root-owner-group --build /tmp/deb /tmp/snappy-app.deb >/dev/null
    dpkg --force-depends -i /tmp/snappy-app.deb" \
    "dpkg-query -W snappy-app >/dev/null 2>&1 && [[ -e /usr/bin/snappy ]]"

run rpm fedora:45 "
    dnf install -y -q rpm-build sudo >/dev/null
    rpmbuild -bb /work/packaging/linux/snappy-app.spec --define '_topdir /tmp/rpm' \
        --define 'snappy_version $version' --define 'snappy_license MIT' --define 'snappy_tree /tmp/tree' >/dev/null
    rpm -i --nodeps --nosignature /tmp/rpm/RPMS/*/snappy-app-*.rpm" \
    "rpm -q snappy-app >/dev/null 2>&1"

run home-copy ubuntu:24.04 "
    apt-get update -qq && apt-get install -y -qq sudo >/dev/null
    mkdir -p /home/person/.local && cp -a /tmp/tree/usr/. /home/person/.local/ && chown -R person: /home/person/.local" \
    "[[ -e ~/.local/bin/snappy || -e ~/.local/share/applications/io.github.farhadzaidi.Snappy.desktop ]]"

run system-copy ubuntu:24.04 "
    apt-get update -qq && apt-get install -y -qq sudo >/dev/null
    cp -a /tmp/tree/usr/. /usr/local/" \
    "[[ -e /usr/local/bin/snappy || -e /usr/local/share/licenses/snappy-app ]]"

exit "$status"
