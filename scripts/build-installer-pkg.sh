#!/usr/bin/env bash
# Build a macOS .pkg installer for LogicX MCP (AU v2 + standalone app + control bridge).
#
# Usage:
#   ./scripts/build-installer-pkg.sh [--build] [--sign-plugins] [--version 0.1.0]
#   ./scripts/build-installer-pkg.sh --au path/to/LogicX\ MCP.component --standalone path/to/LogicX\ MCP.app
#
# Output:
#   release-artefacts/logicx-mcp-<version>-macos-au-standalone.zip
#   release-artefacts/LogicX-MCP-macOS-Installer.pkg
#
# Installs to:
#   /Library/Audio/Plug-Ins/Components/LogicX MCP.component  (+ logicx-control-bridge)
#   /Applications/LogicX MCP.app

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

# shellcheck source=scripts/stage-bundles.sh
source "$SCRIPT_DIR/stage-bundles.sh"

AU_COMPONENT_NAME="LogicX MCP.component"
STANDALONE_APP_NAME="LogicX MCP.app"
PKG_ID="com.audiohacking.logicx-mcp"
OUT_DIR="release-artefacts"
PKG_NAME="LogicX-MCP-macOS-Installer.pkg"

DO_BUILD=false
SIGN_PLUGINS=false
PKG_VERSION=""
AU_PATH=""
STANDALONE_PATH=""

while [ $# -gt 0 ]; do
  case "$1" in
    --build)        DO_BUILD=true; shift ;;
    --build-standalone) DO_BUILD=true; shift ;; # legacy alias
    --sign-plugins) SIGN_PLUGINS=true; shift ;;
    --version)      PKG_VERSION="$2"; shift 2 ;;
    --au)           AU_PATH="$2"; shift 2 ;;
    --standalone)   STANDALONE_PATH="$2"; shift 2 ;;
    -h|--help)
      sed -n '1,20p' "$0"
      exit 0
      ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

if [ -z "$PKG_VERSION" ]; then
  PKG_VERSION=$(awk -F\" '/^version[[:space:]]*=/ { print $2; exit }' Cargo.toml)
fi
ZIP_NAME="logicx-mcp-${PKG_VERSION}-macos-au-standalone.zip"

if [ "$DO_BUILD" = true ]; then
  stage_all_bundles "$PKG_VERSION"
  AU_PATH="${STAGED_AU:-}"
  STANDALONE_PATH="${STAGED_APP:-}"
fi

if [ -z "$AU_PATH" ]; then
  AU_PATH=$(find target/bundles -name "$AU_COMPONENT_NAME" -type d 2>/dev/null | head -1 || true)
fi

if [ -z "$STANDALONE_PATH" ]; then
  STANDALONE_PATH=$(find target/bundles -name "$STANDALONE_APP_NAME" -type d 2>/dev/null | head -1 || true)
fi

if [ -z "$AU_PATH" ] || [ ! -d "$AU_PATH" ]; then
  echo "Error: ${AU_COMPONENT_NAME} not found. Run with --build or:" >&2
  echo "  ./scripts/build-installer-pkg.sh --build" >&2
  exit 1
fi

if [ -z "$STANDALONE_PATH" ] || [ ! -d "$STANDALONE_PATH" ]; then
  echo "Error: ${STANDALONE_APP_NAME} not found. Run with --build." >&2
  exit 1
fi

# Ensure bridge is embedded even when using prebuilt paths.
if [ ! -f "$AU_PATH/Contents/MacOS/logicx-control-bridge" ]; then
  echo "Embedding control bridge into AU..." >&2
  cargo build --release -p logicx-control-bridge 2>/dev/null || cargo build --release -p logicx-control-bridge
  embed_control_bridge "$AU_PATH"
fi
embed_bridge_in_app "$STANDALONE_PATH" 2>/dev/null || true

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
echo ""
echo "=== PKG ready ==="
echo "  ${OUT_DIR}/${PKG_NAME} (version ${PKG_VERSION})"
echo "  Installs:"
echo "    /Library/Audio/Plug-Ins/Components/${AU_COMPONENT_NAME}"
echo "    /Applications/${STANDALONE_APP_NAME}"
echo ""
echo "Install: sudo installer -pkg ${OUT_DIR}/${PKG_NAME} -target /"
echo "Or open the .pkg in Finder."
