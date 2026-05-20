# LogicX MCP — macOS setup

Follow [logic-pro-mcp SETUP](https://github.com/MongLong0214/logic-pro-mcp/blob/main/docs/SETUP.md) for full Logic integration. Priority items:

## Ollama

```bash
brew install ollama
ollama pull qwen3.5
```

In the plugin settings, set **Ollama URL** (default `http://127.0.0.1:11434`) and model name. Remote Ollama URLs work the same way in standalone and AU.

### Why standalone works but AU might not (the blocker)

| | **Standalone (`LogicX MCP.app`)** | **AU plugin (inside Logic)** |
|---|---|---|
| Process | Its own macOS app | Dylib loaded into **Logic Pro** |
| Network | Normal app sockets | Whatever Logic allows plugins to do |
| Ollama client code | `/usr/bin/curl` → Ollama | **Same** `/usr/bin/curl` → Ollama |

There is no separate “AU client” we can magically give independent network access. The plugin **is not an app** — it runs as code inside Logic’s process. Standalone works because it **is** an app.

Both builds now use the **identical direct curl client**. If the AU debug log shows `Operation not permitted`, Logic is blocking outbound network from plugins. That is a host/ OS restriction, not an Ollama configuration issue.

**Workaround (companion app):** run `LogicX MCP.app` alongside Logic — the app holds the network connection; the AU becomes a Logic-embedded UI. (Companion bridge — next step if direct curl is blocked in Logic.)

Install options:

| Method | Installs | Scope |
|--------|----------|-------|
| `./scripts/install-au.sh` | AU + **LogicX MCP.app** + control bridge | `~/Library/...` (user) |
| `./scripts/build-installer-pkg.sh --build --sign-plugins` | Same bundles in `.pkg` + `.zip` | system (`/Library`, `/Applications`) |

## Testing in Logic Pro

**Always reinstall before testing** — agents and contributors must run:

```bash
./scripts/reinstall-for-test.sh
```

This kills stale bridge processes, rebuilds, and installs the AU + `LogicX MCP.app` to user Library paths. A plain `cargo build` does **not** update what Logic loads.

Then quit and relaunch Logic Pro. The AU debug header must show the current build id (`v<git-sha>` or `-dirty`).

| Script | Purpose |
|--------|---------|
| `./scripts/reinstall-for-test.sh` | **Default pre-test step** — kill bridge + install |
| `./scripts/install-au.sh` | Same install without extra messaging |
| `./scripts/kill-bridge.sh` | Stop bridge only (no rebuild) |
| `./scripts/test-live.sh` | Reinstall + `live_logic` / `live_smoke` cargo tests |

PKG install (recommended before testing in Logic):

```bash
./scripts/build-installer-pkg.sh --build --sign-plugins
sudo installer -pkg release-artefacts/LogicX-MCP-macOS-Installer.pkg -target /
```

The pkg installs **both**:
- `/Library/Audio/Plug-Ins/Components/LogicX MCP.component` (with embedded `logicx-control-bridge`)
- `/Applications/LogicX MCP.app` (companion control host — same model as logic-pro-mcp)

Dev refresh without pkg:

```bash
./scripts/install-au.sh   # user-scope AU + app in ~/Applications
```

### Where files are installed

| Install method | AU plugin | Companion app | Control bridge |
|----------------|-----------|---------------|----------------|
| `./scripts/install-au.sh` | `~/Library/Audio/Plug-Ins/Components/LogicX MCP.component` | `~/Applications/LogicX MCP.app` | embedded in both ( `logicx-control-bridge` ) |
| `.pkg` installer | `/Library/Audio/Plug-Ins/Components/LogicX MCP.component` | `/Applications/LogicX MCP.app` | embedded in both |

The AU does **not** need the `.app` running for Ollama (curl works in-process). Logic control delegates to **`logicx-control-bridge`**, which the AU auto-starts from the embedded copy inside the `.component` bundle.

### Plugin UI

- Header shows **build id** (git SHA) — confirms latest install loaded
- **Green ●** = Ollama reachable · **Red ●** = failed · **Yellow ◌** = checking
- **Debug** — curl command, host exe, bridge path, exact stderr (also in Console.app as `[LogicX MCP]`)

## MCU control surface (mixer)

Logic → **Control Surfaces → Setup** → Add **Mackie Control**:

| Logic field | Port |
|-------------|------|
| **Input** (Logic receives) | `LogicProMCP-MCU-Internal` |
| **Output** (Logic sends feedback) | `LogicProMCP-MCU-Feedback` |

Virtual MIDI ports created by LogicX MCP:

| Port | Purpose |
|------|---------|
| `LogicProMCP-MCU-Internal` | MCU commands → Logic (transport, faders) |
| `LogicProMCP-MCU-Feedback` | MCU feedback ← Logic (state cache, health) |
| `LogicX-MCP-Virtual` | CoreMIDI + MMC |
| `LogicProMCP-KeyCmd` | Key command fallback (CGEvent when MIDI Learn not configured) |
| `LogicX-MCP-Scripter` | Plugin parameter CC bridge (insert Scripter MIDI FX) |

## macOS permissions (required)

LogicX MCP controls Logic Pro via **Accessibility + Automation** (same as [logic-pro-mcp](https://github.com/MongLong0214/logic-pro-mcp)).

### AU plugin inside Logic Pro (recommended test path)

On current macOS, Logic loads AU plugins in a separate **XPC host** (`AUHostingServiceXPC_*`). Ollama/curl runs in that process; **Logic control delegates to the companion app** `LogicX MCP.app` (same model as [logic-pro-mcp](https://github.com/MongLong0214/logic-pro-mcp)).

`./scripts/install-au.sh` installs both the AU and **LogicX MCP.app**.

1. **System Settings → Privacy & Security → Accessibility** — enable **LogicX MCP**
2. **System Settings → Privacy & Security → Automation** — select **LogicX MCP** (not `logicx-control-bridge`) and optionally enable **Logic Pro** and **System Events**

**Important:** `logicx-control-bridge` is a bare helper binary embedded in the AU bundle. macOS **never lists bare binaries** under Automation — only proper `.app` bundles appear there. The AU auto-starts **`LogicX MCP.app --control-bridge`** for control operations.

**Tempo (`set_tempo`)** uses native Accessibility only (double-click control-bar slider, type BPM). **System Events is not required for tempo** — Accessibility on LogicX MCP is enough.

To populate the Automation list the first time, run once in Terminal (Ctrl+C to stop after the prompt):

```bash
~/Applications/LogicX\ MCP.app/Contents/MacOS/logicx-control-bridge
```

Alternatively, after `./scripts/install-au.sh`, the standalone host also supports:

```bash
~/Applications/LogicX\ MCP.app/Contents/MacOS/logicx-mcp-standalone --control-bridge
```

Do **not** use `open -a "LogicX MCP" --args --control-bridge` — Truce's default host rejects unknown flags unless the install script synced our host binary.

Run `permissions` in chat — for tempo you want `accessibility: true`, `tempo_control_ready: true`, and `permission_subject: "LogicX MCP"`.

After installing a new build, **quit and relaunch Logic Pro** so macOS rescans the plugin.

To manually stop stale bridge processes:

```bash
./scripts/kill-bridge.sh
```

### Standalone app

When using `LogicX MCP.app` (`cargo truce run`):

1. **Accessibility** — enable **LogicX MCP.app**
2. **Automation** — allow LogicX MCP to control **Logic Pro** and **System Events**

Run `logic_system` → `permissions` in chat to verify all flags are true.

Transport and tempo use native Accessibility on the bridge (double-click control-bar slider + HID keystrokes). System Events AppleScript is an optional fallback. Track creation and MIDI import may still use System Events menu paths when needed.

## Example prompt

> Make a 4-bar techno loop in A minor at 140 BPM

Expected tool flow: `set_tempo` → `set_cycle_range` → `record_sequence` → `set_instrument` → `play`.
