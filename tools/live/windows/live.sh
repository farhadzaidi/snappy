#!/bin/bash
# Run the live scenarios against the Windows daemon in the Windows test VM, never on this
# Mac's desktop.
#
#   tools/live/windows/live.sh [scenario name prefix]
#
# Cross-compiles the daemon, the runner, and the fixture app with cargo-xwin; copies them
# into the VM; runs the scenarios on the VM's desktop; and copies the results,
# screenshots, the daemon's output, and its debug log to target/live/windows/out.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
only="${1:-}"
target=aarch64-pc-windows-msvc
results="$root/target/live/windows/out"
folder='C:\Users\snappy\live'

cd "$root"
cargo xwin build --release --target "$target" -p snappy -p snappy-live

"$here/ssh.sh" "New-Item -ItemType Directory -Force -Path '$folder' | Out-Null; Remove-Item -Recurse -ErrorAction Ignore '$folder\\out'; exit 0"
for program in snappy snappy-live snappy-fixture; do
    "$here/copy.sh" "target/$target/release/$program.exe" "vm:live/$program.exe"
done

status=0
"$here/desktop.sh" "snappy-live.exe --app $folder\\snappy.exe --fixture $folder\\snappy-fixture.exe --out $folder\\out${only:+ --only $only}" 900 ||
    status=$?

rm -rf "$results"
mkdir -p "$results"
"$here/copy.sh" -r "vm:live/out/." "$results" || true
echo "Results: $results/results.json"
exit "$status"
