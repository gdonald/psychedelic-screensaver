#!/bin/bash
# Load the built bundle and drive it, the same way System Settings would.
set -euo pipefail

root="$(cd "$(dirname "$0")" && pwd)"
sdk="$(xcrun --show-sdk-path)"

swiftc "$root/saver/check-bundle.swift" \
    -o "$root/build/check-bundle" \
    -sdk "$sdk" \
    -target arm64-apple-macos13.0 \
    -framework ScreenSaver \
    -framework Cocoa

"$root/build/check-bundle" "$root/build/Psychedelic.saver"
