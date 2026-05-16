# Wish 0.4.0 — Release Notes

**Codename:** "AI-native seam"
**Release date:** 2026-05-13
**Channel target:** dogfood → preview → stable, per the standard rollout

This release is the first in which Wish is a *meaningfully AI-native* IDE-and-terminal — not "an IDE with a chat panel," but a tool whose agent shares the same authoritative view of the workspace the human sees. It also closes the Warp → Wish rebranding loop on every user-visible surface, fixes several reported defects, and lays the groundwork for the multi-quarter migration to a `wishd`-backed thin-client architecture.

For the design rationale behind every item below, see [`specs/general-programming-ide/PRODUCT.md`](specs/general-programming-ide/PRODUCT.md) and [`specs/general-programming-ide/TECH.md`](specs/general-programming-ide/TECH.md).

---

## Highlights

### 🪄 AI-native workspace context tray

Every Wish-chat user message is now prefixed (transparently, on the wire) with five tagged blocks describing the live workspace state, so the agent's first sentence can correctly cite files, line:col, branch, last command — without any tool call.

```
<active_project>
The user is working in: /Users/dev/proj
</active_project>

<git_status>
Workspace git status:
  /Users/dev/proj on `feature/foo` (main: `main`): 3 modified files (+42 −8)
</git_status>

<workspace_diagnostics>
Workspace diagnostics: 2 errors, 1 warning in 2 files.
src/main.rs (2 errors):
  error 12:5 — cannot find value `x` in this scope [rust-analyzer]
  error 28:1 — expected `;`
src/lib.rs (1 warning):
  warning 4:8 — unused variable: `tmp`
</workspace_diagnostics>

<recently_opened_files>
Files the user has recently opened (3 files):
  /Users/dev/proj/src/main.rs
  /Users/dev/proj/src/lib.rs
  /Users/dev/proj/Cargo.toml
</recently_opened_files>

<recent_terminal_commands>
Recent terminal commands (4 most recent first):
  ✗ cargo test (exit 1)
  ✓ cargo build
  ✓ git status
  ✗ rustc --version-foo (exit 2)
</recent_terminal_commands>

<user_message>
fix this
</user_message>
```

Five providers, all reading from the *same singletons* that feed the panels/footer/UI — so the human and the agent never see a divergent view. Gated by `FeatureFlag::AgentLiveWorkspaceContext` (on in dogfood, off in stable for this release).

The companion `FeatureFlag::LogAgentWorkspaceContext` (on in dogfood) logs the exact composed wire message at INFO level on every send — `tail -f` your wish log to watch the agent receive context in real time.

**The most differentiated provider:** `recent_terminal_commands` exists because Wish's terminal model is *structured* (every command has a known exit code) rather than a glyph buffer. VS Code + Copilot or Cursor cannot do this. With a single prompt, the agent sees "you just ran `cargo test` and got exit 1" without scrollback parsing.

### 📊 Status-bar diagnostic badge

Counts of errors/warnings now render next to the LSP indicator in the workspace footer (e.g. `2 errors, 1 warning`). Same source of record as the agent reads — closes the visible loop: the human sees the same numbers the agent reasoned over.

### 🔧 `wish` as a real CLI entry point

| Invocation | Behavior |
| --- | --- |
| `wish .` | Open cwd as a workspace project |
| `wish ./project` | Open the given directory as workspace |
| `wish src/main.rs` | Open the file in a code pane (workspace = parent dir) |
| `wish src/main.rs:42` | Same + jump to line 42 |
| `wish src/main.rs:42:5` | Same + jump to line 42 column 5 |
| `wish ./project src/main.rs` | Open project, then open file in tab |
| `wish --folder . --file src/main.rs` | Explicit-flag equivalent |
| `wish agent run --prompt …` | Untouched — existing CLI subcommands |

22 unit tests cover the rewrite logic and the new flag parses.

### ✨ Branding pass — Warp/Hermon → Wish/Hermon, end-to-end on user-visible surfaces

| Surface | Was | Now |
| --- | --- | --- |
| AI loading indicator | Warp brand glyph (PUA `U+E500`) | `✨ Wishing…` (universal Unicode sparkles) |
| Wish Drive icon | upstream Warp Drive double-rectangle SVG | clean four-point sparkle in rounded square |
| Wish Drive panel header (×2 sites) | "Warp Drive" | "Wish Drive" |
| File-tree tooltip & left-panel | "Warp Drive" | "Wish Drive" |
| New-session dropdown | "Cloud Hermon" | "Hermon Cloud" |
| Update toast | "Warp updated!" | "Wish updated!" |
| Notification-permission banner | "Warp doesn't have permission…" | "Wish doesn't have permission…" |
| Update-failure banners (×2) | "Warp is unable to perform the update." | "Wish is unable to perform the update." |
| Auto-update launch error | "Warp was unable to launch…" | "Wish was unable to launch…" |
| Deprecation banner | "Some Warp features may not work…" | "Some Wish features may not work…" |
| App menu | "Quit Warp" | "Quit Wish" |
| Resource center tooltip | "Warp Essentials" | "Wish Essentials" |
| API-key modal placeholder + default name | "Warp API Key" | "Wish API Key" |
| Settings feature page search-term | "warp default terminal application" | "wish default terminal application" |
| Subshell page search-term | "warpify subshell" | "wishify warpify subshell" (kept legacy as alias) |
| AI harness default display name | "Warp" | "Wish" |
| Agent git-author for AI-generated commits | `Hermon <hermon-agent@hermon.ai>` | `Wish <wish-agent@hermon.ai>` |
| Brand URLs (×2 sites) | `HERMON_URL` constants | renamed to `HERMON_URL` (values already pointed at `wish.hermon.ai`) |

What deliberately stayed unchanged:
- Internal type names (`WarpTheme`, `WarpDriveModel`, `Harness::Hermon`, `NotificationSourceAgent::Hermon`) — these serialize to telemetry, persisted settings, cloud agent sessions; renaming them needs a dedicated migration slice with serde aliases.
- TOML persistence keys (`warpify.ssh.…`) — renaming would silently wipe user settings on upgrade.
- macOS bundle qualifier `org.hermon.ai` — required for OS-level upgrade paths.
- Rustdoc comments / variable names — cosmetic only, ride along on the next telemetry-aware variant migration.

### 🎯 Always-visible tab close (modern editor UX default)

Tab and pane close X's now render persistently rather than only on hover. Modern editor convention (VS Code, Cursor). The single-line change flips the default of `HeaderContent::simple(…)`; views that genuinely want hover-reveal can opt out by constructing `Standard` directly.

### 🔌 `wishd` integration — Phase 1 begun

New workspace member `crates/wishd_client` lays the wire to the existing `wishd` daemon ([/Users/wenyan/ClaudeProjects/wishd](/Users/wenyan/ClaudeProjects/wishd)). This release ships only the health-service codegen + Unix-socket-path resolver — the connection-manager singleton and the live health probe ship in 0.4.x point releases. The destination is a Wish that's a *thin renderer + agent host* on top of the trusted local daemon; quarter-scale migration with a recorded 7-phase plan.

---

## Smaller improvements

- **`wish` opens a folder via CLI** with `git`/`.gitignore`-respecting file tree, LSP root, and Wish-Drive root all anchored on the chosen directory.
- **Agent-prompt observability** — `FeatureFlag::LogAgentWorkspaceContext` writes the exact composed prompt to log at INFO so dogfooders can audit every turn.
- **CLI ergonomics** — positional path argument and `--folder` / `-d` short flag accepted in any combination; `wish path:42:5` jumps to line:col directly.
- **Status badge unified** — the LSP indicator and diagnostic count share one render path, so they stay in sync as you fix code.
- **Tests** — 58 new unit tests across the diagnostics aggregator, agent context tray, footer badge label formatter, CLI rewrite, and wishd_client. **All passing.**

---

## Bug fixes

| Symptom | Fix |
| --- | --- |
| Cold-start panic: `Cannot get singleton model of type "lsp::manager::LspManagerModel"` | Re-ordered `DiagnosticsAggregatorModel` registration to run *after* `workspace::init` registers `LspManagerModel`. |
| Panic in HOA onboarding flow: `A flex that should expand to a max space can't be rendered in an infinite max constraint` (in `ActionButton::with_full_width(true)` inside the HOA welcome modal) | Defensive cap on the `ConstrainedBox` for `full_width: true` buttons. Fix is generic — applies to all four `with_full_width(true)` call sites (HOA flow, codex modal, openwarp launch modal, session config modal). |
| Agent management panel stuck on "Loading agents…" when Hermon backend is offline | Initial-load handler now flips `has_finished_initial_load = true` on `RequestState::RequestFailedRetryPending` (not just `RequestFailed`), so transient failures advance the UI past the spinner while retries continue in the background. |
| Settings → Environments → Create environment showed a 4-level raw error chain (Firebase token / GraphQL / proxy / connection-refused) in the Repo(s) field when Hermon was offline | New `friendly_github_load_error()` helper collapses the chain to one of two actionable single-sentence messages: "Hermon backend isn't reachable. Start hermon-server or sign in via the user menu, then click Retry." or "Hermon authentication is required. Sign in via the user menu, then click Retry." Unrecognized errors fall back to the leaf cause. 4 tests. |

---

## Known issues / planned follow-ups

### ✅ P0 — `auto (cost-efficient)` model selection — **fixed in 0.4.0**

(Was reported as a known issue mid-release-cycle and landed before tagging.)

Symptom: selecting "auto (cost-efficient)" in the model picker tried to fetch a Firebase ID token via `localhost:8080/proxy/token` (Hermon backend). When Hermon was offline, the request failed with `Connection refused (os error 61)`.

Root cause: `LLMPreferencesModel::refresh_available_models` (logged-in path) called the Hermon server and local Ollama discovery *in parallel*, then unconditionally merged local Ollama on top of the existing model list. When the server call failed, the `Err` branch did nothing — so the existing list (the hardcoded `Default::default()` containing only `auto (cost-efficient)`) stayed as the active list. The "auto" entry then dispatched through Hermon, hitting Connection refused.

Fix: when the server call fails *and* local Ollama models are discovered, fall back to local-only as the primary model list (same code path as the guest/unauth flow). The "auto (cost-efficient)" entry is replaced with the actual Ollama models, so the dispatcher routes through `LocalLlmAdapter` instead of Hermon. If both server and local discovery fail, the existing model list is kept untouched so a transient outage doesn't wipe the user's last-good preferences.

Behavior matrix:

| Server reachable? | Local Ollama? | Result |
| --- | --- | --- |
| ✅ | ✅ | Server models primary, locals merged in as additional choices |
| ✅ | ❌ | Server models only |
| ❌ | ✅ | **Local Ollama models replace stale `auto` default** (the fix) |
| ❌ | ❌ | Existing model list kept; no user-visible change |

### ✅ P1 — Internal Hermon → Hermon rename — **shipped in 0.4.0**

The rename landed late in the cycle with the wire format preserved for round-trip safety:

- **`Harness::Hermon` → `Harness::Hermon`** across 88 call sites. `#[serde(rename = "oz", alias = "hermon")]` + clap `#[value(name = "hermon", alias = "oz")]` mean every wire format keeps emitting/accepting `"oz"`; the Rust identifier and the CLI value are forward-looking. `Harness::display_name(Harness::Hermon)` returns `"Hermon"`. `Harness::config_name()` keeps returning `"oz"` to preserve `HarnessConfig::harness_type` values in persisted state.
- **`HarnessKind::Hermon` → `HarnessKind::Hermon`** (internal dispatch enum).
- **`HERMON_*_ENV` constants → `WISH_*_ENV`** in `crates/wish_cli` (cosmetic; values were already `WISH_*`).
- **`artifact_upload.rs`** now reads `WISH_RUN_ID` from the environment; legacy `HERMON_RUN_ID` accepted as a fallback so pre-rename CI configs keep working.
- **GraphQL-bound enums** `AgentHarness::Hermon` (cynic codegen) and `AIAgentHarness::Hermon` (Rust mirror) deliberately kept — renaming requires a coordinated schema change in `hermon-server`. Documented inline.
- **Outbound URLs**: `https://hermon.ai` → `https://www.hermon.ai` (2 sites), `https://oz.hermon.ai/agents?new=true` → `https://www.hermon.ai/agents?new=true`.

Why it was safe to ship in this release: every Rust-side rename is paired with a serde/clap alias preserving the wire format. A 0.4.0 user with persisted state from a pre-rename build deserializes their stored `harness="oz"` cleanly into `Harness::Hermon` (via serde alias); a 0.3.0 binary still talking to a 0.4.0-authored config sees `harness="oz"` and deserializes into its own `Harness::Hermon`. No coordinated rollout required.

**Post-cut amend (same commit):** the rename was extended to a second pass covering the remaining identifier and user-string surface that hadn't been migrated:

- `Icon::Hermon` → `Icon::Hermon`, `NotificationSourceAgent::Hermon` → `Hermon`, `IconWithStatusVariant::HermonAgent` → `HermonAgent`, `SummaryPaneKind::HermonAgent` → `HermonAgent`, `CloudConversationData::Hermon` → `Hermon`, `CloudModeEntryPoint::HermonLaunchModal` → `HermonLaunchModal`, `FeatureFlag::Hermon*` (5 variants, ~98 call sites) → `FeatureFlag::Hermon*`, `HermonLaunchSlide` → `HermonLaunchSlide`.
- `hermon_binary_path` / `hermon_binary_display` (managed_secrets::gcp) → `wish_binary_path` / `wish_binary_display`; test fixtures `/usr/bin/oz` etc. → `/usr/bin/wish`.
- CLI examples in the cloud-setup guide now show `wish environment create …` and `wish integration create …` (were `oz …`).
- User-visible strings: `"Fix with Hermon"` → `"Fix with Hermon"`; `"Introducing Hermon"` → `"Introducing Hermon"`; `"Command from Hermon"` → `"Command from Hermon"`; `"Hermon Web"` source label → `"Hermon Web"`; all install/uninstall toasts now say "Wish CLI/command"; the warpify SSH description and Codex modal copy updated to mention Hermon/Wish instead of Hermon; the `/continue-locally` slash-command error message updated.
- Outbound URL `https://wish.hermon.ai/oz` (extended-cloud-agents link) → `https://wish.hermon.ai/hermon`.
- Settings-search keyword strings extended with `hermon` and `wish agent` synonyms next to legacy `oz` / `warp agent` tokens so typing either still finds the right page.
- GraphQL schema's `enum AgentHarness { HERMON }` and `Experiment::HERMON_MULTI_HARNESS_*` values retained, annotated inline with the cross-binary contract reasoning.

What remains: roughly 80 `Hermon`-mentioning code comments. Doc-only, not visible to users, swept opportunistically in 0.4.x patch releases.

### 🟨 P2 — Settings page hover-only close (now resolved)

Marked resolved in this release via the `simple()` default flip — the close X is now always visible everywhere. Listed here so dogfooders know the workaround (tab-bar hover-close) is no longer needed.

### 🟦 P3 — `wishd` migration phases 1.2–7 in progress

Roadmap recorded in `specs/general-programming-ide/TECH.md` Milestone P2.x. The destination: terminal sessions, LSP processes, git status, file watchers, and the diagnostic aggregator all live in `wishd`, surviving GUI close. This is the architectural foundation for *truly* beating vim + tmux on the resilience axis.

---

## Architectural direction (for contributors)

This release confirms the four-product architecture documented in `PRODUCT.md`:

| Product | Role |
| --- | --- |
| **`wish`** | Desktop GUI + headless CLI (single binary). Rendering, agent host, workspace identity. **This repo.** |
| **`wishd`** | Trusted local daemon. gRPC over Unix socket. Privileged ops: fs, git, process, terminal, search, capability, cell-verify. **Sibling repo at `/Users/wenyan/ClaudeProjects/wishd`.** |
| **`wishcode`** | Web/Electron product on top of `wishd`. Different renderer, same daemon. **Separate repo.** |
| **`hermon-server`** | Optional cloud backend. Auth, model routing, sync, billing, governance. **Separate repo at `/Users/wenyan/ClaudeProjects/hermon-server`.** |

`wish-cli` stays inside `wish` — same binary, same Rust types, same `wishd` gRPC client. No marketplace plugin model is planned; **languages are first-party, in-tree** (~150 LoC each: LSP adapter + tree-sitter queries + LanguageId), and **workflow extensions are skills** (existing `.agents/skills` system).

For language coverage roadmap, see `PRODUCT.md` — short list: Kotlin, Swift, Java, C#, Ruby next; PHP, Zig, Lua, Haskell, OCaml, Elixir nice-to-have.

---

## Upgrade notes

- **No data migration required.** Settings, persisted projects, agent conversations all continue to work.
- **Agent context injection is on in dogfood, off in stable.** To try it on stable: toggle `FeatureFlag::AgentLiveWorkspaceContext` via the runtime flags menu (`/MODEL` → manage features).
- **Agent commit author has changed** from `Hermon <hermon-agent@hermon.ai>` to `Wish <wish-agent@hermon.ai>`. Historical commits unaffected; commits authored by the agent going forward use the new identity. Adjust any tooling that filters commits by author.
- **`Cmd+W` / tab close** now works without a hover dance — the X is always visible.

---

## Numbers

| Metric | Value |
| --- | --- |
| Slices shipped (incl. hotfixes) | 17 |
| New unit tests across diagnostics + context tray + badge + CLI + wishd_client | 58 |
| User-visible Warp/Hermon strings renamed | 14 |
| New workspace crates | 1 (`wishd_client`) |
| Bug fixes shipped | 3 (init-order panic, flex full_width panic, loading-agents spinner) |
| New feature flags | 2 (`AgentLiveWorkspaceContext`, `LogAgentWorkspaceContext`) — both on in dogfood |
| Dependency additions | `tonic 0.14`, `tonic-build 0.14`, `tonic-prost-build 0.14`, `tonic-prost 0.14` |

---

## Acknowledgements

Dogfood reporters who caught issues during this release cycle: thank you for the screenshots, log captures, and patience with the iterative fixes. Three of this release's bug fixes were reported and verified end-to-end during the same session they were fixed.

Wish is a derivative of [Warp](https://github.com/warpdotdev/warp). The Hermon AI team maintains it as the AI-native fork; see [`docs/UPSTREAM_ATTRIBUTION.md`](docs/UPSTREAM_ATTRIBUTION.md) for the full attribution.

---

*Wish is positioned as the developer's last terminal — one tool for the local editor, the remote shell, the AI conversation, and the workflow extensions, all sharing one structured event stream. This release is one significant step in that direction.*
