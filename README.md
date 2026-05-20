# LogicX MCP

Local-first Logic Pro assistant as a **Truce AU v2 plugin** with an embedded chat UI. Uses **Ollama** (e.g. `qwen3.5`) and the [Logic Pro MCP](https://github.com/MongLong0214/logic-pro-mcp) tool contract to control Logic from natural language.

## Quick start

### Prerequisites

- macOS 14+, Logic Pro 12+
- [Rust](https://rustup.rs/) 1.90+
- Xcode CLI tools
- [Ollama](https://ollama.com/) running locally

```bash
ollama pull qwen3.5
ollama serve   # if not already running
```

### Build & install

```bash
cargo truce install --au2 --user -p logicx-plugin
```

Standalone (for development without Logic):

```bash
cargo truce run -p logicx-plugin
```

Package locally (AU + standalone zip + `.pkg`):

```bash
./scripts/build-installer-pkg.sh --build --sign-plugins
```

### Usage in Logic Pro

1. Load **LogicX MCP** on any track (utility / pass-through).
2. Open the plugin window.
3. Type: *"Make a 4-bar techno loop in A minor at 140 BPM"*
4. The agent calls Logic tools (`logic_transport`, `logic_tracks`, …) via Ollama function calling.

Settings (⚙): Ollama URL (default `http://127.0.0.1:11434`) and model name.

## Architecture

```
Chat UI (egui) → logicx-agent (Ollama loop) → logicx-control (8 dispatchers) → Logic Pro
                     ↑
              logicx-core (system prompt + tool schemas)
```

Porting [MongLong0214/logic-pro-mcp](https://github.com/MongLong0214/logic-pro-mcp) channels (MCU, AX, CoreMIDI, …) is in progress. v0.1 includes:

- Full **system prompt** and **8 Ollama tool definitions**
- **SMF generation** for `record_sequence`
- **AppleScript** stubs for transport / health
- **Honest Contract** JSON responses

## Permissions

Grant **Automation** (control Logic Pro) and **Accessibility** when macOS prompts — required as more channels come online.

## CI & releases

- **PR CI:** `.github/workflows/ci.yml` — fmt, clippy, tests on `macos-14`
- **Release:** push tag `v*` → builds AU v2 + standalone, runs `scripts/build-installer-pkg.sh`, uploads zip + `.pkg` to GitHub Releases (ad-hoc signed, no certs required)

See [AGENTS.md](AGENTS.md) for contributor/agent guidance.

## License

MIT
