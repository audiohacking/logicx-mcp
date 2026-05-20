#!/usr/bin/env bash
# Stop all LogicX MCP control-bridge processes and remove IPC artifacts.
set -euo pipefail

SUPPORT="${HOME}/Library/Application Support/LogicX MCP"

terminate() {
  local sig="$1"
  shift
  local pids="$*"
  [ -n "$pids" ] || return 0
  # shellcheck disable=SC2086
  kill "-$sig" $pids 2>/dev/null || true
}

collect_pids() {
  {
    pgrep -f logicx-control-bridge 2>/dev/null || true
    pgrep -f 'logicx-mcp-standalone.*--control-bridge' 2>/dev/null || true
    if [ -f "$SUPPORT/control-bridge.pid" ]; then
      cat "$SUPPORT/control-bridge.pid"
    fi
  } | tr ' ' '\n' | grep -E '^[0-9]+$' | sort -u
}

PIDS="$(collect_pids | tr '\n' ' ' | xargs echo 2>/dev/null || true)"

if [ -n "$PIDS" ]; then
  echo "Stopping control bridge PIDs: $PIDS"
  terminate 15 $PIDS
  sleep 0.3
  # SIGKILL survivors
  ALIVE=""
  for pid in $PIDS; do
    if kill -0 "$pid" 2>/dev/null; then
      ALIVE="$ALIVE $pid"
    fi
  done
  if [ -n "$ALIVE" ]; then
    echo "Force-killing: $ALIVE"
    terminate 9 $ALIVE
  fi
else
  echo "No control bridge processes found."
fi

rm -f "$SUPPORT/control.sock" "$SUPPORT/control-bridge.pid"
echo "Removed control.sock and control-bridge.pid (if present)."
