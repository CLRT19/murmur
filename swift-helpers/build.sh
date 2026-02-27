#!/bin/bash
# Build the murmur-transcribe Swift helper for macOS
# This creates a standalone binary that uses Apple's Speech framework for STT.
#
# Usage: ./build.sh [--release]
# Output: ./murmur-transcribe
#
# Requirements:
# - macOS 10.15+ (Catalina or later)
# - Swift compiler (Xcode or Command Line Tools)
# - Matching Swift compiler and SDK versions

set -euo pipefail

cd "$(dirname "$0")"

# Check we're on macOS
if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "Error: murmur-transcribe only builds on macOS (requires Speech.framework)"
    exit 1
fi

# Check swiftc is available
if ! command -v swiftc &>/dev/null; then
    echo "Error: swiftc not found. Install Xcode or Command Line Tools."
    exit 1
fi

RELEASE=0
if [[ "${1:-}" == "--release" ]]; then
    RELEASE=1
fi

SWIFT_FLAGS=""
if [[ $RELEASE -eq 1 ]]; then
    SWIFT_FLAGS="-O"
fi

echo "Building murmur-transcribe..."
echo "  Swift: $(swiftc --version 2>&1 | head -1)"

swiftc \
    $SWIFT_FLAGS \
    -framework Speech \
    -Xlinker -sectcreate \
    -Xlinker __TEXT \
    -Xlinker __info_plist \
    -Xlinker Info.plist \
    -o murmur-transcribe \
    transcribe.swift

# Code-sign with ad-hoc signature and entitlements
codesign --sign - \
    --entitlements entitlements.plist \
    --force \
    ./murmur-transcribe

echo "Built: $(pwd)/murmur-transcribe"
echo "Test:  ./murmur-transcribe test.wav"
