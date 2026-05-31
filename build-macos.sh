#!/bin/bash
# Build ThetisLink Client for macOS (Intel x86_64)
# Run this script on the Mac after cloning the repo.
#
# Prerequisites:
#   1. Install Rust:
#      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
#      source "$HOME/.cargo/env"
#
#   2. Install Xcode Command Line Tools (for C compiler/linker):
#      xcode-select --install
#
# Usage:
#   chmod +x build-macos.sh
#   ./build-macos.sh

set -e

echo "Building ThetisLink Client for macOS..."
cargo build --release -p sdr-remote-client

BINARY="target/release/ThetisLink-Client"
if [ -f "$BINARY" ]; then
    echo ""
    echo "Build succeeded: $BINARY"
    echo "Run with: ./$BINARY"
else
    echo "ERROR: Binary not found at $BINARY"
    exit 1
fi
