<a href="https://wish.hermon.ai">
    <img width="1024" alt="Wish Agentic Development Environment product preview" src="https://github.com/user-attachments/assets/9976b2da-2edd-4604-a36c-8fd53719c6d4" />
</a>

<p align="center">
  <a href="https://wish.hermon.ai">Website</a>
  ·
  <a href="https://www.hermon.ai">Hermon AI</a>
  ·
  <a href="https://github.com/hermonai/hermon">Hermon Backend</a>
</p>

> [!NOTE]
> Wish is derived from the open-source Warp project. Upstream Warp notices and license terms are preserved; see [docs/UPSTREAM_ATTRIBUTION.md](docs/UPSTREAM_ATTRIBUTION.md).

<h1>Wish</h1>

## About

Wish is Hermon AI's agentic development environment, derived from the open-source [Warp](https://github.com/warpdotdev/warp) client. Wish stays focused on the terminal, code workspace, agents, CLI agent hosting, code review, and local development workflows, with [Hermon](https://github.com/hermonai/hermon) as the remote backend/control-plane and [wishd](https://github.com/hermonai/wishd) as the trusted local daemon.

### Ecosystem

| Repo | Role |
|---|---|
| **wish** (this repo) | Rust agentic terminal (GPU-rendered, Warp-derived) |
| [wishcode](https://github.com/hermonai/wishcode) | Electron desktop AI coding agent |
| [hermon](https://github.com/hermonai/hermon) | Rust remote control plane (auth, orgs, policies, model routing, cells, analytics) |
| [wishd](https://github.com/hermonai/wishd) | Rust trusted local daemon (fs, git, process, terminal, indexing) |

### Hermon Integration

The `hermon_client` crate (`crates/hermon_client/`) provides a typed Rust HTTP client for the Hermon API, covering auth, AI model routing, sessions, orgs, and telemetry. This replaces the legacy Warp backend with Hermon's REST + SSE API surface.

## Installation

Wish distribution instructions will live at [wish.hermon.ai](https://wish.hermon.ai). For now, build from source using the local development commands below.

## Upstream Contributions Overview Dashboard

The upstream Warp project maintains [build.warp.dev](https://build.warp.dev) to:
- Watch thousands of Oz agents triage issues, write specs, implement changes, and review PRs
- View top contributors and in-flight features
- Track your own issues with GitHub sign-in
- Click into active agent sessions in a web-compiled terminal

## Licensing

WishUI (the `wishui_core` and `wishui` crates) is derived from WarpUI and licensed under the [MIT license](LICENSE-MIT).

The rest of the code in this repository is licensed under the [AGPL v3](LICENSE-AGPL).

## Open Source & Contributing

Wish's client codebase is open source and lives in this repository. This fork preserves upstream attribution while evolving the product for Hermon AI. For the full contribution flow inherited from upstream, read the [CONTRIBUTING.md](CONTRIBUTING.md) guide.

> [!TIP]
> **Chat with contributors and the Hermon AI team** in `#oss-contributors` — a good place for ad-hoc questions, design discussion, and pairing with maintainers.

### Issue to PR

Before filing, [search existing issues](https://github.com/hermonai/wish/issues) for your bug or feature request. If nothing exists, [file an issue](https://github.com/hermonai/wish/issues/new/choose) using our templates. Security vulnerabilities should be reported privately as described in [CONTRIBUTING.md](CONTRIBUTING.md#reporting-security-issues).

Once filed, a Hermon AI maintainer reviews the issue and may apply a readiness label: `ready-to-spec` signals the design is open for contributors to spec out, and `ready-to-implement` signals the design is settled and code PRs are welcome. Anyone can pick up a labeled issue — mention **@oss-maintainers** on an issue if you'd like it considered for a readiness label.

### Building the Repo Locally

To build and run Wish from source:

```bash
./script/bootstrap   # platform-specific setup
./script/run         # build and run Wish
./script/presubmit   # fmt, clippy, and tests
```

See [WARP.md](WARP.md) for the full engineering guide, including coding style, testing, and platform-specific notes.

## Joining the Team

Interested in Hermon AI? See [hermon.ai](https://www.hermon.ai).

## Support and Questions

1. Wish product information will live at [wish.hermon.ai](https://wish.hermon.ai).
2. Hermon AI company information lives at [hermon.ai](https://www.hermon.ai).
3. Upstream Warp documentation remains useful for inherited behavior while this rebrand is underway.

## Code of Conduct

We ask everyone to be respectful and empathetic. This fork currently preserves the upstream [Code of Conduct](CODE_OF_CONDUCT.md) while Hermon AI project governance is finalized.

## Open Source Dependencies

We'd like to call out a few of the open source dependencies that helped the upstream Warp project, and therefore Wish, get off the ground:

* [Tokio](https://github.com/tokio-rs/tokio)
* [NuShell](https://github.com/nushell/nushell)
* [Fig Completion Specs](https://github.com/withfig/autocomplete)
* [Warp Server Framework](https://github.com/seanmonstar/warp)
* [Alacritty](https://github.com/alacritty/alacritty)
* [Hyper HTTP library](https://github.com/hyperium/hyper)
* [FontKit](https://github.com/servo/font-kit)
* [Core-foundation](https://github.com/servo/core-foundation-rs)
* [Smol](https://github.com/smol-rs/smol)
