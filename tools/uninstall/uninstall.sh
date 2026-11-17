#!/bin/bash
# Removes Snappy from a Mac or a Linux PC, however it was installed. Published with each
# release, so people can run:
#
#   /bin/bash -c "$(curl -fsSL https://github.com/farhadzaidi/snappy/releases/latest/download/uninstall.sh)"
#
# Options, after `_` in that command (`... uninstall.sh)" _ --keep-settings`):
#   --keep-settings   keep Snappy's settings file, for installing it again later
#   --yes             answer yes to every question
#
# First Snappy undoes its own changes with `snappy --uninstall`: it quits so hidden windows
# come back, turns off Open at login, gives Caps Lock back, removes its GNOME extension and
# KDE settings, and deletes its files. Then this removes the program the way it was
# installed: Homebrew, the disk image, a .deb or .rpm package, or a copy of the .tar.gz.
# Copies from the Mac App Store are removed in Launchpad. It asks once before changing
# anything, and again before anything that needs sudo.
set -uo pipefail

keep_settings=false
assume_yes=false
for argument in "$@"; do
    case "$argument" in
        --keep-settings) keep_settings=true ;;
        --yes) assume_yes=true ;;
        -h | --help)
            sed -n '2,16p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "$argument isn't an option. Use --keep-settings, --yes, or neither."
            exit 2
            ;;
    esac
done

# What the packages and the tarball install, under /usr, ~/.local, or /usr/local, as
# tools/release/linux.sh lays them out.
package=snappy-app
tarball_files=(
    bin/snappy
    bin/snappy-app
    share/applications/io.github.farhadzaidi.Snappy.desktop
    share/icons/hicolor/scalable/apps/io.github.farhadzaidi.Snappy.svg
    share/icons/hicolor/256x256/apps/io.github.farhadzaidi.Snappy.png
    share/licenses/snappy-app/LICENSE.md
)
bundle_id=io.github.farhadzaidi.Snappy
cask=snappy

say() {
    printf '%s\n' "$*"
}

# Asks a yes-or-no question on the terminal, even when this script came through a pipe.
ask() {
    if $assume_yes; then
        return 0
    fi
    local answer
    if ! { printf '%s [y/N] ' "$1" >/dev/tty && read -r answer </dev/tty; } 2>/dev/null; then
        say "There's no terminal to ask in. Run this in a terminal, or add --yes."
        exit 1
    fi
    [[ "$answer" == [yY]* ]]
}

# Asks once, before anything changes, whether to go ahead at all.
confirm() {
    local question="Remove Snappy and its settings from this computer?"
    $keep_settings && question="Remove Snappy from this computer, keeping its settings?"
    if ! ask "$question"; then
        say "Nothing was changed."
        exit 1
    fi
}

# Runs Snappy's own cleanup with the program at $1. Returns its exit status.
clean_up() {
    local arguments=(--uninstall)
    $keep_settings && arguments+=(--keep-settings)
    "$1" "${arguments[@]}"
}

# After Snappy's own cleanup didn't finish: whether to remove the program anyway.
go_on_anyway() {
    case "$1" in
        0) return 0 ;;
        2) say "This copy of Snappy is too old to undo its own changes." ;;
    esac
    ask "Remove Snappy's program anyway?"
}

on_mac() {
    local app="" folder
    for folder in /Applications "$HOME/Applications"; do
        if [[ -x "$folder/Snappy.app/Contents/MacOS/snappy" ]] &&
            [[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$folder/Snappy.app/Contents/Info.plist" 2>/dev/null)" == "$bundle_id" ]]; then
            app="$folder/Snappy.app"
            break
        fi
    done
    local homebrew=false
    if command -v brew >/dev/null 2>&1 && brew list --cask "$cask" >/dev/null 2>&1; then
        homebrew=true
    fi
    if [[ -z "$app" ]] && ! $homebrew; then
        say "Snappy isn't installed in Applications."
        leftovers "$HOME/Library/Application Support/Snappy" "$HOME/Library/Logs/Snappy"
        return
    fi

    confirm
    local status=0
    if [[ -n "$app" ]]; then
        say "Removing Snappy's changes and files…"
        clean_up "$app/Contents/MacOS/snappy" || status=$?
        go_on_anyway "$status" || return 1
    fi

    if $homebrew; then
        say "Removing Snappy with Homebrew…"
        # --zap also trashes the settings, as Snappy's cleanup did unless asked to keep them.
        if $keep_settings; then
            brew uninstall --cask "$cask" || return 1
        else
            brew uninstall --cask --zap "$cask" || return 1
        fi
    elif [[ -d "$app/Contents/_MASReceipt" ]]; then
        say "Snappy came from the Mac App Store. To finish, delete it in Launchpad, or drag it from Applications to the Trash."
    else
        if [[ -w "$(dirname "$app")" ]]; then
            rm -rf -- "$app" || return 1
        else
            ask "Deleting $app needs your password for sudo. Go on?" || return 1
            sudo rm -rf -- "$app" || return 1
        fi
        say "Deleted $app."
    fi

    # The Accessibility permission stays listed after the app is gone.
    if ! tccutil reset Accessibility "$bundle_id" >/dev/null 2>&1; then
        say "Remove Snappy from System Settings > Privacy & Security > Accessibility if it's still listed."
    fi
}

on_linux() {
    if [[ "$(id -u)" == 0 ]]; then
        say "Run this as yourself, without sudo. It asks before anything that needs sudo."
        exit 1
    fi
    local deb=false rpm=false
    if command -v dpkg-query >/dev/null 2>&1 &&
        [[ "$(dpkg-query -W -f='${Status}' "$package" 2>/dev/null)" == "install ok installed" ]]; then
        deb=true
    fi
    if ! $deb && command -v rpm >/dev/null 2>&1 && rpm -q "$package" >/dev/null 2>&1; then
        rpm=true
    fi
    local copies=() prefix
    for prefix in "$HOME/.local" /usr/local; do
        [[ -x "$prefix/bin/snappy" && -x "$prefix/bin/snappy-app" ]] && copies+=("$prefix")
    done
    local program=""
    if $deb || $rpm; then
        program=/usr/bin/snappy
    elif [[ ${#copies[@]} -gt 0 ]]; then
        program="${copies[0]}/bin/snappy"
    fi
    if [[ -z "$program" ]]; then
        say "Snappy isn't installed."
        leftovers "${XDG_CONFIG_HOME:-$HOME/.config}/snappy" "${XDG_STATE_HOME:-$HOME/.local/state}/snappy"
        return
    fi

    confirm
    say "Removing Snappy's changes and files…"
    local status=0
    clean_up "$program" || status=$?
    go_on_anyway "$status" || return 1

    if $deb; then
        say "Snappy was installed from a .deb package."
        ask "Remove the $package package? It needs your password for sudo." || return 1
        if command -v apt-get >/dev/null 2>&1; then
            sudo apt-get remove -y "$package" || return 1
        else
            sudo dpkg --remove "$package" || return 1
        fi
    elif $rpm; then
        say "Snappy was installed from an .rpm package."
        ask "Remove the $package package? It needs your password for sudo." || return 1
        if command -v dnf >/dev/null 2>&1; then
            sudo dnf remove -y "$package" || return 1
        elif command -v zypper >/dev/null 2>&1; then
            sudo zypper --non-interactive remove "$package" || return 1
        else
            sudo rpm -e "$package" || return 1
        fi
    fi
    for prefix in ${copies[@]+"${copies[@]}"}; do
        remove_copy "$prefix" || return 1
    done
}

# Removes a copy of the tarball's files from the prefix $1, file by file, and nothing else.
remove_copy() {
    local prefix="$1" run=() file
    if [[ ! -w "$prefix/bin" ]]; then
        ask "Deleting the copy of Snappy in $prefix needs your password for sudo. Go on?" || return 1
        run=(sudo)
    fi
    for file in "${tarball_files[@]}"; do
        if [[ -e "$prefix/$file" ]]; then
            ${run[@]+"${run[@]}"} rm -f -- "$prefix/$file" || return 1
        fi
    done
    ${run[@]+"${run[@]}"} rmdir -- "$prefix/share/licenses/snappy-app" 2>/dev/null
    say "Deleted the copy of Snappy in $prefix."
}

# Without Snappy's program to run, offers to delete the folders it keeps its files in.
leftovers() {
    local found=() folder
    for folder in "$@"; do
        [[ -e "$folder" ]] && found+=("$folder")
    done
    if [[ ${#found[@]} -eq 0 ]] || $keep_settings; then
        return
    fi
    say "Snappy's files are still here:"
    printf '  %s\n' "${found[@]}"
    if ask "Delete them?"; then
        rm -rf -- "${found[@]}" && say "Deleted them."
    fi
}

case "$(uname -s)" in
    Darwin) on_mac ;;
    Linux) on_linux ;;
    *)
        say "This removes Snappy from macOS and Linux. On Windows, use uninstall.ps1."
        exit 1
        ;;
esac
status=$?
if [[ $status -eq 0 ]]; then
    say "Done."
else
    say "Snappy isn't completely removed."
fi
exit "$status"
