#!/usr/bin/env bash
# Reinstall latest bundles, then run ignored live integration tests.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

"$ROOT/scripts/reinstall-for-test.sh"

echo "=== Running live_logic tests ==="
cargo test -p logicx-control --test live_logic -- --ignored --nocapture

if pgrep -x ollama >/dev/null 2>&1 && curl -sf http://127.0.0.1:11434/api/tags >/dev/null 2>&1; then
  echo ""
  echo "=== Running live_smoke tests (Ollama detected) ==="
  cargo test -p logicx-agent --test live_smoke -- --ignored --nocapture
else
  echo ""
  echo "Skipping live_smoke (Ollama not running at http://127.0.0.1:11434)."
fi
