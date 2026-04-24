#!/bin/bash
set -e

# AegisCore Installation Script
# Detects OS/Arch, downloads the latest binary, and installs the daemon.

GITHUB_REPO="Mattew/aegis"
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

# 2. Download Binary (Placeholder for GitHub Releases)
# VERSION=$(curl -s https://api.github.com/repos/$GITHUB_REPO/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
# BINARY_URL="https://github.com/$GITHUB_REPO/releases/download/$VERSION/aegis-$OS-$BINARY_ARCH.tar.gz"

# For now, we assume the user has built it locally or we provide a build command.
if [ ! -f "./target/release/aegis" ]; then
    echo "Binary not found in target/release/aegis. Please run 'cargo build --release' first."
    exit 1
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
"$INSTALL_DIR/aegis" daemon install

echo "─────────────────────────────────────────────────"
echo "AegisCore installed successfully!"
echo "Run 'aegis doctor' to verify your installation."
echo "Run 'aegis daemon start' to start the global daemon."
echo "─────────────────────────────────────────────────"
