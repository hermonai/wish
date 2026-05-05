# Upstream Sync Strategy

This document explains how Wish stays in sync with the upstream
[Warp](https://github.com/hermonai/warp) project, what gets cherry-picked,
what stays divergent, and how to address the GitHub "N commits behind"
counter.

## TL;DR

- **Wish is a long-term divergent fork.** We **selectively cherry-pick**
  upstream commits that fit Hermon AI's product direction; we ignore
  commits that are infrastructure-only, refer to upstream services
  (Warp Cloud, etc.), or implement features outside Wish's scope.
- **The "N commits behind" badge will keep growing** as upstream
  advances. This is **expected** and not a problem — it's a UI
  artifact, not a build/CI gate.
- **Three options to make the badge zero**, none of which we currently
  recommend (see [The "behind" counter](#the-behind-counter) below).

## What we cherry-pick

| Category | Action |
|----------|--------|
| **Security advisories** (CVE / GHSA fixes) | Always cherry-pick promptly |
| **Generic bugfixes** (terminal rendering, keybinding, dropdown alignment) | Cherry-pick when applicable |
| **Performance improvements** (rendering, IO, async) | Cherry-pick |
| **Cross-platform compatibility fixes** (Windows, Linux specifics) | Cherry-pick |
| **License/attribution updates** | Cherry-pick |
| **Documentation improvements that apply to Wish** | Cherry-pick (with rebrand pass) |

## What we DON'T cherry-pick

| Category | Reason |
|----------|--------|
| Telemetry events to upstream's analytics | Different backend (Hermon) |
| Feature flags backed by upstream's experimentation system | We use our own |
| Upstream's release/CI infrastructure | We have our own |
| Branding/UI tied to "Warp" name or company | We're "Wish" / Hermon AI |
| Features deeply coupled to `wish.warp.dev` (now `wish.hermon.ai`) | Different control plane |
| Internal-issue-tracker references (APP-XXX, QUALITY-XXX) without context | Confusing without their tracker |

## Workflow

```bash
# One-time: add upstream as a remote (if not already)
git remote add upstream https://github.com/hermonai/warp.git

# Each sync cycle:
git fetch upstream master

# List candidate commits (upstream commits not in our history)
git log --oneline HEAD..upstream/master

# Inspect a candidate before picking
git show <sha>

# Cherry-pick (use -x to record provenance in the commit message)
git cherry-pick -x <sha>

# Resolve conflicts (typical: file paths renamed warp→wish, branding strings)
# Then continue:
git cherry-pick --continue

# After all picks for a session, run presubmit
./script/presubmit
```

## Conflict-resolution patterns

When cherry-picking from upstream, you'll commonly hit conflicts on:

1. **Renamed files** — `WARP.md` → `WISH.md`, `warp_*` → `wish_*`. Use
   `git checkout --theirs <upstream-name>` to take their content, then
   move it to the wish path manually.
2. **Branding strings** — Replace `"Warp"` → `"Wish"`, `warp.dev` →
   `hermon.ai`, etc. Search for all occurrences in the conflict diff
   before resolving.
3. **Backend URLs** — `app.warp.dev` is upstream's control plane;
   `wish.hermon.ai` is ours. Use `URL_FROM_BUILD_CHANNEL` (already wired
   to point at `hermon.ai`) rather than hardcoded warp URLs.
4. **Cargo.lock conflicts** — Don't manually merge. After cherry-picking
   the source change, run `cargo update --package <name>` to regenerate.

## The "behind" counter

GitHub shows `"N commits behind hermonai/warp:master"` based on
**graph reachability**. Cherry-picking creates new commits with new
SHAs, so even if the *content* is identical, the upstream SHAs are
still graphwise unreachable from your branch — and the counter keeps
ticking up.

Three options to zero it out:

### Option A: Merge from upstream (preserves divergent history)

```bash
git fetch upstream master
git merge upstream/master
# Resolve any conflicts; commit the merge
git push origin master
```

- **Pro:** Counter goes to zero. All upstream commits are reachable.
- **Con:** Pulls in *every* upstream change including ones we
  intentionally exclude (warp branding, telemetry, etc.). High conflict
  load every cycle.

### Option B: Rebase onto upstream

```bash
git fetch upstream master
git rebase upstream/master
git push --force-with-lease origin master
```

- **Pro:** Linear history, counter at zero.
- **Con:** Rewrites SHAs of all wish commits — invalidates open PRs,
  forks, and CI run links. **Force-push is destructive.**

### Option C: Live with the counter (CURRENT POLICY)

- **Pro:** Cherry-pick selectively. No conflicts from rejected upstream
  changes. Wish history stays Wish-shaped.
- **Con:** GitHub UI shows ever-growing "N commits behind". Cosmetic
  only — does not block CI, PRs, or merges.

> [!NOTE]
> Wish currently uses **Option C**. The counter is a UI artifact and
> can be safely ignored. If a contributor asks about it, point them at
> this document.

## Recent sync history

| Date | Upstream SHA(s) | What was applied |
|------|-----------------|------------------|
| 2026-05 | (upstream `64a0dfb`) | Security: bumped `rand` 0.9.1 → 0.9.4 to resolve GHSA-cq8v-f236-94qc |
| 2026-05 | (upstream `59c6a48`) | Docs: added Alacritty attribution headers to `app/src/terminal/model/grid/grid_storage/resize.rs` and `app/src/terminal/ref_tests/mod.rs` |
| 2026-05 | (upstream `5fe2735`) | Bugfix: copy keybinding now prioritizes selected text in the input over selected blocks (APP-4330). |
| 2026-05 | (upstream `6ea1a52`) | Bugfix: new-session "+" dropdown alignment when Tabs Panel is on the right side. Anchor mirrors panel side. |

Add new rows here as you sync. Include the upstream SHA(s) and a
one-line summary of what changed.

## Cherry-pick candidates from current upstream tip (as of 2026-05)

Recent upstream commits worth evaluating (ordered roughly by
priority):

- ✅ **Applied** `64a0dfb` — security: rand 0.9.4
- ✅ **Applied** `59c6a48` — docs: Alacritty attribution headers
- ✅ **Applied** `5fe2735` — Fix copy keybinding to prioritize input
  text over selected blocks (APP-4330)
- ✅ **Applied** `6ea1a52` — Fix new-session "+" dropdown alignment
  when Tabs Panel is on the right (#9492)
- 🔍 **Worth picking** (deferred to follow-up sync — non-blocking):
  - `39ff0d2` — Skip reconnect for unrecoverable transport disconnects
    (resilience improvement; remote_server crate)
  - `c65ae25` — Make sure Windows quake mode window is correctly sized
    and receives focus (Windows-specific; can't test on macOS host)
  - `ce89a98` — fix(bootstrap): warn before sudo and document install
    steps (DX improvement; touches 7 shell scripts)
  - `2258cd3` — Handle agent management view updates based on event type
    (refactor + correctness; touches `agent_conversations_model` —
    review carefully alongside our local Conversations work)
- ❌ **Skip**:
  - `16578b1` — Update remote server logs (upstream-specific)
  - `9d65653` — docs: mention Oz OSS credits form in README (upstream
    branding)
  - `a548a9a` — Use a PAT for pushing new release branches and tags
    (upstream CI infra)
  - `eb61300` — Gate remote server experiment enablement on windows
    (upstream experiment infra; we use Hermon's flags)
  - `b1cb96f` — Use feature flag in settings crate (refactor that
    couples to upstream's flag system)
  - `693cd58` — Add server conversation ID to computer use telemetry
    events (upstream telemetry pipeline)
- 🤔 **Evaluate case-by-case**:
  - `564ea2a` — Show conversation details panel for local conversations
    (UX feature; check if it makes sense without Hermon side)
  - `888c302` — Stage 1: orchestrate tool (client) (large feature; may
    need adaptation for Hermon backend)
  - `361c267` — Implement full-frame clear for active block for CLI
    Agents (terminal rendering improvement)
  - `3abc48b` — Select new default when a model is disabled (UX; may
    need adaptation to Hermon model routing)

When you sync, record what you picked (and what you intentionally
skipped) under [Recent sync history](#recent-sync-history).
