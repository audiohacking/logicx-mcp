# AGENTS.md — LogicX MCP

Guidance for AI agents and contributors working in this repository.

## Project summary

**LogicX MCP** is a macOS-only [Truce](https://github.com/truce-audio/truce) **AU v2** plugin with a standalone app and an embedded **egui chat UI**. It connects to **local Ollama** (default model: `qwen3.5`) and controls **Logic Pro** using the tool contract from [MongLong0214/logic-pro-mcp](https://github.com/MongLong0214/logic-pro-mcp).

- **Vendor:** `audiohacking` (`com.audiohacking` in `truce.toml`)
- **Target platform:** Apple Silicon macOS only (no Linux/Windows plugin formats)
- **Reference MCP server:** MongLong0214/logic-pro-mcp (primary), koltyj/logic-pro-mcp (minimal fallback ideas)

## Repository layout

```
logicx-mcp/
├── AGENTS.md                 ← this file
├── truce.toml                ← vendor + plugin metadata (source of truth for bundles)
├── Cargo.toml                ← workspace root
├── crates/
│   ├── logicx-core/          ← system prompt, Ollama tool schemas, shared types
│   ├── logicx-control/       ← Logic executor + macOS channels (MIDI/SMF/AppleScript…)
│   ├── logicx-agent/         ← Ollama client + tool-calling loop
│   └── logicx-plugin/      ← Truce plugin (AU + standalone + chat editor)
├── scripts/
│   └── build-installer-pkg.sh  ← audiohacking-style .pkg + zip (see aitroce-vst)
├── docs/
│   ├── SETUP.md              ← Ollama + Logic permissions + MCU setup
│   └── ARCHITECTURE.md       ← design notes
└── .github/workflows/
    ├── ci.yml                ← PR / main CI
    └── release.yml           ← tag v* → GitHub Release artifacts
```

## Architecture (short)

```
Chat UI (egui, GUI thread)
    → logicx-agent (Ollama /api/chat + tools, worker thread)
    → logicx-control (8 dispatchers → channel router)
    → Logic Pro (MCU, AX, CoreMIDI, AppleScript, …)
```

**Hard rules**

1. **Never** run Ollama, MCP, or Accessibility work on the audio realtime thread. `PluginLogic::process` is pass-through only.
2. **Reads** use MCP resources (`logic://…`); **writes** use the 8 tools — same contract as logic-pro-mcp.
3. **MCU control surface** is required for reliable mixer control (see `docs/SETUP.md`).
4. Keep the **8-tool surface** (~3k context tokens). Do not explode into 100+ MCP tools.

## Truce / build conventions

| Item | Value |
|------|--------|
| Truce tag | `v0.45.4` (pinned in `crates/logicx-plugin/Cargo.toml`) |
| Default features | `au` (AU v2), `standalone` |
| Plugin crate | `logicx-plugin` (`-p logicx-plugin`) |
| AU v2 bundle | `LogicX MCP.component` in `target/bundles/` |
| Standalone bundle | `LogicX MCP.app` in `target/bundles/` |

Common commands:

```bash
cargo test --workspace
cargo truce run -p logicx-plugin                    # standalone dev
cargo truce build --au2 -p logicx-plugin           # ad-hoc sign OK, no Xcode/certs required
./scripts/install-au.sh                             # user install + sandboxSafe=false (Logic + Ollama)
./scripts/reinstall-for-test.sh                     # kill bridge + install-au (REQUIRED before Logic testing)
./scripts/test-live.sh                              # reinstall + live_logic (+ live_smoke if Ollama up)
cargo truce install --au2 --user -p logicx-plugin # plain install (no Ollama in AU — use install-au.sh)
./scripts/build-installer-pkg.sh --build --sign-plugins
```

### Before testing in Logic Pro (required for agents)

**Always reinstall the latest AU + app before manual or in-Logic verification.** Code changes are not loaded until bundles are rebuilt and copied to `~/Library/...` and Logic Pro is restarted.

```bash
./scripts/reinstall-for-test.sh   # preferred: kill stale bridge + full install
# or
./scripts/install-au.sh
```

Then:

1. **Quit Logic Pro** completely (Cmd+Q), relaunch.
2. Confirm the AU debug header shows the expected **build id** (git SHA, `-dirty` if uncommitted changes).
3. Run `./scripts/kill-bridge.sh` only if you suspect a stale control bridge without reinstalling.

Do **not** assume `cargo build` alone updates the AU Logic loads — only `install-au.sh` / `reinstall-for-test.sh` deploy to the plugin paths.

AU v2 builds with **ad-hoc signing** (`-`) — no Developer ID or Xcode required. We intentionally do **not** ship AU v3 (appex bundles need a real signing identity + team ID).

## Channel porting status

Reference: [MongLong0214/logic-pro-mcp](https://github.com/MongLong0214/logic-pro-mcp) (~105 routed ops, native Swift AX, MCU feedback).

| Area | Status |
|------|--------|
| 8-tool MCP contract + routing table | Done (~105 ops) |
| CoreMIDI + MMC + SMF `record_sequence` | Done |
| MCU encode + bidirectional feedback cache | Done |
| Accessibility (tracks, library, import) | Done via native AX + AppleScript |
| CGEvent fallbacks + `set_tempo` | Done |
| AppleScript project lifecycle + State B HC wrap | Done |
| AU control bridge (companion app) | Done — logic-pro-mcp process model |
| MIDIKeyCommands (true CC on ch 16) | Done — approval gate + ~50 ops |
| StatePoller + `logic://` resource cache | Done — background AX supplementary poll |
| Operator approval gates | Done — KeyCmd + Scripter |
| Scripter CC bridge | Stub |
| Injectable AX test harness (FakeAXRuntime) | Not started |
| Full reference Swift test matrix (~99 files) | Partial — consolidated in `logic_pro_mcp_parity.rs` |

Port order for remaining parity:

1. Scripter CC bridge + plugin-param routing
2. Injectable AX/CGEvent runtime for offline channel tests
3. Expand live E2E coverage (`scripts/test-live.sh --ignored`)

## System prompt

The Ollama system prompt lives in `crates/logicx-core/src/prompt.rs` (`SYSTEM_PROMPT`). When changing tool behavior, update:

- `prompt.rs` — agent instructions + workflow examples
- `tools.rs` — Ollama function schemas
- `executor.rs` — runtime dispatch

Canonical example user request: *"Make a 4-bar techno loop in A minor at 140 BPM"* → `set_tempo` → `set_cycle_range` → `record_sequence` → `set_instrument` → `play`.

## CI / release (for agents)

- **PR CI** (`.github/workflows/ci.yml`): `fmt`, `clippy`, `test` on `macos-14`. No signing secrets required.
- **Release** (`.github/workflows/release.yml`): triggered on `v*` tags. Builds AU v2 + standalone via `scripts/build-installer-pkg.sh` (ad-hoc sign) and uploads zip + `.pkg` to GitHub Releases. **No signing secrets required.**

Installer pattern matches [audiohacking/aitroce-vst](https://github.com/audiohacking/aitroce-vst) (`scripts/build-installer-pkg.sh`).

## Permissions (macOS)

Document in PRs when touching control channels:

- **Automation** — control Logic Pro
- **Accessibility** — track/mixer reads, SMF import UI
- **Microphone** — standalone only if audio input is added later

## What not to do

- Do not add Linux/Windows plugin formats to default features.
- Do not commit `.cargo/config.toml` (signing identities are local).
- Do not commit secrets or `.env` files.
- Do not expand scope into unrelated Truce DSP unless asked.
- Do not create git commits unless the user explicitly requests it.

## Useful links

- [MongLong logic-pro-mcp API](https://github.com/MongLong0214/logic-pro-mcp/blob/main/docs/API.md)
- [MongLong ARCHITECTURE](https://github.com/MongLong0214/logic-pro-mcp/blob/main/docs/ARCHITECTURE.md)
- [Truce docs](https://truce.audio/)
