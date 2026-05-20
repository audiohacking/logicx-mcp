#!/usr/bin/env bash
# logic-pro-mcp parity unit tests (no Logic Pro required).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "=== logic-pro-mcp parity tests ==="
cargo test -p logicx-control --test logic_pro_mcp_parity -- --nocapture
cargo test -p logicx-control --lib -- --nocapture
cargo test -p logicx-core --lib -- --nocapture

echo ""
echo "=== Parity tests passed ==="
