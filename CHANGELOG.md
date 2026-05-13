# Changelog

All notable changes to **Wish** are recorded here.

Format follows [Keep a Changelog](https://keepachangelog.com/) loosely: each release lists *Added / Changed / Fixed / Known Issues* sections in that order, in reverse-chronological order.

For the long-form rationale on each entry, see [`RELEASE_NOTES.md`](RELEASE_NOTES.md) for the current release and [`specs/general-programming-ide/`](specs/general-programming-ide/) for the spec.

---

## 0.4.0 — 2026-05-13 — "AI-native seam"

### Added

- **Workspace context tray for the agent** — every Wish-chat user message is prefixed with five tagged blocks (`active_project`, `git_status`, `workspace_diagnostics`, `recently_opened_files`, `recent_terminal_commands`) so the agent answers from live state without tool calls. Gated by `FeatureFlag::AgentLiveWorkspaceContext` (on in dogfood).
- **Agent prompt observability** — `FeatureFlag::LogAgentWorkspaceContext` logs the composed wire message at INFO on every send. `tail -f` to audit context injection in real time.
- **Status-bar diagnostic badge** — error/warning counts next to the LSP indicator, reading from the same `DiagnosticsAggregatorModel` the agent reads.
- **CLI workspace + file open** — `wish .`, `wish ./project`, `wish src/main.rs:42:5`, `wish ./project src/main.rs`, and `wish --folder PATH --file PATH` all supported. 22 tests cover the rewrite logic.
- **`wishd` integration scaffolding** — new `crates/wishd_client` member with tonic-prost codegen for `health.proto`, plus a `default_socket_path()` resolver (`WISH_RUNTIME_DIR` override + `$HOME/.wish/wishd.sock` fallback). 5 tests.
- **`DiagnosticsAggregatorModel`** — singleton that aggregates LSP diagnostics across all workspace roots into one ordered, severity-counted view. Foundation for the badge, panel UI, and agent injection. 18 tests.
- **`DiagnosticsSummary` + `format_for_agent_context()`** — serializable snapshot + deterministic plain-text rendering for prompt injection. 8 tests.
- **Per-provider formatters** — `format_active_project_block`, `format_open_files_block`, `format_recent_commands_block`, `format_git_status_block`, all pure functions with edge-case tests.
- **`History::iter_all_session_commands()`** — production accessor for terminal command history flattened across shell hosts (was previously test-only).
- **`OpenedFilesModel::iter()`** — accessor for cross-repo recently-opened files.
- **`GitStatusUpdateModel::cached_repo_metadata()`** — non-subscribing snapshot for the agent context tray.

### Changed

- **Branding pass — Warp/Oz → Wish/Hermon, end-to-end on user-visible surfaces.**
    - "Wishing…" footer now shows `✨` (Unicode sparkles) instead of the upstream Warp brand glyph at PUA `U+E500`.
    - `wish-drive.svg` replaced from the upstream Warp Drive double-rectangle to a clean four-point sparkle in a rounded square.
    - 14 user-visible strings renamed: "Wish Drive" (×4), "Hermon Cloud" (was "Cloud Oz"), "Wish updated!", "Wish API Key", "Wish Essentials", "Quit Wish", and the bulk of the update / notification / launch error toasts.
    - Default git author for AI-generated commits changed from `Oz <oz-agent@warp.dev>` to `Wish <wish-agent@hermon.ai>`. Historical commits untouched.
    - `OZ_URL` brand-URL constants renamed to `HERMON_URL` at two sites (values already pointed at `wish.hermon.ai`).
- **Deeper Oz → Wish/Hermon rename across the internal identifier surface.**
    - `OZ_RUN_ID_ENV` / `OZ_PARENT_RUN_ID_ENV` / `OZ_CLI_ENV` / `OZ_HARNESS_ENV` constants in `crates/wish_cli` renamed to `WISH_*_ENV` (values were already `WISH_*` — purely cosmetic identifier rename).
    - `WISH_RUN_ID_ENV_VAR` constant in `app/src/ai/agent_sdk/artifact_upload.rs` now actually reads `WISH_RUN_ID` from the environment (was `OZ_RUN_ID`); legacy `OZ_RUN_ID` kept as a fallback so existing CI configs and harness invocations keep working.
    - `Harness::Oz` enum variant renamed to `Harness::Hermon` across 88 call sites in the wish workspace. **Wire format unchanged:** serde still emits `"oz"` via `#[serde(rename = "oz", alias = "hermon")]`, clap still accepts `--harness oz` via `#[value(alias = "oz")]`, and `Harness::config_name()` still returns `"oz"` — so telemetry analytics, persisted user preferences, in-flight cloud agent sessions, and existing CLI invocations all round-trip exactly as before.
    - `HarnessKind::Oz` renamed to `HarnessKind::Hermon` (internal-only dispatch enum).
    - `Harness::display_name(Harness::Hermon)` returns `"Hermon"` (was `"Oz"`).
    - GraphQL-bound enums `AgentHarness::Oz` (cynic codegen) and `AIAgentHarness::Oz` (the Rust mirror of the GraphQL type) deliberately keep their `Oz` variant names — renaming requires a coordinated schema change in `hermon-server`. Documented inline.
- **Deeper internal rename pass (post-cut amend).**
    - `Icon::Oz` → `Icon::Hermon`, `NotificationSourceAgent::Oz` → `Hermon`, `IconWithStatusVariant::OzAgent` → `HermonAgent`, `SummaryPaneKind::OzAgent` → `HermonAgent`, `CloudConversationData::Oz` → `Hermon`, `CloudModeEntryPoint::OzLaunchModal` → `HermonLaunchModal`.
    - `FeatureFlag::Oz*` variants (5) → `FeatureFlag::Hermon*` across ~98 call sites. No serde derive on `FeatureFlag` — Debug-formatted menu IDs only, so rename is wire-safe.
    - `OzLaunchSlide` → `HermonLaunchSlide` (file name `oz_launch.rs` retained to avoid a module-path churn cycle).
    - `oz_binary_path` / `oz_binary_display` parameters in `managed_secrets::gcp` renamed to `wish_binary_path` / `wish_binary_display`; test fixtures `/usr/bin/oz` → `/usr/bin/wish`, `/bin/oz` → `/bin/wish`. Reflects the actual binary name produced by `cargo build -p wish`.
    - `visit_oz_button` → `visit_hermon_button` in cloud setup guide view; CLI examples in that guide now show `wish environment create …` and `wish integration create …` (were `oz environment create …`).
    - User-visible toasts and labels: "Successfully installed/uninstalled the Oz CLI/command" → "…Wish CLI/command"; `AgentSource::WebApp` display name "Oz Web" → "Hermon Web"; `"Fix with Oz"` action label → `"Fix with Hermon"`; `"Introducing Oz"` tab name → `"Introducing Hermon"`; `"Command from Oz"` workflow name → `"Command from Hermon"`; "/continue-locally is only available for cloud Oz conversations" → "…cloud Hermon conversations"; warpify SSH description "…auto-complete, Oz, and more." → "…auto-complete, Hermon, and more."; Codex modal "Use Codex directly in Oz and leverage…" → "…in Wish and leverage…".
    - Outbound URLs: `https://wish.hermon.ai/oz` (extended-cloud-agents link in free-tier modal) → `https://wish.hermon.ai/hermon`; `https://docs.warp.dev/reference/cli` (install toast) → `https://www.hermon.ai/docs/reference/cli`.
    - Settings search keywords gained `"hermon"` and `"wish agent"` synonyms next to the legacy `"oz"` / `"warp agent"` tokens, so typing either still finds the same pages.
    - GraphQL schema `enum AgentHarness { OZ }` annotated with a long-form comment explaining why the wire token stays `OZ` (telemetry analytics filter on it; server-side change required to migrate).
- **Outbound URL fixes.** Two remaining `https://warp.dev` open-URL call sites in workspace view changed to `https://www.hermon.ai`. `https://oz.warp.dev/agents?new=true` (used by the Create-API-key modal) changed to `https://www.hermon.ai/agents?new=true`.
- **Tab close X always visible.** `HeaderContent::simple(…)` now defaults `always_show_icons = true`. Matches the modern VS Code / Cursor convention; hover-reveal was the upstream Warp behavior.
- **`Workspace::open_repository`** dispatch threaded through CLI so `wish [path]` works without launching the GUI first.
- **`LspServerModel`** gained `iter_diagnostics()` so subscribers registered after diagnostics arrive can replay state.

### Fixed

- **Cold-start panic** in `DiagnosticsAggregatorModel::new`: now registered after `workspace::init` so `LspManagerModel::handle(ctx)` resolves.
- **HOA welcome modal flex panic** (`A flex that should expand to a max space can't be rendered in an infinite max constraint`): defensive max-width cap on `ActionButton::with_full_width(true)` ConstrainedBox. Covers all four call sites of `with_full_width(true)`.
- **Agent management panel stuck on "Loading agents…"** when Hermon backend is offline: initial-load handler now also flips the loading flag on `RequestState::RequestFailedRetryPending`, so transient failures advance past the spinner while retries continue.
- **`auto (cost-efficient)` model routes through Hermon `/proxy/token` and fails with `Connection refused` when Hermon is offline.** `refresh_available_models`'s logged-in branch now falls back to local Ollama models as the primary list when the server call fails. The stale `Default::default()` "auto" entry no longer dispatches through Hermon when local discovery returns models. Existing list preserved when both server and local discovery fail.
- **Settings → Environments → Create environment surfaced raw 4-level error chain** ("Failed to load GitHub repos: Failed to get access token for GraphQL request: …Connection refused") in the Repo(s) field when Hermon was offline. New `friendly_github_load_error()` helper collapses the chain to one of two actionable messages: "Hermon backend isn't reachable. Start hermon-server or sign in via the user menu, then click Retry." (for connection failures) or "Hermon authentication is required. Sign in via the user menu, then click Retry." (for auth failures). Unrecognized errors fall back to the leaf cause only. 4 unit tests cover the collapse cases.

### Known issues
- **🟦 P2 — Residual `Oz` mentions in internal code comments and doc strings.** The renames above cover identifiers and user-visible strings; ~80 comment-only `Oz` references remain (e.g. `// Default to Oz when the snapshot has no harness`) and will be swept opportunistically. Not user-visible.
- **🟦 P3 — `wishd` integration phases 1.2–7** in progress. Roadmap in `specs/general-programming-ide/TECH.md` Milestone P2.x.

### Dependencies

- Added `tonic 0.14.0` + `tonic-build 0.14.0` + `tonic-prost-build 0.14.0` + `tonic-prost 0.14.0` for the wishd_client crate.

### Architectural

- Confirmed four-product architecture documented in `PRODUCT.md`: `wish` (renderer + agent host), `wishd` (trusted local daemon), `wishcode` (web/Electron alternative renderer), `hermon-server` (optional cloud backend).
- Language support stays first-party (in-tree); workflow extensions are skills; no VS Code-style plugin marketplace planned.

---

## Earlier releases

Earlier release snapshots are preserved as sibling directories (`/Users/wenyan/ClaudeProjects/wish-v0.1.1`, `…wish-v0.2.0`, `…wish-v0.2.5`, `…wish-v0.3.0`). These predate the current Rust-+-WishUI architecture documented in `WISH.md` and the specs.
