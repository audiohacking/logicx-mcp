# LogicX MCP

Chat with **Logic Pro** from inside a **Truce AU v2 plugin**. A local **Ollama** model (e.g. `qwen3.5`) plans and runs DAW actions—tempo, tracks, MIDI, transport, mixer—through natural language.

<img width="400" alt="Screenshot 2026-05-21 at 10 50 50" src="https://github.com/user-attachments/assets/6ef94e9f-d1e0-4ffe-8b06-6c499a8b6570" />

### Status

- Very Experimental ⚠️ use at your own risk

## Install

**Requirements:** macOS 14+, Logic Pro 12+, [Ollama](https://ollama.com/)

```bash
ollama pull qwen3.5
./scripts/install-au.sh
```

Installs the **AU plugin** and **`LogicX MCP.app`** (companion control host). Logic control goes through that app—do not use `cargo truce install` alone.

**System installer (.pkg):**

```bash
./scripts/build-installer-pkg.sh --build --sign-plugins
sudo installer -pkg release-artefacts/LogicX-MCP-macOS-Installer.pkg -target /
```

## Required: Accessibility & Automation

**Nothing in Logic will work until this is done.** The plugin inside Logic delegates control to **`LogicX MCP.app`**. That bridge reads and drives Logic’s UI via macOS permissions.

### 1. Accessibility

1. **System Settings → Privacy & Security → Accessibility**
2. Add **LogicX MCP** (`~/Applications` or `/Applications`) if it is missing
3. Turn **LogicX MCP** **ON**

Needed for tempo, transport, track state, and most control.

### 2. Automation

1. **System Settings → Privacy & Security → Automation**
2. Select **LogicX MCP** (not `logicx-control-bridge`—bare binaries never appear here)
3. Enable **Logic Pro**
4. Enable **System Events**

If **LogicX MCP** is not listed yet, run once to trigger the prompt:

```bash
~/Applications/LogicX\ MCP.app/Contents/MacOS/logicx-control-bridge
```

Press **Ctrl+C** after the dialog, then enable the toggles above.

### 3. Relaunch & verify

1. Quit Logic Pro (**Cmd+Q**) and reopen
2. Load the **LogicX MCP** AU and send a command, e.g. *set tempo to 140*
3. In chat, run **`logic_system` → `permissions`**. Expect:

| Check | Value |
|-------|--------|
| `accessibility` | `true` |
| `tempo_control_ready` | `true` |
| `permission_subject` | `"LogicX MCP"` |

Tempo needs **Accessibility** only. Track creation, MIDI import, and menu fallbacks also need **Automation → System Events**.

Permission or control errors? Re-check toggles for **LogicX MCP**, then see [docs/SETUP.md](docs/SETUP.md) (MCU ports, reinstall, debug).

## Use in Logic

1. Insert **LogicX MCP** on any track (pass-through utility)
2. Open the plugin window
3. Ask in plain language, e.g. *Make a 4-bar techno loop in A minor at 140 BPM*

The agent calls built-in tools (`logic_transport`, `logic_tracks`, `logic_mixer`, …). Settings (⚙): Ollama URL (default `http://127.0.0.1:11434`) and model name.

**Before testing a new build:**

```bash
./scripts/reinstall-for-test.sh   # rebuild, install, restart bridge
```

Then quit and relaunch Logic Pro.

## Developers

```bash
cargo truce run -p logicx-plugin          # standalone UI
cargo test --workspace
./scripts/test-live.sh --ignored          # Logic + Ollama (after reinstall)
```

Contributor notes: [AGENTS.md](AGENTS.md) · Full setup: [docs/SETUP.md](docs/SETUP.md)

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
