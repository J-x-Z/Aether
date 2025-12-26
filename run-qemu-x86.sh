#!/bin/bash
# Run Aether Kernel on QEMU x86_64 with UEFI
#
# Prerequisites:
#   brew install qemu
#   Download OVMF.fd from: https://www.kraxel.org/repos/jenkins/edk2/

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Build kernel for x86_64
echo "[Build] Building Aether Kernel for x86_64-unknown-uefi..."
PATH="$HOME/.cargo/bin:$PATH" cargo build --release --target x86_64-unknown-uefi

# Create ESP directory structure
ESP_DIR="$SCRIPT_DIR/esp"
mkdir -p "$ESP_DIR/EFI/BOOT"

# Copy the kernel EFI binary
cp "$SCRIPT_DIR/target/x86_64-unknown-uefi/release/aether.efi" "$ESP_DIR/EFI/BOOT/BOOTX64.EFI"

echo "[QEMU] Starting QEMU x86_64..."

# Check for UEFI firmware
if [ -f /opt/homebrew/share/qemu/edk2-x86_64-code.fd ]; then
    UEFI_FW="/opt/homebrew/share/qemu/edk2-x86_64-code.fd"
elif [ -f /usr/share/OVMF/OVMF_CODE.fd ]; then
    UEFI_FW="/usr/share/OVMF/OVMF_CODE.fd"
elif [ -f "$SCRIPT_DIR/OVMF.fd" ]; then
    UEFI_FW="$SCRIPT_DIR/OVMF.fd"
else
    echo "Error: UEFI firmware not found!"
    echo "Please download from: https://www.kraxel.org/repos/jenkins/edk2/"
    echo "And place OVMF.fd in $SCRIPT_DIR"
    exit 1
fi

qemu-system-x86_64 \
    -m 512M \
    -bios "$UEFI_FW" \
    -drive format=raw,file=fat:rw:"$ESP_DIR" \
    -nographic \
    -serial mon:stdio
