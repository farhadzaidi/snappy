#!/bin/bash
# Shared paths and QEMU arguments for the Windows 11 ARM64 test VM. Sourced by the other scripts.
# Large files live outside the repository, in SNAPPY_WINDOWS_VM_DIR.

vm_dir="${SNAPPY_WINDOWS_VM_DIR:-$HOME/VMs/snappy-windows}"
ssh_port="${SNAPPY_WINDOWS_SSH_PORT:-2222}"
vnc_display="${SNAPPY_WINDOWS_VNC_DISPLAY:-15}"
# The guest driver (viogpudo) takes the resolution from this device. Its size is the screen size.
display_device="${SNAPPY_WINDOWS_DISPLAY_DEVICE:-virtio-gpu-pci,xres=1920,yres=1080}"

disk="$vm_dir/disk.qcow2"
efi_vars="$vm_dir/efi-vars.fd"
installed_efi_vars="$vm_dir/efi-vars.installed.fd"
installed_snapshot="installed"
setup_iso="$vm_dir/win11-arm64.iso"
config_iso="$vm_dir/config.iso"
virtio_iso="$vm_dir/downloads/virtio-win.iso"
ssh_key="$vm_dir/id_ed25519"
known_hosts="$vm_dir/known_hosts"
pid_file="$vm_dir/qemu.pid"
qmp_socket="$vm_dir/qmp.sock"
qemu_log="$vm_dir/qemu.log"

qemu_share="$(brew --prefix qemu 2>/dev/null || echo /opt/homebrew)/share/qemu"
efi_code="$qemu_share/edk2-aarch64-code.fd"

# Prints the QEMU command line. Extra arguments (CD-ROMs, -no-reboot) are appended.
# The VM is headless: no window, a VNC server on localhost only, and QMP for screenshots
# and shutdown. NVMe needs no driver in Windows.
qemu_command() {
    printf '%s\n' \
        qemu-system-aarch64 \
        -name snappy-windows \
        -machine virt,highmem=on,gic-version=3 \
        -accel hvf \
        -cpu host \
        -smp 4 \
        -m 8G \
        -drive "if=pflash,format=raw,readonly=on,file=$efi_code" \
        -drive "if=pflash,format=raw,file=$efi_vars" \
        -drive "if=none,id=disk0,format=qcow2,discard=unmap,file=$disk" \
        -device nvme,drive=disk0,serial=snappy-disk0 \
        -device qemu-xhci,id=xhci,p2=8,p3=8 \
        -device usb-kbd,bus=xhci.0 \
        -device usb-tablet,bus=xhci.0 \
        -device "$display_device" \
        -netdev "user,id=net0,hostfwd=tcp:127.0.0.1:$ssh_port-:22" \
        -device virtio-net-pci,netdev=net0 \
        -display none \
        -vnc "127.0.0.1:$vnc_display" \
        -qmp "unix:$qmp_socket,server=on,wait=off" \
        -pidfile "$pid_file" \
        -daemonize \
        -D "$qemu_log" \
        "$@"
}

# Prints the arguments that attach an ISO as a USB CD-ROM, which Windows reads without drivers.
usb_cdrom() {
    local id="$1" file="$2"
    printf '%s\n' \
        -drive "if=none,id=$id,media=cdrom,readonly=on,file=$file" \
        -device "usb-storage,bus=xhci.0,drive=$id,removable=on"
}

# Starts QEMU in the background with the given extra arguments.
launch_vm() {
    local args=()
    while IFS= read -r line; do args+=("$line"); done < <(qemu_command "$@")
    rm -f "$qmp_socket"
    "${args[@]}"
}

vm_running() {
    [[ -f "$pid_file" ]] && kill -0 "$(cat "$pid_file")" 2>/dev/null
}

# Sends one QMP command, given as JSON, and prints the reply.
qmp() {
    python3 -I - "$qmp_socket" "$1" <<'EOF'
import json, socket, sys

sock = socket.socket(socket.AF_UNIX)
sock.connect(sys.argv[1])
stream = sock.makefile("rw")
stream.readline()
for command in ('{"execute": "qmp_capabilities"}', sys.argv[2]):
    stream.write(command + "\n")
    stream.flush()
    while True:
        reply = json.loads(stream.readline())
        if "event" not in reply:
            break
print(json.dumps(reply))
EOF
}

# Saves what the VM's display shows, for inspecting a stuck install without a window.
screenshot() {
    qmp "{\"execute\": \"screendump\", \"arguments\": {\"filename\": \"$1\", \"format\": \"png\"}}" >/dev/null
}

ssh_options() {
    printf '%s\n' \
        -i "$ssh_key" \
        -p "$ssh_port" \
        -o IdentitiesOnly=yes \
        -o BatchMode=yes \
        -o StrictHostKeyChecking=accept-new \
        -o "UserKnownHostsFile=$known_hosts" \
        -o ConnectTimeout=5 \
        -o LogLevel=ERROR
}

vm_ssh() {
    local options=()
    while IFS= read -r line; do options+=("$line"); done < <(ssh_options)
    ssh "${options[@]}" snappy@127.0.0.1 "$@"
}

# Waits until SSH answers, for at most the given number of seconds.
wait_for_ssh() {
    local deadline=$((SECONDS + $1))
    until vm_ssh 'exit 0' 2>/dev/null; do
        if ! vm_running; then
            echo "The VM stopped before SSH answered. See $qemu_log." >&2
            return 1
        fi
        if ((SECONDS >= deadline)); then
            echo "SSH did not answer within $1 seconds." >&2
            return 1
        fi
        sleep 10
    done
}

# Waits until QEMU exits, for at most the given number of seconds.
wait_for_exit() {
    local deadline=$((SECONDS + $1))
    while vm_running; do
        if ((SECONDS >= deadline)); then
            return 1
        fi
        sleep 10
    done
}
