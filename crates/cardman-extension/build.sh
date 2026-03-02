#!/usr/bin/env zsh
# Build the cardman-extension wasm + package for browser loading.
#
# Prerequisites:
#   cargo install wasm-pack
#   rustup target add wasm32-unknown-unknown
#
# Usage:
#   zsh crates/cardman-extension/build.sh

set -euo pipefail

CRATE_DIR="$(cd "$(dirname "$0")" && pwd)"
DIST_DIR="${CRATE_DIR}/dist"

echo "🃏 Building Cardman extension…"

# Clean previous build
rm -rf "${DIST_DIR}"
mkdir -p "${DIST_DIR}/pkg" "${DIST_DIR}/icons"

# Build wasm via wasm-pack (produces pkg/ with .wasm + .js glue)
echo "  → Compiling extension wasm…"
wasm-pack build "${CRATE_DIR}" --target web --out-dir "${DIST_DIR}/pkg" --no-typescript

# Build lightweight bridge wasm (no Dioxus, safe for service worker)
BRIDGE_DIR="${CRATE_DIR}/../cardman-wasm-bridge"
echo "  → Compiling bridge wasm…"
wasm-pack build "${BRIDGE_DIR}" --target web --out-dir "${DIST_DIR}/bridge" --no-typescript

# Copy static files
echo "  → Copying static files…"
cp "${CRATE_DIR}/manifest.json" "${DIST_DIR}/"
cp "${CRATE_DIR}/content_loader.js" "${DIST_DIR}/"
cp "${CRATE_DIR}/content.css" "${DIST_DIR}/"
cp "${CRATE_DIR}/background.js" "${DIST_DIR}/"
cp "${CRATE_DIR}/popup.html" "${DIST_DIR}/"
cp "${CRATE_DIR}/popup.js" "${DIST_DIR}/"
cp "${CRATE_DIR}/popup.css" "${DIST_DIR}/"

# Copy icons if they exist
if [ -d "${CRATE_DIR}/icons" ]; then
  cp "${CRATE_DIR}/icons/"* "${DIST_DIR}/icons/" 2>/dev/null || true
fi

echo "✅ Extension built in ${DIST_DIR}"
echo ""
echo "To load in Chrome:"
echo "  1. Open chrome://extensions"
echo "  2. Enable Developer mode"
echo "  3. Click 'Load unpacked'"
echo "  4. Select: ${DIST_DIR}"
echo ""
echo "To load in Firefox:"
echo "  1. Open about:debugging#/runtime/this-firefox"
echo "  2. Click 'Load Temporary Add-on'"
echo "  3. Select: ${DIST_DIR}/manifest.json"
