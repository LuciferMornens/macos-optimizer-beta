#!/bin/bash

# Build script for macOS Optimizer

echo "Building macOS Optimizer for production..."

# Build the Tauri app for macOS
npm run tauri build

echo "Build complete! The app bundle can be found in:"
echo "src-tauri/target/release/bundle/macos/"
echo ""
echo "You can install the app by dragging it to your Applications folder."