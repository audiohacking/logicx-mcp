#!/usr/bin/env bash
# Rebuild and reinstall AU + LogicX MCP.app before manual or live Logic Pro testing.
# Agents: run this (or ./scripts/install-au.sh) before any in-Logic verification.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "=== LogicX MCP: reinstall for test ==="
"$ROOT/scripts/kill-bridge.sh"
"$ROOT/scripts/install-au.sh"

GIT_SHA="$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo dev)"
if ! git -C "$ROOT" diff --quiet 2>/dev/null || ! git -C "$ROOT" diff --cached --quiet 2>/dev/null; then
  GIT_SHA="${GIT_SHA}-dirty"
fi

echo ""
echo "=== Ready to test (build ${GIT_SHA}) ==="
echo "  1. Quit Logic Pro completely (Cmd+Q), then relaunch."
echo "  2. Reload the LogicX MCP AU — debug header should show build id ${GIT_SHA}."
echo "  3. Grant Accessibility to LogicX MCP if prompted."
echo ""
echo "Optional live cargo tests (standalone process, not AU):"
echo "  ./scripts/test-live.sh"
echo ""
