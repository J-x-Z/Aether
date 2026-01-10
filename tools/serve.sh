#!/bin/bash
set -e

# Change to project root (parent of this script's directory)
cd "$(dirname "$0")/.."

# Configuration - Auto-detect IP
IP=$(ipconfig getifaddr en0 2>/dev/null || ipconfig getifaddr en1 2>/dev/null || echo "localhost")
PORT="8000"
TARGET_DIR="target/x86_64-unknown-uefi/release"
EFI_FILE="aether.efi"
DEPLOY_DIR="deploy"

# 0. Ensure Target is Installed
echo "🔧 Checking Rust target..."
rustup target add x86_64-unknown-uefi

# 1. Build the Kernel
echo "🔨 Building AetherOS..."
cargo build --release --target x86_64-unknown-uefi

# 2. Prepare Deploy Directory
mkdir -p $DEPLOY_DIR
cp "$TARGET_DIR/$EFI_FILE" "$DEPLOY_DIR/"

# 3. Generate iPXE Script
echo "📝 Generating iPXE Script..."
cat > "$DEPLOY_DIR/boot.ipxe" <<EOF
#!ipxe
dhcp
chain http://$IP:$PORT/$EFI_FILE
EOF

# 4. Generate Instructions
echo "
===========================================================
🚀 Network Boot Server Ready!
===========================================================

Option A: UEFI HTTP Boot (Direct)
---------------------------------
URL: http://$IP:$PORT/$EFI_FILE

Option B: iPXE Boot (Recommended)
---------------------------------
URL: http://$IP:$PORT/boot.ipxe

On your target machine:
1. Connect Ethernet/Wi-Fi
2. Select 'HTTP Boot' in BIOS
3. Enter one of the URLs above
   (Or use an iPXE USB stick pointing to Option B)

To update:
Press Ctrl+C, run this script again, then Reboot target.
===========================================================
"

# 5. Start HTTP Server
echo "📡 Serving on $IP:$PORT..."
cd $DEPLOY_DIR
python3 -m http.server $PORT
