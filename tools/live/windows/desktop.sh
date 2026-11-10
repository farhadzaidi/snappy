#!/bin/bash
# Run a command on the Windows test VM's desktop and wait for it. Programs started over
# SSH run in session 0, which has no desktop, so this runs the command as a scheduled
# task restricted to the logged-on user, collects its output from a file, and exits with
# its exit code.
#
#   tools/live/windows/desktop.sh 'snappy-windows-tests.exe --test-threads=1' [timeout seconds]
#
# The command runs with cmd.exe in C:\Users\snappy\live, the folder other scripts copy
# builds into, with SNAPPY_WINDOWS_TEST=1 set for the integration tests.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
source "$here/common.sh"

command="$1"
timeout="${2:-600}"
folder='C:\Users\snappy\live'
task=snappy-desktop

script="$(mktemp)"
trap 'rm -f "$script"' EXIT
printf '@echo off\r\ncd /d %s\r\nset SNAPPY_WINDOWS_TEST=1\r\n%s > task.out 2>&1\r\necho %%ERRORLEVEL%% > task.done\r\n' \
    "$folder" "$command" >"$script"

vm_ssh "New-Item -ItemType Directory -Force -Path '$folder' | Out-Null; Remove-Item -ErrorAction Ignore '$folder\\task.out', '$folder\\task.done'; exit 0"
"$here/copy.sh" "$script" "vm:live/task.cmd"
vm_ssh "schtasks /create /tn $task /tr '$folder\\task.cmd' /sc once /st 23:59 /ru snappy /it /f | Out-Null; schtasks /run /tn $task | Out-Null"

deadline=$((SECONDS + timeout))
until vm_ssh "Test-Path '$folder\\task.done'" | grep -q True; do
    if ((SECONDS >= deadline)); then
        echo "The desktop task didn't finish within $timeout seconds." >&2
        vm_ssh "schtasks /end /tn $task | Out-Null" || true
        exit 124
    fi
    sleep 2
done

vm_ssh "Get-Content -Raw '$folder\\task.out'" || true
status="$(vm_ssh "(Get-Content '$folder\\task.done').Trim()" | tr -d '\r')"
exit "${status:-1}"
