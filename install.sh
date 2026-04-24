#!/bin/bash
set -e

# AegisCore Installation Script
# Detects OS/Arch, builds if necessary, and installs the daemon.

GITHUB_REPO="bigchkn/AegisCore"
INSTALL_DIR="/usr/local/bin"
AEGIS_DIR="$HOME/.aegis"

# 1. Detect OS and Architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

if [ "$OS" != "darwin" ]; then
    echo "Error: AegisCore currently only supports macOS (darwin)."
    exit 1
fi

if [ "$ARCH" == "x86_64" ]; then
    BINARY_ARCH="x86_64"
elif [ "$ARCH" == "arm64" ]; then
    BINARY_ARCH="arm64"
else
    echo "Error: Unsupported architecture: $ARCH"
    exit 1
fi

echo "Detected: $OS-$BINARY_ARCH"

# 2. Build or Download Binary
if [ ! -f "./target/release/aegis" ] || [ ! -f "./target/release/aegisd" ]; then
    echo "Binaries not found in target/release/. Attempting to build..."
    if ! command -v cargo &> /dev/null; then
        echo "Error: 'cargo' is not installed and binaries are missing."
        echo "Please install Rust/Cargo from https://rustup.rs/ or provide pre-built binaries."
        exit 1
    fi
    cargo build --release
fi

# 3. Install Binary
echo "Installing aegis to $INSTALL_DIR..."
sudo cp ./target/release/aegis "$INSTALL_DIR/aegis"
sudo cp ./target/release/aegisd "$INSTALL_DIR/aegisd"
sudo chmod +x "$INSTALL_DIR/aegis" "$INSTALL_DIR/aegisd"

# 4. Initialize Aegis Directory
echo "Initializing $AEGIS_DIR..."
mkdir -p "$AEGIS_DIR"
chmod 700 "$AEGIS_DIR"

# 5. Install Launchd Plist
echo "Installing daemon..."
./target/release/aegis daemon install

echo "─────────────────────────────────────────────────"
echo "AegisCore installed successfully!"
echo "Run 'aegis doctor' to verify your installation."
echo "Run 'aegis daemon start' to start the global daemon."
echo "─────────────────────────────────────────────────"
