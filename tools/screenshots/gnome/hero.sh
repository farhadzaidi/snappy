#!/bin/bash
# In the headless GNOME session, as its user: open two editors and a terminal on a
# copy of Snappy's own repository, leave them overlapping, then Shift-drag each into a zone
# of the Main + stack layout, saving a frame at each step. animate.sh turns the frames into
# the README's animation.
#
#   hero.sh <snappy executable> <repository bundle> <folder for the frames>
#
# Writes frame-NNN.png and frames.tsv: each frame's file, the pointer's position, and how
# long the frame shows, in milliseconds.
set -euo pipefail

snappy="$1"
bundle="$2"
out="$3"

# Evdev key codes, as the test extension's virtual keyboard takes them.
KEY_LEFTSHIFT=42
KEY_ENTER=28
declare -A KEYS=(
    [a]=30 [b]=48 [c]=46 [d]=32 [e]=18 [f]=33 [g]=34 [h]=35 [i]=23 [j]=36 [k]=37 [l]=38
    [m]=50 [n]=49 [o]=24 [p]=25 [q]=16 [r]=19 [s]=31 [t]=20 [u]=22 [v]=47 [w]=17 [x]=45
    [y]=21 [z]=44 [0]=11 [1]=2 [2]=3 [3]=4 [4]=5 [5]=6 [6]=7 [7]=8 [8]=9 [9]=10 [-]=12
    [ ]=57
)

# What the terminal shows.
TERMINAL_COMMAND="git log --oneline -14"
# The layout the windows go into, as the top bar menu names it.
LAYOUT="Main + stack"

SNAPPY_START=4
APP_TIMEOUT=30
SETTLE=1.5

# Milliseconds each kind of frame shows: before anything moves, the pointer reaching a
# window, the zones appearing, each step of a drag, the target zone lit before the drop,
# the snapped window, and the tidy desktop at the end.
SHOW_START=1000
SHOW_REACH=40
SHOW_ZONES=350
SHOW_DRAG=55
SHOW_TARGET=550
SHOW_SNAPPED=700
SHOW_END=2200
REACH_STEPS=6
DRAG_STEPS=12

# The height of a GNOME header bar, whose middle is the grip, and how far into the editor's
# header bar its empty part starts, past the Open and New Tab buttons.
HEADER_HEIGHT=44
MANIFEST_GRIP=150
# Where the pointer waits before and after the drags: below the windows, in the open.
REST_X=1000
REST_Y=1010

helper() {
    gdbus call --session --dest org.gnome.Shell --object-path /io/github/farhadzaidi/SnappyTest \
        --method io.github.farhadzaidi.SnappyTest.Call "$1"
}
# The helper's answer, without the D-Bus wrapping: `ok`'s value as JSON.
answer() {
    python3 -c 'import ast, json, sys; print(json.dumps(json.loads(ast.literal_eval(sys.stdin.read())[0])["ok"]))'
}
key() { helper "{\"op\":\"key\",\"keycode\":$1,\"pressed\":$2}" >/dev/null; }
move_pointer() { helper "{\"op\":\"pointerMove\",\"x\":$1,\"y\":$2}" >/dev/null; }
button() { helper "{\"op\":\"button\",\"pressed\":$1}" >/dev/null; }
titles() { helper '{"op":"tabList"}' | answer | python3 -c 'import json, sys; print("\n".join(json.load(sys.stdin)))'; }

# Opens an app and prints the title of its new window once it's settled.
open_app() {
    local before title
    before="$(titles)"
    "$@" >/dev/null 2>&1 &
    for _ in $(seq $((APP_TIMEOUT * 10))); do
        [[ "$(titles)" != "$before" ]] && break
        sleep 0.1
    done
    sleep "$SETTLE"
    title="$(comm -13 <(sort <<<"$before") <(titles | sort) | head -1)"
    echo "$title"
}

# Moves a window's top-left corner to $2,$3 and prints its frame as "x y width".
place() {
    helper "{\"op\":\"placeWindow\",\"title\":\"$1\",\"x\":$2,\"y\":$3}" | answer |
        python3 -c 'import json, sys; f = json.load(sys.stdin); print(f["x"], f["y"], f["width"])'
}

type_text() {
    local char i
    for ((i = 0; i < ${#1}; i++)); do
        char="${1:i:1}"
        key "${KEYS[$char]}" true
        key "${KEYS[$char]}" false
    done
}

frame=0
pointer_x="$REST_X"
pointer_y="$REST_Y"
record() {
    local name
    name="$(printf 'frame-%03d.png' "$frame")"
    helper "{\"op\":\"screenshot\",\"path\":\"$out/$name\"}" >/dev/null
    printf '%s\t%s\t%s\t%s\n' "$name" "$pointer_x" "$pointer_y" "$1" >>"$out/frames.tsv"
    frame=$((frame + 1))
}
# Moves the pointer to $1,$2 in $3 steps, recording each for $4 milliseconds.
glide() {
    local from_x="$pointer_x" from_y="$pointer_y" step
    for step in $(seq "$3"); do
        pointer_x=$((from_x + ($1 - from_x) * step / $3))
        pointer_y=$((from_y + ($2 - from_y) * step / $3))
        move_pointer "$pointer_x" "$pointer_y"
        sleep 0.03
        record "$4"
    done
}
# Shift-drags from the grip $1,$2 and drops at $3,$4.
drag() {
    glide "$1" "$2" "$REACH_STEPS" "$SHOW_REACH"
    key $KEY_LEFTSHIFT true
    button true
    sleep 0.3
    record "$SHOW_ZONES"
    glide "$3" "$4" "$DRAG_STEPS" "$SHOW_DRAG"
    sleep 0.2
    record "$SHOW_TARGET"
    button false
    key $KEY_LEFTSHIFT false
    sleep "$SETTLE"
    record "$SHOW_SNAPPED"
}

# A dark desktop with a quiet vertical gradient, and nothing that opens the overview.
gsettings set org.gnome.desktop.interface color-scheme prefer-dark
gsettings set org.gnome.desktop.interface enable-hot-corners false
gsettings set org.gnome.mutter overlay-key ''
gsettings set org.gnome.desktop.background picture-options none
gsettings set org.gnome.desktop.background primary-color '#2b3a52'
gsettings set org.gnome.desktop.background secondary-color '#10151f'
gsettings set org.gnome.desktop.background color-shading-type vertical

mkdir -p ~/Projects
git clone --quiet "$bundle" ~/Projects/snappy
# The terminal's title starts with the user's name.
USER="$(id -un)"
export USER

SNAPPY_HOME="$HOME/.local/share/snappy-screenshots" "$snappy" >"$out/snappy.log" 2>&1 &
sleep "$SNAPPY_START"
helper "{\"op\":\"activateMenuItem\",\"title\":\"$LAYOUT\"}" >/dev/null
sleep "$SETTLE"

# Opened in stacking order, so each one's grip stays in view: the manifest at the back, the
# terminal in front.
manifest="$(open_app gnome-text-editor --new-window ~/Projects/snappy/crates/core/Cargo.toml)"
editor="$(open_app gnome-text-editor --new-window ~/Projects/snappy/crates/core/src/layout.rs)"
open_app kgx --working-directory="$HOME/Projects/snappy" >/dev/null
type_text "$TERMINAL_COMMAND"
key $KEY_ENTER true
key $KEY_ENTER false
sleep "$SETTLE"
terminal="$(titles | grep '@' | head -1)"

read -r manifest_x manifest_y _ <<<"$(place "$manifest" 150 300)"
read -r editor_x editor_y editor_width <<<"$(place "$editor" 420 90)"
read -r terminal_x terminal_y terminal_width <<<"$(place "$terminal" 980 430)"
sleep "$SETTLE"

move_pointer "$pointer_x" "$pointer_y"
record "$SHOW_START"
# Each by its header bar, dropped near the top of its zone: the manifest left of its title,
# which the code editor in front covers, the others by their titles.
drag $((manifest_x + MANIFEST_GRIP)) $((manifest_y + HEADER_HEIGHT / 2)) 1600 640
drag $((editor_x + editor_width / 2)) $((editor_y + HEADER_HEIGHT / 2)) 640 110
drag $((terminal_x + terminal_width / 2)) $((terminal_y + HEADER_HEIGHT / 2)) 1600 110
glide "$REST_X" "$REST_Y" "$REACH_STEPS" "$SHOW_REACH"
record "$SHOW_END"
