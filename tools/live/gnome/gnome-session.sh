#!/bin/bash
# Inside the GNOME test container, as root: start a headless GNOME Shell on Wayland as the
# user "snappy", in a D-Bus session of its own, then run a command as that user against it.
# Nothing here reaches the machine running the container.
#
#   gnome-session.sh <command> [args...]
#
# The shell has two side-by-side 1920×1080 virtual monitors, Xwayland, and the test helper
# extension (test-extension/), which injects input and reports windows for the live tests.
# Inside the command, `restart-shell` stops GNOME Shell and starts it again in the same
# session, as logging out and back in would, so newly installed extensions load.
#
# Environment: SNAPPY_GNOME_DYNAMIC_WORKSPACES=false gives GNOME a fixed number of
# workspaces; SNAPPY_GNOME_MONITORS=1 runs one monitor; SNAPPY_GNOME_PREINSTALL=1 loads
# Snappy's extension from the repository at login.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
user=snappy

if [[ "$(id -u)" == 0 ]]; then
    mkdir -p /tmp/.X11-unix /run/dbus
    chmod 1777 /tmp/.X11-unix
    # GNOME Shell takes /run/systemd/seats as a sign that logind runs, which it doesn't here.
    rm -rf /run/systemd/seats
    dbus-daemon --system --fork
    exec su "$user" --preserve-environment --shell /bin/bash -c "$(printf '%q ' "$0" "$@")"
fi

export HOME="/home/$user"
export XDG_RUNTIME_DIR="/tmp/xdg-$user"
export XDG_SESSION_TYPE=wayland XDG_CURRENT_DESKTOP=GNOME XDG_SESSION_DESKTOP=gnome
export GSETTINGS_BACKEND=dconf
mkdir -p -m 700 "$XDG_RUNTIME_DIR"
cd "$HOME"

if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" || "${SNAPPY_GNOME_IN_SESSION:-}" != 1 ]]; then
    export SNAPPY_GNOME_IN_SESSION=1
    exec dbus-run-session -- "$0" "$@"
fi

logs="$HOME/gnome-logs"
mkdir -p "$logs"

monitors=(--virtual-monitor 1920x1080)
if [[ "${SNAPPY_GNOME_MONITORS:-2}" != 1 ]]; then
    monitors+=(--virtual-monitor 1920x1080)
fi

shell_pid_file="$XDG_RUNTIME_DIR/gnome-shell.pid"

wait_for_shell() {
    for _ in $(seq 300); do
        if gdbus call --session --dest org.gnome.Shell --object-path /io/github/farhadzaidi/SnappyTest \
            --method io.github.farhadzaidi.SnappyTest.Call '{"op":"ready"}' 2>/dev/null | grep -q true; then
            return 0
        fi
        sleep 0.1
    done
    echo "GNOME Shell didn't start; its log:" >&2
    tail -40 "$logs/gnome-shell.log" >&2
    return 1
}

start_shell() {
    gnome-shell --headless --wayland "${monitors[@]}" >>"$logs/gnome-shell.log" 2>&1 &
    echo $! >"$shell_pid_file"
    wait_for_shell
}

stop_shell() {
    local pid
    pid="$(cat "$shell_pid_file")"
    kill "$pid" 2>/dev/null || true
    for _ in $(seq 100); do
        kill -0 "$pid" 2>/dev/null || return 0
        sleep 0.1
    done
    kill -KILL "$pid" 2>/dev/null || true
}

if [[ "${1:-}" == --restart-shell ]]; then
    stop_shell
    start_shell
    exit
fi

test_uuid="snappy-test@farhadzaidi.github.io"
test_folder="$HOME/.local/share/gnome-shell/extensions/$test_uuid"
mkdir -p "$test_folder"
cp "$here/test-extension/"* "$test_folder/"

enabled="'$test_uuid'"
# The adapter's integration tests start with Snappy's extension already running; the live
# suite installs it the way a first launch does.
if [[ "${SNAPPY_GNOME_PREINSTALL:-}" == 1 ]]; then
    snappy_folder="$HOME/.local/share/gnome-shell/extensions/snappy@farhadzaidi.github.io"
    mkdir -p "$snappy_folder"
    cp -r "$here/../../../apps/gnome-extension/." "$snappy_folder/"
    enabled+=", 'snappy@farhadzaidi.github.io'"
fi
gsettings set org.gnome.shell enabled-extensions "[$enabled]"
gsettings set org.gnome.mutter dynamic-workspaces "${SNAPPY_GNOME_DYNAMIC_WORKSPACES:-true}"
gsettings set org.gnome.mutter workspaces-only-on-primary false
# Animations make every move take longer than the tests allow for; the welcome tour and
# the overview at login would cover the windows.
gsettings set org.gnome.desktop.interface enable-animations false
gsettings set org.gnome.shell welcome-dialog-last-shown-version '999'

trap stop_shell EXIT
start_shell

# The command runs with `restart-shell` on its path.
bin="$XDG_RUNTIME_DIR/bin"
mkdir -p "$bin"
printf '#!/bin/bash\nexec %q --restart-shell\n' "$0" >"$bin/restart-shell"
chmod +x "$bin/restart-shell"
export PATH="$bin:$PATH"
export SNAPPY_GNOME_LOGS="$logs"
# Clients find the compositor here; Xwayland starts on demand for X11 clients.
export WAYLAND_DISPLAY=wayland-0 DISPLAY=:0
# For `docker exec` into a running session.
export -p >"$XDG_RUNTIME_DIR/session.env"

"$@"
