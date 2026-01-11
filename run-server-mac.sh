#!/bin/bash
set -e

# Configuration
# The IP address of this Mac (Server).
# You can hardcode it or let the script detect it.
# SERVER_IP="192.168.100.15" 
# Auto-detect en0 (WiFi/Ethernet) IP
SERVER_IP=$(ipconfig getifaddr en0 || echo "127.0.0.1")
PORT="8000"
DEPLOY_DIR="deploy"

# Colors
GREEN='\033[0;32m'
NC='\033[0m'

echo -e "${GREEN}[Mac Server] Detected IP: $SERVER_IP${NC}"

# 1. Build the Kernel (Optional - if you want to ensure latest version)
echo -e "${GREEN}[Mac Server] Building Kernel...${NC}"
cargo build --target x86_64-unknown-uefi

# 2. Prepare Hosting Directory
rm -rf $DEPLOY_DIR
mkdir -p $DEPLOY_DIR/EFI/BOOT

# 3. Copy Kernel
# Note: Ensure the path matches your target directory structure
KERNEL_SRC="target/x86_64-unknown-uefi/debug/aether.efi"
if [ ! -f "$KERNEL_SRC" ]; then
    echo "Error: Kernel binary not found at $KERNEL_SRC"
    exit 1
fi
cp "$KERNEL_SRC" "$DEPLOY_DIR/EFI/BOOT/BOOTX64.EFI"
echo -e "${GREEN}[Mac Server] Copied BOOTX64.EFI${NC}"

# 4. Create boot.ipxe
cat > "$DEPLOY_DIR/boot.ipxe" <<EOF
#!ipxe
echo [Aether] Loading Kernel from $SERVER_IP...
chain http://$SERVER_IP:$PORT/EFI/BOOT/BOOTX64.EFI || shell
boot
EOF
echo -e "${GREEN}[Mac Server] Generated boot.ipxe${NC}"

# 5. Start HTTP Server
echo -e "${GREEN}[Mac Server] Starting HTTP Server on port $PORT...${NC}"
echo "Press Ctrl+C to stop."
cd $DEPLOY_DIR
python3 -m http.server $PORT
