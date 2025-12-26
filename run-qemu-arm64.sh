#!/bin/bash
# Run Aether Kernel on QEMU ARM64 with UEFI
#
# Prerequisites:
#   brew install qemu
#   Download QEMU_EFI.fd from: https://releases.linaro.org/components/kernel/uefi-linaro/latest/

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Build kernel for ARM64
echo "[Build] Building Aether Kernel for aarch64-unknown-uefi..."
PATH="$HOME/.cargo/bin:$PATH" cargo build --release --target aarch64-unknown-uefi

# Create ESP directory structure
ESP_DIR="$SCRIPT_DIR/esp"
mkdir -p "$ESP_DIR/EFI/BOOT"

# Copy the kernel EFI binary
cp "$SCRIPT_DIR/target/aarch64-unknown-uefi/release/aether.efi" "$ESP_DIR/EFI/BOOT/BOOTAA64.EFI"

echo "[QEMU] Starting QEMU aarch64..."

# Check for UEFI firmware
if [ -f /opt/homebrew/share/qemu/edk2-aarch64-code.fd ]; then
    UEFI_FW="/opt/homebrew/share/qemu/edk2-aarch64-code.fd"
elif [ -f /usr/share/qemu-efi-aarch64/QEMU_EFI.fd ]; then
    UEFI_FW="/usr/share/qemu-efi-aarch64/QEMU_EFI.fd"
elif [ -f "$SCRIPT_DIR/QEMU_EFI.fd" ]; then
    UEFI_FW="$SCRIPT_DIR/QEMU_EFI.fd"
else
    echo "Error: UEFI firmware not found!"
    echo "Please download from: https://releases.linaro.org/components/kernel/uefi-linaro/latest/"
    echo "And place QEMU_EFI.fd in $SCRIPT_DIR"
    exit 1
fi

qemu-system-aarch64 \
    -M virt \
    -cpu cortex-a72 \
    -m 512M \
    -bios "$UEFI_FW" \
    -drive if=virtio,format=raw,file=fat:rw:"$ESP_DIR" \
    -nographic \
    -serial mon:stdio
