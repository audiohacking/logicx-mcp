#!/usr/bin/env bash
# Build a macOS .pkg installer for LogicX MCP (AU v2 + standalone) from the Truce build tree.
#
# Pattern matches audiohacking/aitroce-vst/scripts/build-installer-pkg.sh
#
# Run from repo root after:
#   cargo truce build --au2 -p logicx-plugin
#   ./scripts/build-installer-pkg.sh --build-standalone --sign-plugins
#
# Or let this script build everything:
#   ./scripts/build-installer-pkg.sh --build --sign-plugins
#
# Usage:
#   ./scripts/build-installer-pkg.sh [--build] [--build-standalone] [--sign-plugins] [--version 0.1.0]
#   ./scripts/build-installer-pkg.sh --au path/to/LogicX\ MCP.component --standalone path/to/LogicX\ MCP.app
#
# Output:
#   release-artefacts/logicx-mcp-<version>-macos-au-standalone.zip
#   release-artefacts/LogicX-MCP-macOS-Installer.pkg
#
# Installs to:
#   /Library/Audio/Plug-Ins/Components/  (AU v2)
#   /Applications/                       (standalone .app)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

PLUGIN_DISPLAY_NAME="LogicX MCP"
AU_COMPONENT_NAME="LogicX MCP.component"
STANDALONE_APP_NAME="LogicX MCP.app"
STANDALONE_BIN="logicx-mcp-standalone"
PKG_ID="com.audiohacking.logicx-mcp"
OUT_DIR="release-artefacts"
ZIP_NAME=""
PKG_NAME="LogicX-MCP-macOS-Installer.pkg"

DO_BUILD=false
BUILD_STANDALONE=false
SIGN_PLUGINS=false
PKG_VERSION=""
AU_PATH=""
STANDALONE_PATH=""

while [ $# -gt 0 ]; do
  case "$1" in
    --build)            DO_BUILD=true; shift ;;
    --build-standalone) BUILD_STANDALONE=true; shift ;;
    --sign-plugins)     SIGN_PLUGINS=true; shift ;;
    --version)          PKG_VERSION="$2"; shift 2 ;;
    --au)               AU_PATH="$2"; shift 2 ;;
    --standalone)       STANDALONE_PATH="$2"; shift 2 ;;
    -h|--help)
      sed -n '1,26p' "$0"
      exit 0
      ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

if [ -z "$PKG_VERSION" ]; then
  PKG_VERSION=$(awk -F\" '/^version[[:space:]]*=/ { print $2; exit }' Cargo.toml)
fi
ZIP_NAME="logicx-mcp-${PKG_VERSION}-macos-au-standalone.zip"

stage_standalone_app() {
  local built="target/release/${STANDALONE_BIN}"
  local staged="target/bundles/${STANDALONE_APP_NAME}"

  if [ ! -f "$built" ]; then
    echo "Building standalone binary..." >&2
    cargo build --release -p logicx-plugin --features standalone
    built="target/release/${STANDALONE_BIN}"
  fi

  if [ ! -f "$built" ]; then
    echo "Error: standalone binary not found at $built" >&2
    exit 1
  fi

  echo "Staging ${STANDALONE_APP_NAME}..." >&2
  rm -rf "$staged"
  mkdir -p "$staged/Contents/MacOS"
  cp "$built" "$staged/Contents/MacOS/${STANDALONE_BIN}"
  chmod 755 "$staged/Contents/MacOS/${STANDALONE_BIN}"

  cat > "$staged/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>${PLUGIN_DISPLAY_NAME}</string>
    <key>CFBundleDisplayName</key>
    <string>${PLUGIN_DISPLAY_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>com.audiohacking.logicx-mcp.standalone</string>
    <key>CFBundleExecutable</key>
    <string>${STANDALONE_BIN}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleVersion</key>
    <string>${PKG_VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${PKG_VERSION}</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSMicrophoneUsageDescription</key>
    <string>${PLUGIN_DISPLAY_NAME} would like to use the microphone for plugin audio input.</string>
</dict>
</plist>
EOF
}

if [ "$DO_BUILD" = true ]; then
  echo "Building AU v2..." >&2
  cargo truce build --au2 -p logicx-plugin
  BUILD_STANDALONE=true
fi

if [ "$BUILD_STANDALONE" = true ]; then
  stage_standalone_app
fi

if [ -z "$AU_PATH" ]; then
  AU_PATH=$(find target/bundles -name "$AU_COMPONENT_NAME" -type d 2>/dev/null | head -1 || true)
fi

if [ -z "$STANDALONE_PATH" ]; then
  STANDALONE_PATH=$(find target/bundles -name "$STANDALONE_APP_NAME" -type d 2>/dev/null | head -1 || true)
fi

if [ -z "$AU_PATH" ] || [ ! -d "$AU_PATH" ]; then
  echo "Error: ${AU_COMPONENT_NAME} not found. Build first:" >&2
  echo "  cargo truce build --au2 -p logicx-plugin" >&2
  find target -type d -name "*.component" 2>/dev/null || true
  exit 1
fi

if [ -z "$STANDALONE_PATH" ] || [ ! -d "$STANDALONE_PATH" ]; then
  echo "Error: ${STANDALONE_APP_NAME} not found. Stage standalone first:" >&2
  echo "  ./scripts/build-installer-pkg.sh --build-standalone" >&2
  find target/bundles -type d -name "*.app" 2>/dev/null || true
  exit 1
fi

echo "Using AU:         $AU_PATH"
echo "Using standalone: $STANDALONE_PATH"

mkdir -p "$OUT_DIR"
rm -rf "$OUT_DIR/$AU_COMPONENT_NAME" "$OUT_DIR/$STANDALONE_APP_NAME"
cp -R "$AU_PATH" "$OUT_DIR/$AU_COMPONENT_NAME"
cp -R "$STANDALONE_PATH" "$OUT_DIR/$STANDALONE_APP_NAME"

if [ "$SIGN_PLUGINS" = true ]; then
  echo "Ad-hoc signing plugin bundles..."
  xcrun codesign --force --sign - --deep "$OUT_DIR/$AU_COMPONENT_NAME"
  xcrun codesign --force --sign - --deep "$OUT_DIR/$STANDALONE_APP_NAME"
fi

(
  cd "$OUT_DIR"
  rm -f "$ZIP_NAME"
  zip -r "$ZIP_NAME" "$AU_COMPONENT_NAME" "$STANDALONE_APP_NAME"
)
echo "Created ${OUT_DIR}/${ZIP_NAME}"

rm -rf payload
mkdir -p payload/Library/Audio/Plug-Ins/Components
mkdir -p payload/Applications
cp -R "$OUT_DIR/$AU_COMPONENT_NAME" payload/Library/Audio/Plug-Ins/Components/
cp -R "$OUT_DIR/$STANDALONE_APP_NAME" payload/Applications/

pkgbuild \
  --root payload \
  --identifier "$PKG_ID" \
  --version "$PKG_VERSION" \
  --install-location / \
  "$OUT_DIR/$PKG_NAME"

rm -rf payload
echo "Created ${OUT_DIR}/${PKG_NAME} (version ${PKG_VERSION})"
echo "Install: sudo installer -pkg ${OUT_DIR}/${PKG_NAME} -target /"
echo "Or open the .pkg in Finder for a GUI install."
