#!/usr/bin/env bash
# Clean user install: AU + standalone app + control bridge (dev/testing).
# Agents: prefer ./scripts/reinstall-for-test.sh before Logic Pro verification.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/stage-bundles.sh
source "$ROOT/scripts/stage-bundles.sh"

INSTALLED="$HOME/Library/Audio/Plug-Ins/Components/LogicX MCP.component"
AU_CACHE="$HOME/Library/Caches/AudioUnitCache"
APP_INSTALLED="$HOME/Applications/LogicX MCP.app"

GIT_SHA="$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo dev)"
if ! git -C "$ROOT" diff --quiet 2>/dev/null || ! git -C "$ROOT" diff --cached --quiet 2>/dev/null; then
  GIT_SHA="${GIT_SHA}-dirty"
fi
echo "=== LogicX MCP install (build ${GIT_SHA}) ==="

echo ""
echo "1. Removing old AU and clearing cache..."
"$ROOT/scripts/kill-bridge.sh" || true
pkill -f logicx-ollama-proxy 2>/dev/null || true
rm -rf "$INSTALLED"
rm -rf "$AU_CACHE"
echo "   Removed: $INSTALLED"
echo "   Cleared: $AU_CACHE"

echo ""
echo "2. Building and staging bundles..."
stage_all_bundles

echo ""
echo "3. Installing AU (user)..."
cargo truce install --au2 --user -p logicx-plugin --no-build
# Re-embed bridge after truce install copies fresh component
embed_control_bridge "$INSTALLED"

echo ""
echo "4. Installing standalone app (control bridge host)..."
mkdir -p "$(dirname "$APP_INSTALLED")"
rm -rf "$APP_INSTALLED"
cp -R "$STAGED_APP" "$APP_INSTALLED"
embed_bridge_in_app "$APP_INSTALLED"
echo "   Installed: $APP_INSTALLED"

PLUGIN_BIN="$INSTALLED/Contents/MacOS/LogicX MCP"
BRIDGE_DST="$INSTALLED/Contents/MacOS/logicx-control-bridge"
echo ""
echo "=== Installed ==="
echo "  AU:    $INSTALLED"
echo "  App:   $APP_INSTALLED"
echo "  Build: ${GIT_SHA}"
echo "  AU SHA256:     $(shasum -a 256 "$PLUGIN_BIN" | awk '{print $1}')"
echo "  Bridge SHA256: $(shasum -a 256 "$BRIDGE_DST" | awk '{print $1}')"
echo ""
echo "For system-wide install use:"
echo "  ./scripts/build-installer-pkg.sh --build --sign-plugins"
echo "  sudo installer -pkg release-artefacts/LogicX-MCP-macOS-Installer.pkg -target /"
echo ""
echo "IMPORTANT: Quit Logic Pro completely, then relaunch."
echo "Grant Accessibility to LogicX MCP (required for tempo)."
echo "Optional Automation: System Settings → Automation → LogicX MCP (NOT logicx-control-bridge)."
echo "First-time Automation prompt:"
echo "  ~/Applications/LogicX\\ MCP.app/Contents/MacOS/logicx-control-bridge"
echo "  (System Settings shows the process name from permissions tool output)"
echo "Companion app (optional): $APP_INSTALLED"
