#!/bin/bash
# Build the Windows adapter's tests on this Mac and run them on the Windows test VM's
# desktop: the unit tests and the integration tests, which only run there.
#
#   tools/live/windows/test.sh [test name filter]
#
# Cross-compiles with cargo-xwin for aarch64-pc-windows-msvc, copies the test programs into
# the VM, and runs them one test at a time, since they share one screen and keyboard.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
filter="${1:-}"
target=aarch64-pc-windows-msvc

cd "$root"
programs="$(cargo xwin test -p snappy-windows --target "$target" --no-run --message-format json 2>/dev/null |
    python3 -I -c '
import json, sys
for line in sys.stdin:
    message = json.loads(line)
    if message.get("reason") == "compiler-artifact" and message.get("executable") and message["profile"]["test"]:
        print(message["executable"])
')"
[[ -n "$programs" ]] || { echo "The tests didn't build." >&2; exit 1; }

"$here/ssh.sh" "New-Item -ItemType Directory -Force -Path C:\\Users\\snappy\\live | Out-Null"
commands=()
while IFS= read -r program; do
    name="$(basename "$program")"
    "$here/copy.sh" "$program" "vm:live/$name"
    commands+=("$name --test-threads=1 $filter")
done <<<"$programs"

status=0
for command in "${commands[@]}"; do
    echo "== $command"
    "$here/desktop.sh" "$command" || status=$?
done
exit "$status"
