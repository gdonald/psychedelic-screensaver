#!/bin/bash
# Build Psychedelic.saver from the Rust core and the Swift shell.
set -euo pipefail

root="$(cd "$(dirname "$0")" && pwd)"
build="$root/build"
bundle="$build/Psychedelic.saver"

cargo build --release --lib

rm -rf "$bundle"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources" "$build/objects"
cp "$root/saver/Info.plist" "$bundle/Contents/Info.plist"

sdk="$(xcrun --show-sdk-path)"

swiftc -c \
    "$root/saver/PsychedelicSaverView.swift" \
    "$root/saver/ConfigureSheetController.swift" \
    "$root/saver/Settings.swift" \
    -import-objc-header "$root/saver/PsyBridge.h" \
    -module-name Psychedelic \
    -swift-version 5 \
    -O \
    -whole-module-optimization \
    -sdk "$sdk" \
    -target arm64-apple-macos13.0 \
    -o "$build/objects/Psychedelic.o"

clang -bundle \
    -o "$bundle/Contents/MacOS/Psychedelic" \
    "$build/objects/Psychedelic.o" \
    "$root/target/release/libpsychedelic.a" \
    -isysroot "$sdk" \
    -target arm64-apple-macos13.0 \
    -framework ScreenSaver \
    -framework Cocoa \
    -framework Metal \
    -framework QuartzCore \
    -L"$sdk/usr/lib/swift" \
    -Xlinker -rpath -Xlinker /usr/lib/swift

codesign --force --sign - --timestamp=none "$bundle"

echo "built $bundle"
