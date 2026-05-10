# Wish

Wish is the Hermon AI agentic development environment derived from the open-source Warp client. It keeps the fast native terminal, code workspace, agent workflows, code review surfaces, and WishUI foundation, while moving the product experience toward local-first AI development with Hermon as the optional cloud control plane.

![Wish workspace](docs/assets/wish-workspace.png)

Useful links:

- [Wish product site](https://wish.hermon.ai)
- [Hermon AI](https://www.hermon.ai)
- [Wish repository](https://github.com/hermonai/wish)
- [Hermon backend repository](https://github.com/hermonai/hermon)
- [Upstream Warp repository](https://github.com/warpdotdev/warp)

## Current Direction

Wish is a Warp-derived native terminal and agentic IDE focused on:

- Local-first terminal, editor, and code review workflows
- Built-in agent mode and CLI agent hosting
- Multi-agent orchestration for local and cloud execution
- Local model support, including Ollama when available
- Hermon backend integration for hosted auth, model routing, sync, governance, billing, and team workflows
- WishUI and WishUI Core as the native UI foundation

Wish should remain usable without backend login for local development. Cloud services should enhance the client, not block local terminal/editor work.

Storm, Finalverse, 3D scene views, spatial UI, and `wishui-3d` are separate future product work and are not part of this repository.

## Innovation Track

Wish is maintained as an advanced Hermon edition of the Warp codebase. Current innovation areas include:

- Hermon-branded onboarding, settings, menus, app icons, and product surfaces
- Local-first startup behavior with backend login deferred to cloud features
- Hermon Cloud and Hermon agent language replacing Oz/Warp cloud product surfaces
- Wish Drive and local workspace persistence aligned with Hermon identity
- Configurable Hermon backend URLs for local and hosted environments
- Local model discovery so Ollama-backed models can be used without cloud dependency
- Ongoing upstream sync from `hermonai/warp:master` while preserving Wish rebranding and Hermon-specific features

## Local Development

To build and run Wish from source:

```bash
./script/bootstrap
./script/run
./script/presubmit
```

For a direct debug build of the OSS binary:

```bash
cargo run --bin wish-oss
```

See [WISH.md](WISH.md) for engineering conventions, build notes, test guidance, and codebase orientation.

## Configuration

Wish supports local and hosted Hermon environments.

| Variable | Purpose |
| --- | --- |
| `HERMON_API_URL` | Hermon backend base URL for hosted services and local development overrides |
| `WISH_API_URL` | Wish client API base URL override where applicable |
| `OLLAMA_BASE_URL` | Optional Ollama base URL override, defaulting to the local Ollama service when available |
| `ANTHROPIC_API_KEY` | Enables Anthropic/Claude model access where configured |
| `OPENAI_API_KEY` | Enables OpenAI model access where configured |
| `GEMINI_API_KEY` | Enables Google Gemini model access where configured |
| `XAI_API_KEY` | Enables Grok model access where configured |

Cloud-backed model lists, auth, sync, billing, and team services should route through Hermon. Local terminal/editor use and local model discovery should not require a Hermon login.

## WishUI

WishUI is the native UI foundation for Wish:

- `crates/wishui-core` owns backend-neutral UI concepts such as app/entity model, elements, layout, input, actions, scene, rendering-neutral GPU metadata, and theme/tokens.
- `crates/wishui` owns concrete platform/rendering/window integration such as native windows, Metal, WGPU, text/glyph rendering, image/texture cache, and frame scheduling.

The current WishUI scope is terminal/editor/agent workspace UI. It does not include 3D or spatial UI.

## Hermon Backend

Hermon is the backend and control plane for Wish. It is responsible for:

- Auth and UASM
- Model routing
- Agent session sync
- Workspace sync
- Cloud task orchestration
- Skill registry
- Telemetry and audit
- Billing and entitlements
- Team governance

See also:

- [Hermon Backend Integration](docs/HERMON_BACKEND_INTEGRATION.md)
- [Wish/Hermon Protocol Boundary](docs/WISH_HERMON_PROTOCOL_BOUNDARY.md)

## Upstream Sync

Wish tracks upstream Warp closely because the upstream client is moving quickly. The preferred sync workflow is:

```bash
git fetch warp-upstream master
git merge --no-ff warp-upstream/master
cargo fmt --all
cargo check --workspace
```

During conflicts, accept upstream functional changes where possible, then preserve Wish product identity, Hermon backend boundaries, WishUI crate names, local-first behavior, and upstream legal attribution.

## Licensing and Attribution

Wish preserves upstream Warp legal notices and attribution. See [Upstream Attribution](docs/UPSTREAM_ATTRIBUTION.md).

- The client app is licensed under [AGPL v3](LICENSE-AGPL).
- WishUI and WishUI Core are licensed under [MIT](LICENSE-MIT).
- New original Wish work may carry Hermon AI copyright notices.
- Files derived from upstream Warp should preserve upstream notices.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening issues or pull requests. In short:

- Keep changes focused.
- Preserve upstream attribution and legal notices.
- Prefer local-first behavior when a backend is unavailable.
- Keep product-facing language Wish/Hermon, except in explicit upstream attribution or third-party dependency contexts.
- Run formatting and checks before submitting:

```bash
cargo fmt --all
cargo check --workspace
```

## Support and Security

- File bugs and feature requests in [GitHub Issues](https://github.com/hermonai/wish/issues).
- Report security issues privately via [GitHub Security Advisories](https://github.com/hermonai/wish/security/advisories/new) or [security@hermon.ai](mailto:security@hermon.ai).
- Use [SECURITY.md](SECURITY.md) for the disclosure policy.

## Selected Open Source Dependencies

Wish builds on a large Rust and systems ecosystem, including:

- [Tokio](https://github.com/tokio-rs/tokio)
- [NuShell](https://github.com/nushell/nushell)
- [Fig Completion Specs](https://github.com/withfig/autocomplete)
- [Warp Server Framework](https://github.com/seanmonstar/warp)
- [Alacritty](https://github.com/alacritty/alacritty)
- [Hyper HTTP library](https://github.com/hyperium/hyper)
- [FontKit](https://github.com/servo/font-kit)
- [Core-foundation](https://github.com/servo/core-foundation-rs)
- [Smol](https://github.com/smol-rs/smol)
