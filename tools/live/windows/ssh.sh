#!/bin/bash
# Run a command in the Windows test VM as `snappy`, or open a shell with no arguments.
# The remote shell is Windows PowerShell, and login uses the VM's own key, so nothing prompts.
#
#   tools/live/windows/ssh.sh "Get-ComputerInfo | Select OsName, OsArchitecture"
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
source "$here/common.sh"

options=()
while IFS= read -r line; do options+=("$line"); done < <(ssh_options)

if [[ $# -eq 0 ]]; then
    exec ssh -t "${options[@]}" snappy@127.0.0.1
fi
exec ssh "${options[@]}" snappy@127.0.0.1 "$@"
