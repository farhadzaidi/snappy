#!/bin/bash
# Inside the Linux development container: start a headless X11 desktop, then run a command
# against it. The desktop is Xorg on the dummy driver with two side-by-side 1920×1080
# RandR outputs, an evdev US keymap, the Xfwm4 window manager with its compositor on, and
# a D-Bus session bus. With no command it runs the snappy-x11 suite, integration included.
#
#   tools/live/linux/run.sh tools/live/linux/x11-session.sh
#   tools/live/linux/run.sh tools/live/linux/x11-session.sh xprop -root
#
# Environment: SNAPPY_X11_COMPOSITOR=off runs Xfwm4 without its compositor.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
display=":99"
compositor="${SNAPPY_X11_COMPOSITOR:-on}"

if [[ $# -eq 0 ]]; then
    set -- cargo test -p snappy-x11 -- --test-threads=1
fi

log_dir="$(mktemp -d)"
cleanup() {
    kill "${wm_pid:-}" "${xorg_pid:-}" "${DBUS_SESSION_BUS_PID:-}" 2>/dev/null || true
}
trap cleanup EXIT

wait_for() {
    for _ in $(seq 200); do
        "$@" >/dev/null 2>&1 && return 0
        sleep 0.05
    done
    return 1
}

Xorg "$display" -config "$here/xorg-dummy.conf" -noreset -nolisten tcp \
    -logfile "$log_dir/xorg.log" >/dev/null 2>&1 &
xorg_pid=$!
export DISPLAY="$display"
wait_for xdpyinfo || { tail -20 "$log_dir/xorg.log"; exit 1; }

xrandr --addmode DUMMY1 1920x1080
xrandr --output DUMMY0 --mode 1920x1080 --pos 0x0 --primary \
    --output DUMMY1 --mode 1920x1080 --pos 1920x0
setxkbmap -rules evdev -model pc105 -layout us

eval "$(dbus-launch --sh-syntax)"

xfwm4 --compositor="$compositor" --replace >"$log_dir/xfwm4.log" 2>&1 &
wm_pid=$!
wait_for sh -c 'xprop -root _NET_SUPPORTING_WM_CHECK | grep -q "window id"' \
    || { cat "$log_dir/xfwm4.log"; exit 1; }

# The dummy driver reports its second output connected only after a full RandR probe,
# which `xrandr` performs; the window manager then rereads the monitors.
xrandr >/dev/null
sleep 0.5

export SNAPPY_X11_TEST_DISPLAY="$display"
"$@"
