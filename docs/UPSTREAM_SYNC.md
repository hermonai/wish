# Upstream Sync Strategy

Wish is a Hermon AI edition of the open-source Warp client. Upstream Warp moves quickly, so Wish uses regular merge-based syncs to keep the GitHub branch graph current while preserving Wish branding, local-first behavior, WishUI crate names, and Hermon backend boundaries.

## Current Policy

- Merge `hermonai/warp:master` into Wish frequently.
- Keep upstream functional improvements unless they directly conflict with Wish product direction.
- Resolve conflicts in favor of upstream implementation details and Wish product identity.
- Push the merge so GitHub reports Wish as `0` commits behind upstream Warp.
- Keep legal and attribution references to upstream Warp intact.

The current sync has merged upstream through `b9ec4f39` (`Use tombstone for failure UX for cloud mode (#10895)`).

## Sync Workflow

```bash
git fetch warp-upstream master
git merge --no-ff warp-upstream/master

# Resolve conflicts, then verify:
cargo fmt --all
cargo check --workspace
git diff --check
rg -n '^(<<<<<<< |=======$|>>>>>>> )' . --glob '!target/**'

# Confirm the behind count is zero before/after pushing:
git rev-list --left-right --count HEAD...warp-upstream/master
git push origin master
git rev-list --left-right --count origin/master...warp-upstream/master
```

The second number from `git rev-list --left-right --count` is the number of upstream commits Wish is still behind. It should be `0` after the merge commit is pushed.

## Conflict Priorities

1. Preserve buildability and upstream behavior.
2. Preserve Wish product identity: `Wish`, `WishUI`, `wishui-core`, `wish-oss`, and Hermon AI domains.
3. Preserve Hermon backend boundaries: local-first by default, cloud services through Hermon, configurable `HERMON_API_URL` and `WISH_API_URL`.
4. Preserve Wish innovations: onboarding, local Ollama/model discovery, Wish Drive, built-in agents, SDLC tasks, custom inference, and Hermon Cloud language.
5. Preserve upstream legal notices and explicit attribution.

## Common Merge Fixes

| Upstream term | Wish term |
| --- | --- |
| Warp product UI | Wish |
| WarpUI / `warpui` | WishUI / `wishui` |
| `warp_core` | `wish_core` |
| `warp-oss` | `wish-oss` |
| Oz Cloud product copy | Hermon Cloud |
| Oz first-party harness UI | Hermon agent / Wish Agent |

GraphQL wire values, persisted analytics values, and compatibility aliases may still use upstream names when changing them would break stored data or server schema compatibility. Those cases should be isolated and documented in comments.

## Latest Sync Notes

The May 2026 merge brought in upstream work around cloud-mode failure UX, remote development, code review state, global skills, custom inference endpoints, cloud orchestration, MCP transport updates, agent-management polish, and workspace reliability. Wish kept those improvements while reapplying Hermon/Wish naming, local-first startup expectations, and Hermon Cloud positioning.
