#!/bin/bash
# Inside a running GNOME test session (gnome-session.sh): install Snappy's extension the way
# a first launch does, log out and back in by restarting GNOME Shell, check the extension
# runs, then run the live scenarios with Wayland windows and again with Xwayland ones.
#
#   suite.sh <binaries folder> <output folder> [scenario name prefix]
set -uo pipefail

bin="$1"
out="$2"
only="${3:-}"
uuid="snappy@farhadzaidi.github.io"
extension_info() {
    gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell \
        --method org.gnome.Shell.Extensions.GetExtensionInfo "$uuid"
}
fail() {
    echo "FAIL $*" | tee -a "$out/install.txt"
    cp "$SNAPPY_GNOME_LOGS/gnome-shell.log" "$out/" 2>/dev/null
    exit 1
}

mkdir -p "$out/install-home"
{
    echo "GNOME Shell $(gnome-shell --version)"
    echo "Before the first launch: $(extension_info)"
} >"$out/install.txt"

SNAPPY_HOME="$out/install-home" SNAPPY_DEBUG=1 "$bin/snappy" --background >"$out/install-home/daemon.log" 2>&1 &
daemon=$!
for _ in $(seq 100); do
    gsettings get org.gnome.shell enabled-extensions | grep -q "$uuid" && break
    sleep 0.1
done
gsettings get org.gnome.shell enabled-extensions | grep -q "$uuid" \
    || fail "the first launch didn't turn the extension on"
[[ -f "$HOME/.local/share/gnome-shell/extensions/$uuid/metadata.json" ]] \
    || fail "the first launch didn't install the extension"
kill -TERM "$daemon"
wait "$daemon"
echo "After the first launch: installed and turned on; $(extension_info)" >>"$out/install.txt"

restart-shell || fail "GNOME Shell didn't start again"
info="$(extension_info)"
echo "After logging in again: $info" >>"$out/install.txt"
grep -q "'state': <1.0>" <<<"$info" || fail "the extension isn't running"
version="$(gdbus call --session --dest org.gnome.Shell --object-path /io/github/farhadzaidi/Snappy/Shell \
    --method org.freedesktop.DBus.Properties.Get io.github.farhadzaidi.Snappy.Shell Version)"
echo "Protocol: $version" >>"$out/install.txt"
grep -q "uint32 1" <<<"$version" || fail "the extension speaks another protocol version"

# GNOME's session manager would hand apps Xwayland's display and authorization; here they
# come from the running GNOME Shell, which made new ones when it started again.
xwayland_environment() {
    local line
    line="$(grep -o 'Using public X11 display :[0-9]*' "$SNAPPY_GNOME_LOGS/gnome-shell.log" | tail -1)"
    export DISPLAY="${line##* }"
    XAUTHORITY="$(ls -t "$XDG_RUNTIME_DIR"/.mutter-Xwaylandauth.* 2>/dev/null | head -1)"
    export XAUTHORITY
}

status=0
for display in wayland x11; do
    [[ "$display" == x11 ]] && xwayland_environment
    name="$display"
    [[ "$display" == x11 ]] && name=xwayland
    echo "== $name windows"
    SNAPPY_LIVE_FIXTURE_DISPLAY="$display" "$bin/snappy-live" --app "$bin/snappy" --fixture "$bin/snappy-fixture" \
        --out "$out/$name" ${only:+--only "$only"} || status=1
done
cp "$SNAPPY_GNOME_LOGS/gnome-shell.log" "$out/"
exit "$status"
