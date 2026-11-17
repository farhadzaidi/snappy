#!/bin/bash
# In the headless GNOME session, as its user: start Snappy, open three apps, snap each into
# the default layout with Snappy + Space, then Shift-drag one and capture the zones.
#
#   scene.sh <snappy executable> <folder for the screenshots>
#
# Input and screenshots go through the live suite's test extension (tools/live/gnome), so
# GNOME draws everything, translucent overlay included, as it would on a real desktop.
set -euo pipefail

snappy="$1"
out="$2"

# Evdev key codes, as the test extension's virtual keyboard takes them.
KEY_LEFTCTRL=29
KEY_LEFTALT=56
KEY_LEFTMETA=125
KEY_LEFTSHIFT=42
KEY_SPACE=57

# Where the apps land: one 1920×1080 monitor, GNOME 49's top bar, and Snappy's default
# layout, Center stage. The terminal snaps into the left zone, so its title sits here.
TERMINAL_TITLE=(244 66)
# A point in the middle zone, near its top, so the dragged window stays on screen.
DRAG_TARGET=(960 120)
DRAG_STEPS=24

# Seconds: for Snappy to start, for an app's window to appear, and for a window to take its
# size and focus, or a snap to finish.
SNAPPY_START=4
APP_TIMEOUT=30
SETTLE=1.5

helper() {
    gdbus call --session --dest org.gnome.Shell --object-path /io/github/farhadzaidi/SnappyTest \
        --method io.github.farhadzaidi.SnappyTest.Call "$1"
}
key() { helper "{\"op\":\"key\",\"keycode\":$1,\"pressed\":$2}" >/dev/null; }
pointer() { helper "{\"op\":\"pointerMove\",\"x\":$1,\"y\":$2}" >/dev/null; }
button() { helper "{\"op\":\"button\",\"pressed\":$1}" >/dev/null; }
screenshot() { helper "{\"op\":\"screenshot\",\"path\":\"$out/$1.png\"}" >/dev/null; }
# How many windows are open: the helper lists their titles, each in a pair of quotes.
window_count() {
    local quotes
    quotes="$(helper '{"op":"tabList"}' | tr -cd '"' | wc -c)"
    echo $((quotes / 2))
}

# The Snappy key, Ctrl+Alt+Super, with one more key.
snappy_chord() {
    local modifier
    for modifier in $KEY_LEFTCTRL $KEY_LEFTALT $KEY_LEFTMETA; do key "$modifier" true; done
    key "$1" true
    key "$1" false
    for modifier in $KEY_LEFTMETA $KEY_LEFTALT $KEY_LEFTCTRL; do key "$modifier" false; done
}

# Opens an app, waits for its window, and snaps it into the nearest empty zone.
open_and_snap() {
    local before
    before="$(window_count)"
    "$@" >/dev/null 2>&1 &
    for _ in $(seq $((APP_TIMEOUT * 10))); do
        (($(window_count) > before)) && break
        sleep 0.1
    done
    sleep "$SETTLE"
    snappy_chord $KEY_SPACE
    sleep "$SETTLE"
}

# A dark desktop with a quiet vertical gradient, and nothing that opens the overview.
gsettings set org.gnome.desktop.interface color-scheme prefer-dark
gsettings set org.gnome.desktop.interface enable-hot-corners false
gsettings set org.gnome.mutter overlay-key ''
gsettings set org.gnome.desktop.background picture-options none
gsettings set org.gnome.desktop.background primary-color '#2b3a52'
gsettings set org.gnome.desktop.background secondary-color '#10151f'
gsettings set org.gnome.desktop.background color-shading-type vertical

# Something to show in each app.
mkdir -p ~/Documents/Photos ~/Documents/Receipts ~/Projects ~/Music ~/Pictures
touch ~/Documents/Budget.ods ~/Documents/Slides.odp ~/Documents/Itinerary.pdf
cat >~/Documents/Notes.md <<'NOTES'
# This week

## To do

- [x] Book the train for Friday
- [ ] Send the slides to Maya
- [ ] Renew the library books
- [ ] Pick up the bike from the shop

## Groceries

- Oranges
- Rice
- Coffee beans
- Basil

## Ideas

A shelf above the desk for the plants, and a lamp that doesn't glare on the screen.
NOTES

SNAPPY_HOME="$HOME/.local/share/snappy-screenshots" "$snappy" >"$out/snappy.log" 2>&1 &
sleep "$SNAPPY_START"

# The terminal's title starts with the user's name.
USER="$(id -un)"
export USER

# Each app opens in the middle and goes to the nearest empty zone: Files, which won't get
# narrower than about 900 pixels in this folder, to the wide middle zone, then the terminal
# to the left, and the editor to the right.
open_and_snap nautilus ~/Documents
open_and_snap kgx --working-directory="$HOME/Projects"
open_and_snap gnome-text-editor ~/Documents/Notes.md
screenshot snapped

pointer "${TERMINAL_TITLE[@]}"
sleep 0.2
key $KEY_LEFTSHIFT true
button true
sleep 0.1
for step in $(seq "$DRAG_STEPS"); do
    pointer $((TERMINAL_TITLE[0] + (DRAG_TARGET[0] - TERMINAL_TITLE[0]) * step / DRAG_STEPS)) \
        $((TERMINAL_TITLE[1] + (DRAG_TARGET[1] - TERMINAL_TITLE[1]) * step / DRAG_STEPS))
    sleep 0.03
done
sleep "$SETTLE"
screenshot dragging

# Letting go of Shift first cancels the snap.
key $KEY_LEFTSHIFT false
button false
