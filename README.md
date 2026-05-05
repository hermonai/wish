<h1 align="center">
  <a href="https://wish.hermon.ai">Wish</a>
</h1>

<p align="center">
  <strong>The agentic development environment from Hermon AI.</strong><br/>
  GPU-rendered terminal · built-in SDLC agents · cloud sync via Hermon
</p>

<p align="center">
  <a href="https://wish.hermon.ai">Website</a>
  ·
  <a href="https://www.hermon.ai">Hermon AI</a>
  ·
  <a href="https://github.com/hermonai/hermon">Hermon Backend</a>
  ·
  <a href="docs/HERMON_ECOSYSTEM.md">Ecosystem</a>
</p>

> [!NOTE]
> Wish is derived from the open-source Warp project. Upstream notices and
> license terms are preserved; see
> [docs/UPSTREAM_ATTRIBUTION.md](docs/UPSTREAM_ATTRIBUTION.md). The Hermon
> AI–specific code, branding, and integrations live in this repository.

## What is Wish?

Wish is a **terminal + agent IDE** for developers who want AI in the loop
without leaving the command line. It pairs a GPU-rendered terminal with a
fleet of purpose-built agents — planners, coders, reviewers, testers — and
keeps your conversations, workflows, and credentials in sync across
machines via the [Hermon](https://github.com/hermonai/hermon) control
plane.

Highlights:

- **Built-in SDLC agent suite** — 10 agents (planner, coder, reviewer,
  tester, debugger, deployer, documenter, refactorer, security,
  orchestrator) that ship with the binary; see Settings → Built-in Agents
  to browse them.
- **Hermon-backed sync** — sessions, conversations, and Wish Drive content
  follow you across devices when you sign in to Hermon.
- **Local-first** — works fully offline against built-in agents and a
  local LLM (Ollama or any OpenAI-compatible endpoint).
- **Multi-shell, multi-platform** — bash, zsh, fish, PowerShell on
  macOS / Linux / Windows.
- **Open source** — AGPLv3 (terminal app) + MIT (UI core); see [Licensing](#licensing).

## Ecosystem

| Repo | Role |
|------|------|
| **wish** *(this repo)* | Rust agentic terminal — GPU-rendered, agent-aware |
| [wishcode](https://github.com/hermonai/wishcode) | Electron desktop AI coding agent |
| [hermon](https://github.com/hermonai/hermon) | Rust remote control plane (auth, orgs, model routing, telemetry) |
| [wishd](https://github.com/hermonai/wishd) | Rust trusted local daemon (filesystem, git, process, terminal, indexing) |

See [`docs/HERMON_ECOSYSTEM.md`](docs/HERMON_ECOSYSTEM.md) for an
architectural overview.

## Hermon Integration

The [`hermon_client`](crates/hermon_client) crate provides a typed Rust
HTTP client for the Hermon API, covering:

- **Auth** — register, login, session management
- **AI** — model routing with SSE streaming
- **Agents** — CRUD, invoke, tool approval, system-agent listing
- **Conversations** — history, message streaming, fork/archive
- **Drive** — object storage and sync
- **Telemetry** — event ingestion
- **Orgs / Sessions** — user and device management

This replaces the legacy upstream backend with Hermon's REST + SSE API
surface. The crate is fully unit-tested (57 tests) and decoupled from UI.

## Installation

> Pre-built binaries will live at [wish.hermon.ai](https://wish.hermon.ai).
> Until then, build from source:

```bash
git clone https://github.com/hermonai/wish
cd wish
./script/bootstrap   # one-time platform setup (Rust toolchain, deps)
./script/run         # build and launch Wish
```

For the OSS variant (no proprietary upstream pieces):

```bash
cargo run --bin wish-oss
```

See [WISH.md](WISH.md) for the full engineering guide — coding style,
testing, platform-specific notes, and architecture deep-dives.

### Local development without Hermon

Want to run Wish entirely on your machine — no cloud login, no
backend? See [`docs/LOCAL_DEV.md`](docs/LOCAL_DEV.md) for:

- Pure-local mode quick start
- Ollama / local-LLM configuration
- Local Hermon backend setup (gateway + dashboard)
- Local-dev login flow
- Local Wish Drive options
- Showing the Agent Conversations / Wish Drive sidebar chips

## Configuration

| Variable | Default | Purpose |
|----------|---------|---------|
| `HERMON_API_URL` | `https://wish.hermon.ai` (or `http://localhost:8080` for local dev) | Hermon gateway URL |
| `HERMON_DASHBOARD_URL` | `https://wish.hermon.ai` (or `http://localhost:3000` for local dev) | Dashboard URL |
| `WISH_LOG` | `info` | Tracing log level |

Per-user configuration lives at `~/.wish/settings.toml`. The first launch
shows a Welcome page; you can re-open it any time via **Help → Show
Welcome Page** or **Settings → About**.

## Licensing

- **WishUI** (the `wishui_core` and `wishui` crates) is derived from
  WarpUI and licensed under the [MIT license](LICENSE-MIT).
- **Everything else** in this repository is licensed under
  [AGPL v3](LICENSE-AGPL).

## Open Source & Contributing

Wish's terminal client is fully open source. This fork preserves upstream
attribution while evolving the product for Hermon AI. For the contribution
flow inherited from upstream, read [CONTRIBUTING.md](CONTRIBUTING.md).

> [!TIP]
> **Chat with the Hermon AI team and contributors** on
> [Slack](https://www.hermon.ai/slack) — `#wish-contributors` is the best
> channel for design discussion, pairing, and ad-hoc questions.

### Issue → spec → PR

Before filing, [search existing issues](https://github.com/hermonai/wish/issues).
If nothing matches, [open a new issue](https://github.com/hermonai/wish/issues/new/choose)
using the templates. Security vulnerabilities should be reported privately —
see [CONTRIBUTING.md#reporting-security-issues](CONTRIBUTING.md#reporting-security-issues).

A maintainer triages each issue and may apply a readiness label:

- `ready-to-spec` — design is open for contributors to spec
- `ready-to-implement` — design is settled, code PRs welcome

Mention **@oss-maintainers** if you'd like an issue considered for a
readiness label.

### Building from source

```bash
./script/bootstrap   # platform-specific setup (Rust, deps)
./script/run         # build and run Wish
./script/presubmit   # fmt, clippy, and tests
```

Cross-platform notes, agent runtime architecture, and the Hermon API
surface are all documented in [WISH.md](WISH.md) and the
[`docs/`](docs/) directory.

## Joining the Team

Curious about Hermon AI? Visit [hermon.ai](https://www.hermon.ai).

## Support

1. **Wish product** — [wish.hermon.ai](https://wish.hermon.ai)
2. **Hermon AI company** — [hermon.ai](https://www.hermon.ai)
3. **Source code & issues** — [github.com/hermonai/wish](https://github.com/hermonai/wish)
4. **Inherited Warp documentation** — still useful for unmodified upstream
   behavior; see the upstream repo for reference.

## Code of Conduct

We ask everyone to be respectful and empathetic. This fork currently
preserves the upstream
[Code of Conduct](CODE_OF_CONDUCT.md) while Hermon AI project governance
is finalized.

## Open Source Dependencies

A non-exhaustive shout-out to the open-source projects that made Wish (and
upstream Warp before it) possible:

- [Tokio](https://github.com/tokio-rs/tokio) — async runtime
- [NuShell](https://github.com/nushell/nushell) — shell parser
- [Fig Completion Specs](https://github.com/withfig/autocomplete)
- [Alacritty](https://github.com/alacritty/alacritty) — terminal emulator
- [Hyper](https://github.com/hyperium/hyper) — HTTP
- [FontKit](https://github.com/servo/font-kit)
- [Core-Foundation](https://github.com/servo/core-foundation-rs)
- [Smol](https://github.com/smol-rs/smol)

---

<sub>
Wish is derived from <a href="https://github.com/warpdotdev/warp">Warp</a>.
Upstream attribution lives in
<a href="docs/UPSTREAM_ATTRIBUTION.md">UPSTREAM_ATTRIBUTION.md</a>.
</sub>
