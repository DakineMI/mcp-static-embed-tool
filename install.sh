#!/bin/bash

# Installation script for Static Embedding Server
# This script builds and installs the embed-tool binary

set -e

echo "🚀 Installing Static Embedding Server..."

# Check if cargo is available
if ! command -v cargo &> /dev/null; then
    echo "❌ Cargo (Rust) is not installed. Please install Rust first:"
    echo "   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

echo "📦 Building release binary..."
cargo build --release

# Determine installation directory
if [[ $EUID -eq 0 ]]; then
    # Running as root, install system-wide
    INSTALL_DIR="/usr/local/bin"
else
    # Running as user, install to ~/.local/bin
    INSTALL_DIR="$HOME/.local/bin"
    mkdir -p "$INSTALL_DIR"
fi

BINARY_PATH="./target/release/static_embedding_server"
INSTALL_PATH="$INSTALL_DIR/embed-tool"

echo "📋 Installing binary to $INSTALL_PATH..."
cp "$BINARY_PATH" "$INSTALL_PATH"
chmod +x "$INSTALL_PATH"

echo "✅ Installation complete!"
echo ""
echo "🎉 Static Embedding Server installed as 'embed-tool'"
echo ""
echo "📚 Quick start:"
echo "   embed-tool --help                    # Show help"
echo "   embed-tool server start              # Start the server"
echo "   embed-tool embed \"Hello world\"      # Generate embeddings"
echo ""
echo "📖 For more information, see the README.md file"

# Add to PATH if not already there
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo ""
    echo "💡 To use embed-tool from anywhere, add $INSTALL_DIR to your PATH:"
    if [[ -n "$ZSH_VERSION" ]]; then
        echo "   echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.zshrc"
        echo "   source ~/.zshrc"
    elif [[ -n "$BASH_VERSION" ]]; then
        echo "   echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.bashrc"
        echo "   source ~/.bashrc"
    else
        echo "   Add 'export PATH=\"$INSTALL_DIR:\$PATH\"' to your shell profile"
    fi
fi