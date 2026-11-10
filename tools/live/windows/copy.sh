#!/bin/bash
# Copy files into or out of the Windows test VM over SSH. A path starting with `vm:` names
# a path in the VM, relative to the home folder of `snappy` unless absolute.
#
#   tools/live/windows/copy.sh target/app.exe vm:live/      # into the VM
#   tools/live/windows/copy.sh -r vm:live/out target/out     # out of the VM
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
source "$here/common.sh"

options=()
while IFS= read -r line; do
    # scp names the port with -P where ssh uses -p.
    [[ "$line" == -p ]] && line=-P
    options+=("$line")
done < <(ssh_options)

args=()
for arg in "$@"; do
    case "$arg" in
        vm:*) args+=("snappy@127.0.0.1:${arg#vm:}") ;;
        *) args+=("$arg") ;;
    esac
done
exec scp -q "${options[@]}" "${args[@]}"
