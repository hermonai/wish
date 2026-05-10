# Contributing to Wish

Thanks for helping improve Wish. This guide covers the contribution flow for the Hermon AI Wish client repository.

## Principles

- Wish is a Warp-derived project. Preserve upstream license notices, copyright notices, and attribution.
- Product-facing language should say Wish, WishUI, Hermon AI, and Hermon unless a file is explicitly about upstream Warp attribution or a third-party dependency.
- Keep changes buildable and tightly scoped.
- Preserve local-first behavior: the client should not require backend login for local terminal, editor, and workspace use.
- Do not add Storm, Finalverse, 3D scene views, spatial UI, or `wishui-3d` to this repository.

## Filing Issues

Search [existing Wish issues](https://github.com/hermonai/wish/issues) before filing a new one.

A good bug report includes:

- A clear title and summary.
- Steps to reproduce.
- Expected vs. actual behavior.
- Wish version from `Settings -> About`.
- OS and shell details.
- Logs, screenshots, or screen recordings when relevant.

Feature requests should describe the user-facing problem first. Include the workflow, the current limitation, and any constraints that matter.

Security vulnerabilities must not be filed publicly. See [SECURITY.md](SECURITY.md).

## Pull Requests

Before opening a PR:

1. Branch from `master`.
2. Keep the PR focused on one logical change.
3. Add tests when the change affects behavior.
4. Run the relevant checks.
5. Include screenshots or a short video for user-visible UI changes.
6. Explain how the change preserves Wish branding and upstream attribution when those are relevant.

The default validation set is:

```bash
cargo fmt --all
cargo check --workspace
```

For broader validation, run:

```bash
cargo test --workspace --no-fail-fast
```

## Code Style

- Prefer existing local patterns over new abstractions.
- Use `rg` for repository search.
- Keep Rust imports readable and prefer imports over long path qualifiers.
- Prefer exhaustive `match` arms when future enum variants should be surfaced by the compiler.
- Name context parameters `ctx` and place them last when following established local style.
- Keep comments short and useful; avoid restating the obvious.

For detailed engineering guidance, see [WISH.md](WISH.md).

## UI Changes

Wish UI work should follow existing WishUI patterns:

- Use shared UI components and themes before creating one-off styling.
- Keep text compact and product-facing copy Wish-branded.
- Use the Hermon/Wish assets already bundled in `app/assets/bundled`.
- Verify user-visible changes with screenshots or a local run when practical.
- Do not introduce 3D or spatial concepts into WishUI in this phase.

## Feature Flags

Feature flags live in the inherited feature flag system. When adding one:

- Add the Cargo feature.
- Add the `FeatureFlag` variant in `crates/wish_features/src/lib.rs`.
- Add the runtime feature registration in `app/src/lib.rs`.
- Prefer runtime checks such as `FeatureFlag::YourFlag.is_enabled()` unless the code cannot compile without a compile-time gate.
- Remove flags and dead branches once the rollout is complete.

## Specs

Larger product or architecture changes should have specs under `specs/`.

- `product.md` describes user-facing behavior and testable invariants.
- `tech.md` describes the implementation plan, modules touched, risks, and validation.

Keep Storm/Finalverse ideas in separate future planning spaces, not in Wish implementation specs.

## Backend Changes

Hermon is the backend/control plane for Wish. Backend-facing client work should:

- Preserve existing upstream API behavior until Hermon-compatible replacements exist.
- Document `HERMON_API_URL` and `WISH_API_URL` when new configuration is introduced.
- Keep auth/model routing/session sync/team governance work behind clear boundaries.
- Avoid making local startup depend on remote credentials.

## Licensing

Do not delete or rewrite license files. Do not blindly replace legal text.

- Preserve upstream Warp notices in derived files.
- Add Hermon AI modification notices only where appropriate.
- Keep [docs/UPSTREAM_ATTRIBUTION.md](docs/UPSTREAM_ATTRIBUTION.md) accurate when rebranding work changes the repository identity or boundaries.

## Getting Help

- Use [GitHub Issues](https://github.com/hermonai/wish/issues) for bugs and feature requests.
- Use [GitHub Security Advisories](https://github.com/hermonai/wish/security/advisories/new) or [security@hermon.ai](mailto:security@hermon.ai) for private security reports.
- Mention maintainers on an issue or PR when a contribution is blocked on project direction.
