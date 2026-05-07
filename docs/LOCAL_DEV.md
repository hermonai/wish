# Local Development Setup

This document covers running Wish entirely on your local machine — no
Hermon backend, no cloud login, no telemetry. Useful for:

- Trying Wish before signing up for Hermon
- Self-hosted / air-gapped environments
- Development against local LLMs (Ollama, llama.cpp, etc.)
- Testing UI changes without round-tripping to a backend

## Quick start (pure-local)

```bash
# Build and run wish-oss (the open-source variant, no proprietary
# upstream pieces).
cd ~/ClaudeProjects/wish
cargo run --bin wish-oss
```

That's it. Wish boots, the welcome page opens, and you can use the
terminal immediately — no login required.

## What works without login

| Feature | Local-only | Hermon-required |
|---------|------------|-----------------|
| Terminal (all shells) | ✅ | — |
| Block-based command UI | ✅ | — |
| Built-in SDLC agents (catalog) | ✅ | — |
| Settings → Built-in Agents page | ✅ | — |
| Local theme & keybinding customization | ✅ | — |
| **Agent invocation** (chat with an agent) | ⚠️ requires local LLM (see below) | ✅ via Hermon model routing |
| Conversation history | local file only | Synced across devices |
| Wish Drive | ⚠️ local file only (see below) | ✅ Synced |
| Cross-device session sync | ❌ | ✅ |
| Telemetry | ❌ (disabled) | Optional |

## Optional: connecting to a local Hermon backend

If you want to test the full Hermon integration without using
`wish.hermon.ai`:

```bash
# Terminal 1 — start the gateway (assumes ~/ClaudeProjects/hermon)
cd ~/ClaudeProjects/hermon
cargo run --bin hermon-gateway --release

# Terminal 2 — start the dashboard (Next.js)
cd ~/ClaudeProjects/hermon/dashboard
npm run dev

# Terminal 3 — start Wish pointing at local
HERMON_API_URL=http://localhost:8080 \
HERMON_DASHBOARD_URL=http://localhost:3000 \
cargo run --bin wish
```

The Hermon gateway listens on `:8080`, the dashboard on `:3000`. Override
ports with `HERMON_API_URL` or `WISH_API_URL`; local macOS bundles use the
canonical `wish://` callback scheme by default, with `WISH_URL_SCHEME`
available only when a special callback scheme is required. These URLs are
also documented in [`HERMON_ECOSYSTEM.md`](HERMON_ECOSYSTEM.md).

### Local-dev login steps

1. Launch the gateway + dashboard as above.
2. Open Wish; click **Settings → Account → Sign in**.
3. The system browser opens to `http://localhost:3000/login`.
4. Click **Sign up** (or **Sign in** if you already created a
   local-dev account).
5. The dashboard issues a session token and redirects to Wish via the
   `wish://auth-callback?token=...` URL scheme handler.
6. Wish stores the token in the OS keychain (`~/.wish/keystore` on
   non-keychain platforms) and reflects the logged-in state in the
   account panel.

Once logged in, agents, conversations, and Drive content sync to your
local Hermon instance.

## Local LLM (Ollama)

Wish ships with a typed `hermon_client` that knows how to talk to
Hermon's `/v1/ai` endpoint, but for **pure-local** AI inference you can
point it at an OpenAI-compatible local endpoint such as Ollama,
llama.cpp's `llama-server`, or LM Studio.

> [!NOTE]
> First-class Ollama integration in the **agent** runtime (so the
> built-in SDLC agents invoke a local model) is on the roadmap. The
> hermon_client::ai namespace is wire-compatible with OpenAI-style
> endpoints, so the plumbing is straightforward — see the open task in
> [`docs/AGENT_REGISTRY.md`](AGENT_REGISTRY.md) → "Natural next steps".
>
> Until that lands, local LLM use happens at the *settings* layer:
> point Wish's "AI provider" config at your local endpoint.

### Ollama setup

```bash
# Install Ollama (macOS shown — see https://ollama.com for Linux/Windows)
brew install ollama
brew services start ollama

# Pull a model (any OpenAI-compatible chat model works)
ollama pull llama3.2:3b      # ~2 GB, fast on M-series
ollama pull qwen2.5-coder:7b # ~4.5 GB, strong on code

# Verify the server is up
curl http://localhost:11434/api/tags
```

### Pointing Wish at Ollama

In `~/.wish/settings.toml`:

```toml
[ai.providers.ollama]
type = "openai_compatible"
base_url = "http://localhost:11434/v1"
api_key = "ollama"        # any non-empty string; Ollama doesn't check
default_model = "llama3.2:3b"

[ai]
default_provider = "ollama"
```

Restart Wish after editing `settings.toml`.

### Verifying

Open the Settings → Account page — under "AI" you should see the
configured Ollama provider listed and a small green dot indicating
connectivity.

## Local Wish Drive

Wish Drive's storage layer is currently network-only (it talks to
`/v1/drive/objects` on the Hermon backend). For pure-local mode there
are two viable paths:

### Path A: Use a local Hermon instance

Run the gateway as in the previous section. Drive content lives in
the gateway's database (SQLite by default in dev mode at
`~/.hermon/dev.db`). Backed up by your normal filesystem backup.

### Path B: File-system mode (planned)

A future Wish Drive backend will treat a local directory (default:
`~/.wish/drive/`) as the storage root, indexing it the same way the
network backend indexes server-side objects. Until that lands, the
left-panel "Wish Drive" tab will surface an empty state when no
backend is reachable.

> [!TIP]
> If the **Wish Drive** button isn't visible in the sidebar, it's
> hidden by default in fresh installs. Enable it in
> **Settings → Features → Vertical tabs** by toggling on the **Drive**
> chip in the toolbar configuration. The button reappears on the next
> render. See "Showing the Agent Conversations / Wish Drive buttons"
> below for details.

## Showing the Agent Conversations / Wish Drive buttons

These two surfaces are **tabs inside the Tools Panel**, not standalone
top-level buttons. The Tools Panel itself is one of the default
left-side chips in the vertical-tabs toolbar.

### Path: Tools Panel → Agent Conversations / Wish Drive

```
[ Tabs Panel ]  [ Tools Panel ▼ ]  [ Agent Management ]
                       │
                       └─ ┌──────────────────────────┐
                          │ Project explorer         │
                          │ Global search            │
                          │ Wish Drive            ←──┘ click here
                          │ Agent Conversations   ←──┘ or here
                          └──────────────────────────┘
```

### If you don't see the Tools Panel chip

The toolbar chips render only when **vertical tabs** are enabled. To
turn them on:

1. Open **Settings → Features**.
2. Toggle **Use vertical tabs** ON.
3. Reopen the workspace tab. The vertical-tabs sidebar appears on the
   left edge, with chips at the top: **Tabs Panel**, **Tools Panel**,
   **Agent Management** (defaults).

### If vertical tabs are on but Tools Panel is hidden

You may have set a custom chip configuration. In `~/.wish/settings.toml`:

```toml
[appearance.tabs]
header_toolbar_chip_selection = "default"  # restore the defaults
```

Or, to keep your custom layout but include Tools Panel:

```toml
[appearance.tabs.header_toolbar_chip_selection.custom]
left  = ["tabs_panel", "tools_panel", "agent_management"]
right = ["code_review", "notifications_mailbox"]
```

### If Tools Panel opens but Wish Drive / Conversations look empty

Pure-local mode (no Hermon login) means:

- **Agent Conversations** — shows an empty state. Conversations are
  persisted in the local SQLite database and listed here once you
  start chatting with an agent. (Agent invocation requires either a
  local LLM provider — see Ollama section above — or a Hermon login.)
- **Wish Drive** — shows an empty state until a Drive backend is
  configured. See the [Local Wish Drive](#local-wish-drive) section
  above for the two supported paths.

Both surfaces work fully once you sign in to Hermon (or run a local
Hermon instance).

## Settings file location

| Platform | Default path |
|----------|--------------|
| macOS / Linux (stable) | `~/.wish/settings.toml` |
| macOS / Linux (preview) | `~/.wish-preview/settings.toml` |
| Windows | `%APPDATA%/Wish/settings.toml` |

The first launch creates the directory + an empty `settings.toml`. The
**Welcome page** flag (`general.welcome_page_shown`) is also persisted
here.

## Troubleshooting

### "I don't see the welcome page on first launch"

Wish records that you've seen the welcome page in
`general.welcome_page_shown`. To re-show it:

1. **From the menu**: **Help → Show Welcome Page** opens it any time.
2. **From settings**: **Settings → About → Show Welcome Page** does
   the same.
3. **Force-reset first-launch state**: delete `~/.wish/settings.toml`
   (or just delete the `welcome_page_shown` line) and relaunch.

### "Login link doesn't open my local dashboard"

The "Already have an account? Log in" link uses
`AuthManager::sign_in_url()`, which by default targets
`https://wish.hermon.ai`. Override it for local-dev with the
`HERMON_DASHBOARD_URL` env var:

```bash
HERMON_DASHBOARD_URL=http://localhost:3000 cargo run --bin wish
```

### "AGI provider list is empty in Settings"

Pure-local mode without an explicit provider config yields an empty
list — that's expected. Add an Ollama provider as shown above, or
sign in to Hermon to use the cloud provider catalog.

### "Cargo build is slow / fails on first run"

The full Wish workspace has ~3,700 unit tests and a large dependency
graph. Initial builds take 8-15 minutes on M-series and 20-30
minutes on x86. Subsequent builds are incremental. If you hit
out-of-disk-space errors during the build, see the
`target/debug/incremental` cache — it can be safely deleted.
