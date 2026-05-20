#!/usr/bin/env bash
# Build and stage LogicX MCP AU + standalone app + control bridge (shared by install-au.sh and pkg).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

AU_COMPONENT_NAME="LogicX MCP.component"
STANDALONE_APP_NAME="LogicX MCP.app"
STANDALONE_BIN="logicx-mcp-standalone"
BRIDGE_BIN="logicx-control-bridge"

# Populated by stage_all_bundles
STAGED_AU=""
STAGED_APP=""

stage_standalone_app_fallback() {
  local pkg_version="${1:-0.1.0}"
  local built="$REPO_ROOT/target/release/${STANDALONE_BIN}"
  local staged="$REPO_ROOT/target/bundles/${STANDALONE_APP_NAME}"

  if [ ! -f "$built" ]; then
    echo "Building standalone binary (truce app bundle missing)..." >&2
    cargo build --release -p logicx-plugin --features standalone
    built="$REPO_ROOT/target/release/${STANDALONE_BIN}"
  fi

  if [ ! -f "$built" ]; then
    echo "Error: standalone binary not found at $built" >&2
    exit 1
  fi

  echo "Staging fallback ${STANDALONE_APP_NAME}..." >&2
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
    <string>LogicX MCP</string>
    <key>CFBundleDisplayName</key>
    <string>LogicX MCP</string>
    <key>CFBundleIdentifier</key>
    <string>com.audiohacking.logicx-mcp.standalone</string>
    <key>CFBundleExecutable</key>
    <string>${STANDALONE_BIN}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleVersion</key>
    <string>${pkg_version}</string>
    <key>CFBundleShortVersionString</key>
    <string>${pkg_version}</string>
    <key>NSAppleEventsUsageDescription</key>
    <string>LogicX MCP controls Logic Pro via Automation (Apple Events).</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF
  echo "$staged"
}

embed_control_bridge() {
  local au_path="$1"
  local bridge_src="$REPO_ROOT/target/release/${BRIDGE_BIN}"
  local bridge_dst="$au_path/Contents/MacOS/${BRIDGE_BIN}"

  if [ ! -f "$bridge_src" ]; then
    echo "Error: control bridge not built at $bridge_src" >&2
    exit 1
  fi

  cp "$bridge_src" "$bridge_dst"
  chmod +x "$bridge_dst"

  if [ -f "$REPO_ROOT/scripts/patch-au-plist.sh" ]; then
    "$REPO_ROOT/scripts/patch-au-plist.sh" "$au_path"
  fi

  echo "  embedded bridge: $bridge_dst" >&2
}

patch_app_plist() {
  local app_path="$1"
  local plist="$app_path/Contents/Info.plist"
  if [ ! -f "$plist" ]; then
    return 0
  fi
  /usr/libexec/PlistBuddy -c "Delete :NSAppleEventsUsageDescription" "$plist" 2>/dev/null || true
  /usr/libexec/PlistBuddy -c "Add :NSAppleEventsUsageDescription string LogicX MCP controls Logic Pro via Automation (Apple Events)." "$plist"
}

embed_bridge_in_app() {
  local app_path="$1"
  local bridge_src="$REPO_ROOT/target/release/${BRIDGE_BIN}"
  local bridge_dst="$app_path/Contents/MacOS/${BRIDGE_BIN}"

  if [ -f "$bridge_src" ] && [ -d "$app_path/Contents/MacOS" ]; then
    cp "$bridge_src" "$bridge_dst"
    chmod +x "$bridge_dst"
    echo "  embedded bridge in app: $bridge_dst" >&2
  fi
}

# Truce's bundled standalone host does not pass through custom flags like --control-bridge.
# Overwrite with our cargo-built host (src/main.rs handles --control-bridge).
sync_standalone_host_binary() {
  local app_path="$1"
  local dst="$app_path/Contents/MacOS/${STANDALONE_BIN}"
  local built="$REPO_ROOT/target/release/${STANDALONE_BIN}"

  echo "Building standalone host (with --control-bridge support)..." >&2
  cargo build --release -p logicx-plugin --features standalone --bin "${STANDALONE_BIN}"

  if [ ! -f "$built" ]; then
    echo "Error: standalone host not found at $built" >&2
    exit 1
  fi

  cp "$built" "$dst"
  chmod 755 "$dst"
  echo "  synced standalone host: $dst" >&2
}

stage_all_bundles() {
  local pkg_version="${1:-}"
  if [ -z "$pkg_version" ]; then
    pkg_version=$(awk -F\" '/^version[[:space:]]*=/ { print $2; exit }' "$REPO_ROOT/Cargo.toml")
  fi

  cd "$REPO_ROOT"

  echo "Building control bridge..." >&2
  cargo build --release -p logicx-control-bridge

  echo "Building plugin bundles (AU + standalone)..." >&2
  cargo truce build -p logicx-plugin

  STAGED_AU=$(find target/bundles -name "$AU_COMPONENT_NAME" -type d 2>/dev/null | head -1 || true)
  STAGED_APP=$(find target/bundles -name "$STANDALONE_APP_NAME" -type d 2>/dev/null | head -1 || true)

  if [ -z "$STAGED_AU" ] || [ ! -d "$STAGED_AU" ]; then
    echo "Error: ${AU_COMPONENT_NAME} not found under target/bundles" >&2
    find target -type d -name "*.component" 2>/dev/null || true
    exit 1
  fi

  if [ -z "$STAGED_APP" ] || [ ! -d "$STAGED_APP" ]; then
    echo "Warning: truce did not produce ${STANDALONE_APP_NAME}; using fallback staging" >&2
    STAGED_APP=$(stage_standalone_app_fallback "$pkg_version")
  fi

  echo "Staging AU: $STAGED_AU" >&2
  echo "Staging app: $STAGED_APP" >&2

  embed_control_bridge "$STAGED_AU"
  embed_bridge_in_app "$STAGED_APP"
  sync_standalone_host_binary "$STAGED_APP"
  patch_app_plist "$STAGED_APP"

  export STAGED_AU STAGED_APP
}
