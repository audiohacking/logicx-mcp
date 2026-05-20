#!/usr/bin/env bash
# Mark the AU as not sandbox-safe so Logic Pro grants network + Automation rights.
set -euo pipefail

COMPONENT="${1:?Usage: patch-au-plist.sh path/to/LogicX MCP.component}"

PLIST="${COMPONENT}/Contents/Info.plist"
if [[ ! -f "$PLIST" ]]; then
  echo "Missing Info.plist: $PLIST" >&2
  exit 1
fi

/usr/libexec/PlistBuddy -c "Set :AudioComponents:0:sandboxSafe false" "$PLIST"
echo "Patched sandboxSafe=false in $PLIST"

EXEC="${COMPONENT}/Contents/MacOS/LogicX MCP"
if [[ -f "$EXEC" ]]; then
  codesign -s - -f "$EXEC"
fi
codesign -s - -f "$COMPONENT"
echo "Re-signed $COMPONENT"
