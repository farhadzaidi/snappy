#!/bin/bash
# Run the live scenarios in the macOS test VM, never on this Mac's desktop.
#
#   tools/live/macos/run.sh [scenario name prefix]
#
# Builds the signed app, the runner, and the fixture app; copies them into the Tart VM
# `snappy-macos`; grants the app Accessibility there; runs the scenarios; and copies the
# results and screenshots back to target/live/macos/out.
set -euo pipefail

root="$(cd "$(dirname "$0")/../../.." && pwd)"
vm="${SNAPPY_MACOS_VM:-snappy-macos}"
tart="${TART:-$(command -v tart || echo "$HOME/.local/bin/tart")}"
stage="$root/target/live/macos"
only="${1:-}"

cd "$root"
tools/macos/bundle.sh >/dev/null
cargo build --release -p snappy-live

rm -rf "$stage"
mkdir -p "$stage/payload" "$stage/out"
cp -R dist/Snappy.app "$stage/payload/"
cp target/release/snappy-live target/release/snappy-fixture "$stage/payload/"

# The VM runs headless. Start it when it isn't running.
running="$("$tart" list --format json | python3 -c 'import json, sys; print(any(v["Name"] == sys.argv[1] and v["Running"] for v in json.load(sys.stdin)))' "$vm")"
if [[ "$running" != True ]]; then
    "$tart" run --no-graphics "$vm" >"$stage/vm.log" 2>&1 &
    "$tart" ip "$vm" --wait 180 >/dev/null
    sleep 20
fi

in_vm() { "$tart" exec "$vm" /bin/sh -c "$1"; }

tar -C "$stage/payload" -cf - . | "$tart" exec -i "$vm" /bin/sh -c 'rm -rf ~/live && mkdir -p ~/live && tar -C ~/live -xf -'

# Grant Accessibility to this exact build, matched by its code signature.
codesign -d -r- dist/Snappy.app 2>&1 | sed -n 's/^designated => //p' >"$stage/requirement.txt"
csreq -r "$stage/requirement.txt" -b "$stage/requirement.bin"
csreq="$(xxd -p "$stage/requirement.bin" | tr -d '\n')"
in_vm "sudo sqlite3 '/Library/Application Support/com.apple.TCC/TCC.db' \"INSERT OR REPLACE INTO access (service, client, client_type, auth_value, auth_reason, auth_version, csreq, flags) VALUES ('kTCCServiceAccessibility', 'local.snappy', 0, 2, 4, 1, X'$csreq', 0);\" && sudo killall tccd 2>/dev/null; true"

# System prompts left on screen would cover the test windows and catch their clicks.
in_vm "killall UserNotificationCenter 2>/dev/null; true"

status=0
in_vm "~/live/snappy-live --app ~/live/Snappy.app --fixture ~/live/snappy-fixture --out ~/live/out ${only:+--only $only}" || status=$?

in_vm 'cd ~/live/out && tar -cf - results.json $(ls *.png home/logs/*.json* 2>/dev/null)' | tar -C "$stage/out" -xf - || true
echo "Results: $stage/out/results.json"
exit "$status"
