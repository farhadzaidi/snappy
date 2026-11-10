#!/bin/bash
# Build the Windows 11 ARM64 test VM from scratch, with no interaction.
#
#   tools/live/windows/create.sh
#
# Downloads Windows 11 Pro ARM64 from Microsoft's Media Creation Tool catalog, the virtio
# drivers, and Win32-OpenSSH; builds a bootable setup ISO and a config ISO holding the answer
# file; installs Windows into a new disk, replacing any existing one; and stops the VM once
# SSH answers. The install takes about 12 minutes after 5.5 GB of downloads. Each download
# and ISO is reused when it's already there; delete it to fetch or build it again.
# Needs `brew install qemu wimlib xorriso`.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
source "$here/common.sh"

catalog_url="https://go.microsoft.com/fwlink/?LinkId=2156292"
virtio_url="https://fedorapeople.org/groups/virt/virtio-win/direct-downloads/stable-virtio/virtio-win.iso"
openssh_version="10.4.0.0p1-Preview"
openssh_url="https://github.com/PowerShell/Win32-OpenSSH/releases/download/$openssh_version/OpenSSH-ARM64.zip"
openssh_sha256="d2b1928225479fec9e0c938f527fdb5ed676446cca1bbcc5af84aca1f272842e"
disk_size="64G"
downloads="$vm_dir/downloads"
build="$vm_dir/build"

log() { echo "$(date '+%H:%M:%S') $*"; }

fail() {
    echo "$*" >&2
    exit 1
}

require_tools() {
    local tool
    for tool in qemu-system-aarch64 qemu-img wimlib-imagex xorriso curl shasum ssh-keygen; do
        command -v "$tool" >/dev/null || fail "Missing $tool. Run: brew install qemu wimlib xorriso"
    done
    [[ -f "$efi_code" ]] || fail "Missing $efi_code."
}

# Prints the URL, SHA-1, and file name of the Windows 11 Pro ARM64 en-US ESD in the catalog.
find_esd() {
    local catalog="$downloads/catalog"
    mkdir -p "$catalog"
    curl -fsSL -o "$catalog/products.cab" "$catalog_url"
    tar -xf "$catalog/products.cab" -C "$catalog" products.xml
    python3 -I - "$catalog/products.xml" <<'EOF'
import sys
import xml.etree.ElementTree as ET

for file in ET.parse(sys.argv[1]).iter("File"):
    fields = {child.tag: (child.text or "") for child in file}
    if (fields.get("Architecture"), fields.get("LanguageCode"), fields.get("Edition")) == ("ARM64", "en-us", "Professional"):
        print(fields["FilePath"], fields["Sha1"], fields["FileName"])
        break
else:
    sys.exit("No Windows 11 Pro ARM64 en-US image in the catalog.")
EOF
}

# Downloads a file unless it's already there with the expected hash, then checks the hash.
fetch() {
    local url="$1" file="$2" algorithm="$3" expected="$4"
    if [[ -f "$file" ]] && [[ "$(shasum -a "$algorithm" "$file" | cut -d' ' -f1)" == "$expected" ]]; then
        return
    fi
    log "Downloading $(basename "$file")"
    curl -fsSL --retry 3 -C - -o "$file" "$url" || curl -fsSL --retry 3 -o "$file" "$url"
    [[ "$(shasum -a "$algorithm" "$file" | cut -d' ' -f1)" == "$expected" ]] || fail "Hash mismatch for $file."
}

download_all() {
    mkdir -p "$downloads"

    local esd_url esd_sha1 esd_name
    read -r esd_url esd_sha1 esd_name < <(find_esd)
    esd="$downloads/$esd_name"
    fetch "$esd_url" "$esd" 1 "$esd_sha1"

    fetch "$openssh_url" "$downloads/OpenSSH-ARM64.zip" 256 "$openssh_sha256"

    # Fedora publishes no checksum next to the stable ISO, so it's only checked to open.
    if [[ ! -f "$virtio_iso" ]]; then
        log "Downloading virtio-win.iso"
        curl -fsSL --retry 3 -o "$virtio_iso.partial" "$virtio_url"
        mv "$virtio_iso.partial" "$virtio_iso"
    fi
    xorriso -indev "$virtio_iso" -ls /NetKVM/w11/ARM64 >/dev/null 2>&1 || fail "virtio-win.iso has no ARM64 network driver."
}

# Builds the setup ISO the way the Media Creation Tool does: image 1 of the ESD is the media
# itself, images 2 and 3 (Windows PE and Windows Setup) make boot.wim, and the Pro image
# makes install.esd. install.esd is over 4 GB; ISO level 3 stores it in several extents,
# which Windows Setup reads.
build_setup_iso() {
    [[ -f "$setup_iso" ]] && return

    local media="$build/setup"
    log "Building $(basename "$setup_iso")"
    rm -rf "$media"
    mkdir -p "$media"
    wimlib-imagex apply "$esd" 1 "$media" >/dev/null
    wimlib-imagex export "$esd" 2 "$media/sources/boot.wim" --compress=LZX >/dev/null
    wimlib-imagex export "$esd" 3 "$media/sources/boot.wim" --boot >/dev/null
    wimlib-imagex export "$esd" "Windows 11 Pro" "$media/sources/install.esd" --compress=LZMS --solid >/dev/null

    # The no-prompt boot image skips "Press any key to boot from CD or DVD".
    xorriso -as mkisofs -quiet \
        -iso-level 3 -J -joliet-long -D -N -relaxed-filenames -allow-lowercase \
        -V SNAPPY_WIN11_A64 \
        -e efi/microsoft/boot/efisys_noprompt.bin -no-emul-boot \
        -o "$setup_iso.partial" "$media"
    mv "$setup_iso.partial" "$setup_iso"
    rm -rf "$media"
}

# Builds the small ISO holding the answer file, the first-logon script, OpenSSH, the host's
# public key, the virtio storage and network drivers that setup loads from $WinPEDriver$,
# and the display driver that the first-logon script installs.
build_config_iso() {
    local config="$build/config"
    log "Building $(basename "$config_iso")"
    rm -rf "$config"
    mkdir -p "$config/\$WinPEDriver\$"
    cp "$here/autounattend.xml" "$here/first-logon.ps1" "$downloads/OpenSSH-ARM64.zip" "$config/"
    cp "$ssh_key.pub" "$config/authorized_keys"

    local driver
    for driver in viostor vioscsi NetKVM; do
        xorriso -osirrox on -indev "$virtio_iso" -extract "/$driver/w11/ARM64" "$config/\$WinPEDriver\$/$driver" >/dev/null 2>&1
    done
    xorriso -osirrox on -indev "$virtio_iso" -extract /viogpudo/w11/ARM64 "$config/viogpudo" >/dev/null 2>&1
    chmod -R u+w "$config"

    xorriso -as mkisofs -quiet -J -joliet-long -r -V SNAPPY_CONFIG -o "$config_iso" "$config"
    rm -rf "$config"
}

create_disk() {
    log "Creating a $disk_size disk and fresh firmware variables"
    rm -f "$disk" "$efi_vars" "$known_hosts"
    qemu-img create -q -f qcow2 "$disk" "$disk_size"
    cp "$qemu_share/edk2-arm-vars.fd" "$efi_vars"
}

# Saves a screenshot of the stuck VM, stops it, and fails.
abort_install() {
    screenshot "$vm_dir/failure.png" 2>/dev/null || true
    kill "$(cat "$pid_file")" 2>/dev/null || true
    fail "$1 Screenshot: $vm_dir/failure.png. VNC while it runs: 127.0.0.1:$((5900 + vnc_display))."
}

# Windows PE copies Windows to the disk, then restarts. -no-reboot turns that restart into an
# exit, so the setup ISO can be detached before it boots again and starts over.
install_windows() {
    # Until the first-logon script installs the virtio display driver, Windows draws only to ramfb.
    display_device=ramfb

    local cdroms=()
    while IFS= read -r line; do cdroms+=("$line"); done < <(usb_cdrom setup "$setup_iso"; usb_cdrom config "$config_iso")

    log "Installing Windows, first stage"
    launch_vm "${cdroms[@]}" -no-reboot
    wait_for_exit 5400 || abort_install "Windows PE didn't finish within 90 minutes."

    local written
    written="$(qemu-img info --output=json "$disk" | python3 -I -c 'import json, sys; print(json.load(sys.stdin)["actual-size"])')"
    ((written > 4000000000)) || fail "The first stage stopped before Windows was copied to the disk."

    cdroms=()
    while IFS= read -r line; do cdroms+=("$line"); done < <(usb_cdrom config "$config_iso")

    log "Installing Windows, second stage: devices, first logon, OpenSSH"
    launch_vm "${cdroms[@]}"
    wait_for_ssh 3600 || abort_install "SSH didn't answer within an hour of the second stage."

    local deadline=$((SECONDS + 600))
    until vm_ssh 'Test-Path C:\Windows\Temp\snappy-setup.done' 2>/dev/null | grep -q True; do
        ((SECONDS < deadline)) || abort_install "The first-logon script didn't finish. See C:\\Windows\\Temp\\snappy-setup.log."
        sleep 10
    done

    log "Shutting down"
    vm_ssh 'Stop-Computer -Force' >/dev/null 2>&1 || true
    wait_for_exit 300 || qmp '{"execute": "quit"}' >/dev/null
}

# Saves the freshly installed disk and firmware variables, for reset.sh.
save_installed_state() {
    qemu-img snapshot -c "$installed_snapshot" "$disk"
    cp "$efi_vars" "$installed_efi_vars"
}

require_tools
vm_running && fail "The VM is running. Stop it first: tools/live/windows/stop.sh"
mkdir -p "$vm_dir" "$build"

if [[ ! -f "$ssh_key" ]]; then
    ssh-keygen -q -t ed25519 -N '' -C snappy-windows-vm -f "$ssh_key"
fi

download_all
build_setup_iso
build_config_iso
create_disk
install_windows
save_installed_state
log "Done. Start the VM with tools/live/windows/start.sh."
