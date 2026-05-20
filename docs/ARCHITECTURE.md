# Architecture

See [AGENTS.md](../AGENTS.md) for the canonical overview agents should read first.

## Layers

| Layer | Crate | Responsibility |
|-------|-------|----------------|
| UI | `logicx-plugin` | egui chat, Ollama settings, thread-safe state via `StateBinding` |
| Agent | `logicx-agent` | Ollama `/api/chat`, tool loop, event stream to UI |
| Contract | `logicx-core` | `SYSTEM_PROMPT`, 8 tool definitions, `HonestResult` types |
| Control | `logicx-control` | `LogicExecutor` → macOS channels |

## Threading

- **Audio thread:** pass-through in `process()`.
- **GUI thread:** egui editor, `StateBinding` updates.
- **Worker thread:** `run_agent()` — blocking HTTP to Ollama + tool execution.

## MCP contract

Aligned with [logic-pro-mcp](https://github.com/MongLong0214/logic-pro-mcp):

- **8 tools:** `logic_transport`, `logic_tracks`, `logic_mixer`, `logic_midi`, `logic_edit`, `logic_navigate`, `logic_project`, `logic_system`
- **Resources:** `logic://transport/state`, `logic://tracks`, … (reads only; resource polling planned)

## Bundles (Truce)

From `truce.toml` plugin name **LogicX MCP**:

| Artifact | Path under `target/bundles/` | Install location (.pkg) |
|----------|------------------------------|-------------------------|
| AU v2 | `LogicX MCP.component` | `/Library/Audio/Plug-Ins/Components/` |
| Standalone | `LogicX MCP.app` | `/Applications/` |

Built with `cargo truce build --au2` and standalone staging in `scripts/build-installer-pkg.sh`.
