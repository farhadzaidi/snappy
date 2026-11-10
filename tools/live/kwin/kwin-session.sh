#!/bin/bash
# Start a headless KWin on Wayland, sealed off from the desktop this runs on, and run a
# command against it.
#
#   tools/live/kwin/kwin-session.sh <command> [args...]
#
# It uses the machine's own KWin, so the KWin under test is the one Snappy will meet, and
# nothing has to be built first. Bubblewrap gives it a private /tmp, /run, and /var, its own
# network namespace (so no abstract socket, including X11's, is shared), only /usr, /etc,
# and the repository (read-only), a minimal /dev without GPU nodes, and an empty
# environment. Inside, KWin gets a D-Bus session of its own, two side-by-side 1920×1080
# virtual outputs, Xwayland, and no animations. Without GPU nodes KWin runs without
# compositing: it manages, moves, and drags windows as usual and puts nothing on screen, so
# the tests read frames, never pixels. The desktop's own KWin, session bus, Wayland socket,
# and home folder can't be reached.
#
# Results go to target/live/kwin/out/ (writable inside), KWin's log to kwin.log there.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
out="$root/target/live/kwin/out"

if [[ "${SNAPPY_KWIN_SANDBOX:-}" != 1 ]]; then
    mkdir -p "$out"
    exec bwrap --die-with-parent --new-session \
        --unshare-user --unshare-pid --unshare-ipc --unshare-net --unshare-uts --unshare-cgroup-try \
        --ro-bind /usr /usr --ro-bind /etc /etc \
        --symlink usr/bin /bin --symlink usr/sbin /sbin --symlink usr/lib /lib --symlink usr/lib64 /lib64 \
        --proc /proc --dev /dev --tmpfs /tmp --tmpfs /run --tmpfs /var \
        --ro-bind "$root" "$root" --bind "$out" "$out" \
        --clearenv --setenv PATH /usr/bin:/usr/sbin --setenv LANG C.UTF-8 \
        --setenv HOME /tmp/home --setenv XDG_RUNTIME_DIR /tmp/xdg \
        --setenv SNAPPY_KWIN_SANDBOX 1 --setenv SNAPPY_KWIN_OUT "$out" \
        ${SNAPPY_KWIN_TEST:+--setenv SNAPPY_KWIN_TEST "$SNAPPY_KWIN_TEST"} \
        --chdir "$root" "$0" "$@"
fi

if [[ "${SNAPPY_KWIN_IN_SESSION:-}" != 1 ]]; then
    # Inside the sandbox, before its own bus: nothing of the desktop's may be visible.
    if [[ -e /run/user || -n "${DBUS_SESSION_BUS_ADDRESS:-}${WAYLAND_DISPLAY:-}${DISPLAY:-}" ]]; then
        echo "kwin-session.sh: not sealed off from the desktop; refusing to start KWin." >&2
        exit 2
    fi
    mkdir -p -m 700 "$XDG_RUNTIME_DIR"
    mkdir -p "$HOME/.config"
    mkdir -p -m 1777 /tmp/.X11-unix
    export SNAPPY_KWIN_IN_SESSION=1
    exec dbus-run-session -- "$0" "$@"
fi

export XDG_SESSION_TYPE=wayland XDG_CURRENT_DESKTOP=KDE XDG_SESSION_DESKTOP=KDE
log="$SNAPPY_KWIN_OUT/kwin.log"
: >"$log"

# Animations make every move take longer than the tests allow for.
cat >"$HOME/.config/kdeglobals" <<'CONFIG'
[KDE]
AnimationDurationFactor=0
CONFIG

socket=wayland-snappy
# KWin's messages, and the scripts', go to the log rather than the journal.
QT_FORCE_STDERR_LOGGING=1 \
    QT_LOGGING_RULES="js.debug=true;qml.debug=true;kwin_scripting.debug=true" \
    kwin_wayland --virtual --xwayland --width 1920 --height 1080 --output-count 2 --no-lockscreen \
    --socket "$socket" >>"$log" 2>&1 &
kwin=$!
trap 'kill "$kwin" 2>/dev/null || true; wait "$kwin" 2>/dev/null || true' EXIT

for _ in $(seq 100); do
    if [[ -S "$XDG_RUNTIME_DIR/$socket" ]] && dbus-send --session --print-reply --dest=org.kde.KWin /KWin \
        org.freedesktop.DBus.Peer.Ping >/dev/null 2>&1; then
        break
    fi
    sleep 0.1
done
if ! kill -0 "$kwin" 2>/dev/null; then
    echo "KWin didn't start; its log:" >&2
    tail -40 "$log" >&2
    exit 1
fi

# Clients find the compositor here.
export WAYLAND_DISPLAY="$socket" QT_QPA_PLATFORM=wayland
"$@"
