#!/bin/bash

# Development workflow with 1Password git-crypt integration
# This ensures .specstory is always available for development

echo "🚀 Starting development environment..."

# Check if git-crypt is locked
if git-crypt status 2>/dev/null | grep -q "encrypted"; then
    echo "🔒 Repository is locked, unlocking with 1Password..."
    
    # Unlock using 1Password
    op document get "specstory-encryption-key" --vault="Personal" | git-crypt unlock /dev/stdin
    
    if [ $? -eq 0 ]; then
        echo "✅ Repository unlocked!"
    else
        echo "❌ Failed to unlock. Make sure:"
        echo "   1. 1Password CLI is installed and signed in"
        echo "   2. Key exists in 1Password as 'specstory-encryption-key'"
        exit 1
    fi
else
    echo "✅ Repository already unlocked"
fi

# Optional: Start your development server
echo "🔧 Repository ready for development"
echo ""
echo "Available commands:"
echo "  cargo run server start     # Start embedding server"
echo "  cargo test                 # Run tests"
echo "  git-crypt lock             # Lock repository when done"