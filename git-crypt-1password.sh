#!/bin/bash

# Git-crypt with 1Password Integration
# ====================================

KEY_NAME="specstory-encryption-key"
VAULT_NAME="Personal"  # Change to your vault name

setup_1password_key() {
    echo "Setting up git-crypt key in 1Password..."
    
    # Check if 1Password CLI is installed
    if ! command -v op &> /dev/null; then
        echo "❌ 1Password CLI not found. Install it first:"
        echo "   brew install 1password-cli"
        echo "   op signin"
        exit 1
    fi
    
    # Check if key file exists
    if [ ! -f "specstory-encryption-key.key" ]; then
        echo "❌ Key file not found. Generate it first with:"
        echo "   git-crypt export-key specstory-encryption-key.key"
        exit 1
    fi
    
    # Store the key in 1Password
    echo "📦 Storing key in 1Password..."
    op document create "specstory-encryption-key.key" \
        --title="$KEY_NAME" \
        --vault="$VAULT_NAME" \
        --tags="git-crypt,specstory,embedding-server"
    
    if [ $? -eq 0 ]; then
        echo "✅ Key stored in 1Password successfully!"
        echo "🗑️  You can now safely delete the local key file:"
        echo "   rm specstory-encryption-key.key"
    else
        echo "❌ Failed to store key in 1Password"
        exit 1
    fi
}

unlock_with_1password() {
    echo "🔓 Unlocking git-crypt with key from 1Password..."
    
    # Download key from 1Password and unlock directly
    op document get "$KEY_NAME" --vault="$VAULT_NAME" | git-crypt unlock /dev/stdin
    
    if [ $? -eq 0 ]; then
        echo "✅ Git-crypt unlocked successfully!"
    else
        echo "❌ Failed to unlock git-crypt"
        exit 1
    fi
}

case "${1:-}" in
    setup)
        setup_1password_key
        ;;
    unlock)
        unlock_with_1password
        ;;
    *)
        echo "Usage: $0 {setup|unlock}"
        echo ""
        echo "Commands:"
        echo "  setup  - Store git-crypt key in 1Password"
        echo "  unlock - Unlock git-crypt using key from 1Password"
        echo ""
        echo "Setup steps:"
        echo "1. Install 1Password CLI: brew install 1password-cli"
        echo "2. Sign in: op signin"
        echo "3. Store key: $0 setup"
        echo "4. Unlock anytime: $0 unlock"
        ;;
esac