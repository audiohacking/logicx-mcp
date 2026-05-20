# LogicX MCP — macOS setup

Follow [logic-pro-mcp SETUP](https://github.com/MongLong0214/logic-pro-mcp/blob/main/docs/SETUP.md) for full Logic integration. Priority items:

## Ollama

```bash
brew install ollama
ollama pull qwen3.5
```

In the plugin settings, set model to match your local tag (e.g. `qwen3.5`, `qwen3.5:latest`).

## MCU control surface (mixer — coming soon)

Logic → Control Surfaces → Setup → Add **Mackie Control** → In/Out: `LogicProMCP-MCU-Internal` (once our MCU channel ships).

## macOS permissions

- **Privacy & Security → Automation** — allow LogicX MCP / Logic Pro
- **Privacy & Security → Accessibility** — add Logic Pro and the standalone host if needed

## Example prompt

> Make a 4-bar techno loop in A minor at 140 BPM

Expected tool flow: `set_tempo` → `set_cycle_range` → `record_sequence` → `set_instrument` → `play`.
