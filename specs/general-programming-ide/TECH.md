# Wish general-purpose IDE — implementation plan

This document maps PRODUCT.md onto the existing Wish codebase, identifies the crates and surfaces that need work, and proposes a phased delivery plan. It is a living plan: each milestone ends in a working, shippable state behind one or more `FeatureFlag`s.

## Inventory: what already exists (do not duplicate)

The Wish workspace already contains substantial IDE plumbing. Understanding what's there is the first step.

### Editor and rendering
- [crates/editor](../../crates/editor) — buffer model, multiline, selection, search, decorations, render. ~4.8 k LoC. Solid.
- [crates/syntax_tree](../../crates/syntax_tree) — tree-sitter highlight + indent queries.
- [crates/wishui, crates/wishui-core, crates/wishui_extras](../../crates) — the WishUI framework. All new IDE views render through this.

### Language and code-intelligence
- [crates/lsp](../../crates/lsp) — LSP client with manager, transport, install, server_repo_watcher, plus first-class adapters in `servers/` for **clangd, generic, go, pyright, rust, typescript_language_server**. ~4.6 k LoC. Most of P1's LSP needs are already implemented at the protocol layer.
- [crates/languages](../../crates/languages) — language metadata.

### Code panel (existing IDE-shaped surface)
- [app/src/code](../../app/src/code) — already contains `local_code_editor.rs` (2.5 k LoC), `view.rs` (2.3 k LoC), `footer.rs` (2.0 k LoC), `global_buffer_model.rs` (2.0 k LoC), `find_references_view.rs`, `inline_diff.rs`, `diff_viewer.rs`, `editor_management.rs`, `language_server_extension.rs`, `lsp_telemetry.rs`, `opened_files.rs`, `file_tree/`. This is the seam P1 extends — we are not building a new code panel from scratch.

### Search, terminal, AI
- [crates/wish_ripgrep](../../crates/wish_ripgrep) — workspace search engine; needs UI bindings.
- [crates/wish_terminal](../../crates/wish_terminal) — native terminal with command-block model.
- [app/src/ai](../../app/src/ai) — Agent Mode, conversations, harnesses, skills, attachments. Inline AI edits will reuse this.
- [app/src/command_palette.rs](../../app/src/command_palette.rs) — palette infrastructure.

### Persistence, settings, IPC
- [crates/persistence](../../crates/persistence) — Diesel/SQLite. Workspace state persists here.
- [crates/settings, crates/settings_value](../../crates/settings) — settings infrastructure.
- [crates/ipc, crates/jsonrpc](../../crates) — process boundary plumbing reusable for DAP.

### Vim / input
- [crates/vim](../../crates/vim) — vim emulation, already integrated.

### What is missing entirely (need new crates)
- DAP client (debugger).
- Test runner abstraction.
- Workspace tasks engine.
- Source control panel as a UI (git plumbing may exist; the panel does not).
- Snippets engine.
- Outline / breadcrumbs view.
- Notebook view.

---

## Architecture principles

1. **Reuse before refactor.** The existing `app/src/code` surface is the IDE today. P1 features extend it; only refactor what blocks the work.
2. **Crate per concern.** Each new vertical (DAP, tasks, source-control, tests, snippets, outline) is a crate under `crates/`. UI for each is a sibling module under `app/src/code/` or `app/src/`.
3. **Local-first by construction.** No new IDE workflow may require `HERMON_API_URL`. Cloud features sit behind explicit Hermon-gated flags.
4. **Feature-flag everything new.** Every new user-visible behavior lands behind a `FeatureFlag` in `crates/wish_core/src/features.rs`, dogfood-on early, removed after stabilization per `remove-feature-flag` skill.
5. **AI never blocks.** Any AI-assisted action has a deterministic non-AI path that's at least as fast.
6. **WishUI all the way down.** No new view escapes the WishUI Entity-Handle model. No HTML, no embedded webviews.

---

## Phase 1 — Basic IDE

### Milestone P1.1 — "Open Folder" workspace as a first-class concept

**Status:** Largely *already implemented* in the upstream Wish codebase. We ship the missing CLI entry point in this iteration; the rest is verification.

What was already in place before this work (gated by `FeatureFlag::Projects`, default-on in dogfood):

- A `Project` row in [crates/persistence/src/model.rs:223](../../crates/persistence/src/model.rs) with `path`, `added_ts`, `last_opened_ts`, backed by the `projects` table migration.
- `ProjectManagementModel` in [app/src/projects.rs](../../app/src/projects.rs) — singleton `SingletonEntity` that loads persisted projects and exposes `upsert_project(path)`, `all_projects()`, and emits `ProjectEvent::Added/Removed/Updated`.
- The `workspace:open_repository` global action in [app/src/workspace/global_actions.rs:89](../../app/src/workspace/global_actions.rs) which:
  - If an active window exists, dispatches `WorkspaceAction::OpenRepository { path }` to its workspace.
  - Otherwise dispatches `root_view:open_new_from_path` to create a new window pinned to the path.
- `Workspace::handle_open_repository` in [app/src/workspace/view.rs:11294](../../app/src/workspace/view.rs) which upserts the project in `ProjectManagementModel`, opens a `SingleTerminal` tab with the path as `initial_directory`, and sets `maybe_set_pending_repo_init_path` so LSP and the file tree pick up the new root.
- A command-palette `ProjectDataSource` and `SuggestedProjectsDataSource` under [app/src/search/command_search/projects](../../app/src/search/command_search/projects) that already serves recent projects via fuzzy search.
- A right-panel "Open Repository" button and `OpenFolder` app menu entry.

What this slice adds:

- `--folder PATH` / `-d PATH` CLI option in [crates/wish_cli/src/lib.rs](../../crates/wish_cli/src/lib.rs) (`AppArgs::folder`).
- Positional-path support: `wish .`, `wish ./project`, `wish /abs/path`, `wish ../sibling`, `wish ~/foo` are all rewritten to `wish --folder PATH` by a conservative preprocessing pass that leaves subcommands and URL positionals alone.
- Startup dispatch in [app/src/lib.rs](../../app/src/lib.rs) that canonicalizes the path (so `.` resolves to the launch cwd) and fires `workspace:open_repository`, reusing the existing global-action plumbing rather than duplicating it.
- Unit tests covering the rewrite logic and the new flag parse paths.

Slice shipped on top of the workspace primitive — `wish path/to/file.rs[:LINE[:COL]]` opens files in code panes:

- New repeatable `--file PATH` / `-F PATH` option on `AppArgs` ([crates/wish_cli/src/lib.rs](../../crates/wish_cli/src/lib.rs)). Accepts the editor-standard `path:line:col` suffix.
- `rewrite_path_positional` now classifies each positional via `classify_path_positional`:
  - Existing directory → `--folder PATH`.
  - Existing file (possibly with `:LINE[:COL]`) → `--file PATH`.
  - Unknown path-shape → first one becomes `--folder` (matches `wish ./new-proj`), subsequent unknowns become files.
  - Multiple files supported: `wish src/a.rs src/b.rs` opens both, with the folder derived from the first file's parent at startup.
- `dispatch_to_active_workspace` re-exported as `pub(crate)` from [app/src/workspace/mod.rs](../../app/src/workspace/mod.rs) so startup can dispatch typed `WorkspaceAction`s directly without adding more global actions (per the existing "no new global actions" guidance).
- Startup ([app/src/lib.rs](../../app/src/lib.rs)) parses each file via `wish_util::path::CleanPathResult::with_line_and_column_number`, canonicalizes the path, derives a folder root when `--folder` was omitted, dispatches `workspace:open_repository`, then dispatches `WorkspaceAction::OpenFileInNewTab { full_path, line_and_column }` per file.
- URL guard tightened from `Url::parse` (too liberal — it accepts `Cargo.toml:10` as scheme=`Cargo.toml`) to a strict `contains("://")` check, so `:LINE[:COL]` suffixes are correctly preserved.
- 13 new tests cover the file-flag parse, the new classification cases (file in cwd, file with `:LINE`, file with `:LINE:COL`, multiple files, mixed folder+file, subcommand passthrough, flag passthrough after positional), and `split_line_column_suffix` edge cases (Windows drive letter `C:\…` rejected, no-digits rejected).

Remaining gaps in P1.1 to address in follow-ups:

- Restore previously open files, cursor positions, and pane layout per project. Today the projects table holds only `path` + timestamps; per-project session state lives elsewhere or not at all. Add a `project_sessions` migration.
- Per-workspace `.wish/settings.toml` overrides. The settings layer in [crates/settings](../../crates/settings) is global only; needs a project-scoped overlay layer keyed off `ProjectManagementModel`'s active project.
- Recent-projects palette is wired (`ProjectDataSource`) but the cold-start "no window open" code path goes through `root_view:open_new_from_path` — verify behavior when `wish recent` (future CLI command) wants to pre-populate the picker.

No new feature flag introduced; we ride on the existing `FeatureFlag::Projects`.

### Milestone P1.2 — Editor essentials gap-fill (2–3 weeks)

The `editor` crate already has selection, search, multiline, render. Verify and fill gaps:

- Bracket auto-pair, auto-close strings (driven by tree-sitter language config).
- Comment toggle (line + block) — language-aware via `crates/languages`.
- Expand/shrink selection by syntax node (tree-sitter walk).
- Multi-cursor: confirm `crates/editor/src/selection.rs` already supports it; wire keybindings (`Alt+Click`, `Cmd/Ctrl+D`, `Cmd/Ctrl+Shift+L`).
- Find-in-file UI polish (regex, count, find-prev). Replace-in-file (`Cmd/Ctrl+H`).
- Go-to-line palette (`Cmd/Ctrl+G`).
- Save-all command (`Cmd/Ctrl+Alt+S`).

No new crates. Mostly UI wiring in `app/src/code/local_code_editor.rs` and `view.rs`.

### Milestone P1.3a — Problems panel foundation (workspace-wide diagnostics aggregator)

**Status:** Pure-data foundation shipped; view is the next slice.

Built as the data layer the Problems panel will sit on:

- New `pub fn iter_diagnostics(&self) -> impl Iterator<Item = (&Path, &DocumentDiagnostics)>` on [crates/lsp/src/model.rs](../../crates/lsp/src/model.rs) so a subscriber can replay any diagnostics that arrived before it registered.
- New [app/src/code/diagnostics.rs](../../app/src/code/diagnostics.rs):
  - `DiagnosticsState` — pure (no `wishui`) storage keyed by `(LanguageServerId, PathBuf)`. Distinct servers reporting on the same file (`rust-analyzer` + `clippy`, `pyright` + `ruff`) do not clobber each other. Idempotent re-publishes don't fire a change. Exposes `update_path`, `clear_server`, `counts() -> SeverityCounts`, `entry_count`, `file_count`, `entries_for_path`, `entries_grouped_by_path`.
  - `DiagnosticsAggregatorModel` — `SingletonEntity` wrapper. On construction it subscribes to `LspManagerModel` for server lifecycle (`ServerStarted` → subscribe to that server, `ServerRemoved` → purge its rows). For each tracked server it subscribes to `LspEvent::DiagnosticsUpdated` and reflects the snapshot into state. Emits `DiagnosticsAggregatorEvent::Changed` when state actually changes (so passive views don't repaint on no-ops).
  - Catches up at startup with any servers that were already registered before the aggregator was constructed.
- Initialised at app startup in [app/src/lib.rs](../../app/src/lib.rs) **immediately after `workspace::init(ctx)`**, gated by `cfg(not(target_family = "wasm"))`. The aggregator's constructor calls `LspManagerModel::handle(ctx)`, so it must register *after* `workspace::init` which is where `lsp::init` registers `LspManagerModel`. Registering earlier panics at startup with `Cannot get singleton model of type "lsp::manager::LspManagerModel" that was never registered` — a real bug we hit and fixed.
- 10 unit tests on `DiagnosticsState` covering: empty state, ingest+clear-via-empty, idempotent republish, multi-server multi-file severity tallies, two-servers-same-file independence, grouped sort order (severity then position), file ordering, no-op clear on unknown server, unknown-severity bookkeeping, path filter.

Build clean (`cargo check --bin dev`, 0 errors). `cargo clippy -p wish --lib --tests` clean. No new feature flag — the model is invisible without a UI consumer.

The follow-up "Problems panel" view (`app/src/code/problems_panel.rs`) reads `DiagnosticsAggregatorModel::state().entries_grouped_by_path()`, subscribes to `Changed`, and renders via WishUI. Click-to-jump dispatches `WorkspaceAction::OpenFileInNewTab { full_path, line_and_column }` using the seam exposed in the file-open slice.

### Milestone P1.3b — Agent-facing diagnostic surface

Shipped alongside P1.3a so the agent always sees the same state the human does. This is what makes the IDE *AI-native* rather than "IDE plus chat":

- `DiagnosticsSummary` — `serde::Serialize` struct with `totals: SeverityCounts`, `file_count: usize`, `by_file: Vec<PerFileSummary>` (sorted descending by severity weight), `top: Vec<TopDiagnostic>` (severity-first then file:line). Capped to a caller-supplied `top_n` so it's cheap on huge workspaces.
- `TopDiagnostic` — denormalized for wire/prompt legibility: `path`, `line`, `column`, `severity` (string label), `message`, `source`. JSON output deliberately matches what the agent should reason about.
- `DiagnosticsState::summarize(top_n)` — produces the snapshot.
- `DiagnosticsState::format_for_agent_context(max_files, max_per_file)` — deterministic, 1-based line/column plain-text rendering. Example:
  ```
  Workspace diagnostics: 2 errors, 1 warning in 2 files.
  src/main.rs (2 errors):
    error 12:5 — cannot find value `x` in this scope [rust-analyzer]
    error 28:1 — expected `;`
  src/lib.rs (1 warning):
    warning 4:8 — unused variable: `tmp`
  ```
  Pluralization, grouping, and "…and N more" caps are all unit-tested.
- 8 additional unit tests on the summary + formatter (empty state, severity-weight ordering, top-N caps, JSON shape, 1-based line/col, file/per-file caps, singular grammar).

Consumers (future slices):
- **Problems panel header**: renders `summarize(0).totals` as a tab badge.
- **Status-bar diagnostic badge**: same source.
- **Agent context tray**: inserts `format_for_agent_context(max_files=8, max_per_file=5)` as a tagged context block before the user's prompt, so every inline edit / chat turn knows the current diagnostic state without an LSP roundtrip.
- **Inline AI fix on diagnostic**: pulls a single `TopDiagnostic`'s range + message into the agent's task description, scoped to that one defect.

This is the seam PRODUCT.md item #36 ("Live workspace state for the agent") sits on.

### Milestone P1.3c — Agent prompt injection (the AI-native seam goes live)

**Status:** Shipped. First slice in which Wish's AI-native principle becomes user-visible.

The first consumer of the diagnostic surface from P1.3b is the Wish chat itself. Every user message sent to the agent now carries a tagged preamble of live workspace state, so the model never has to ask "what's broken?" — the answer is already in the prompt.

What landed:

- New `FeatureFlag::AgentLiveWorkspaceContext` in [crates/wish_features/src/lib.rs](../../crates/wish_features/src/lib.rs). Off in stable, on in dogfood.
- New module [app/src/ai/wish_conversation/agent_context.rs](../../app/src/ai/wish_conversation/agent_context.rs):
  - `NamedContextBlock { name: &'static str, body: String }` — one section per provider, so future blocks (git status, open files, last failing test) drop in without touching call sites.
  - `compose_message_with_context(message, blocks) -> String` — pure, deterministic composer. Empty `blocks` returns the message unchanged so clean workspaces produce zero behavior change. Uses XML-style tags that frontier models reliably treat as structured context rather than user prose.
  - `collect_workspace_context(ctx) -> Vec<NamedContextBlock>` — pulls `DiagnosticsAggregatorModel::format_for_agent_context(8, 5)` and emits a `workspace_diagnostics` block. Skips the block entirely on clean workspaces (no `Workspace diagnostics: clean.` preamble).
- [app/src/ai/wish_conversation/model.rs](../../app/src/ai/wish_conversation/model.rs) `send_user_message` now calls a `compose_outgoing(&message, ctx)` helper that respects the feature flag. The composed string is what the adapter receives; conversation history (`append_user`) still records the user's *original* message so the chat UI shows the human-authored text, not the synthetic preamble.

Wire format the model receives when diagnostics are present:

```
<workspace_diagnostics>
Workspace diagnostics: 2 errors, 1 warning in 2 files.
src/main.rs (2 errors):
  error 12:5 — cannot find value `x` in this scope [rust-analyzer]
  error 28:1 — expected `;`
src/lib.rs (1 warning):
  warning 4:8 — unused variable: `tmp`
</workspace_diagnostics>

<user_message>
fix the errors
</user_message>
```

When the workspace is clean, the wire is exactly the original message — no preamble.

6 new unit tests cover the composer: empty blocks pass-through, one block tagged correctly, multiple blocks in insertion order, trailing-whitespace trim, empty user message preserved, inner newlines preserved verbatim.

Build clean (`cargo check --bin dev` 1.93 s incremental, 0 errors). 24/24 tests pass (18 diagnostics + 6 agent_context). `cargo fmt -p wish -p wish_features --check` clean. `cargo clippy -p wish --lib --tests --all-features` clean on touched code.

Future providers that plug into the same `Vec<NamedContextBlock>`:
- `git_status` (current branch, ahead/behind, changed-file list)
- `active_test_failures` (the test runner's last red bar)
- `recent_commands` (last N terminal commands + exit codes)
- `active_selection` (when an inline AI edit is triggered with a selection)

### Milestone P1.3d — Agent now sees the user's focus (`recently_opened_files`)

**Status:** Shipped — second `NamedContextBlock` provider, proves the extension model.

The agent context tray is no longer a one-trick pony. Today's prompt to the agent looks like:

```
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

<user_message>
help me fix this
</user_message>
```

The model now knows what the user is *looking at* in addition to what's broken — so "help me fix this" resolves to a specific file even when the user doesn't name it.

What landed:

- [app/src/code/opened_files.rs](../../app/src/code/opened_files.rs): new `OpenedFilesModel::iter()` accessor exposing every `(repo, files_in_repo)` pair without leaking internal storage.
- [app/src/ai/wish_conversation/agent_context.rs](../../app/src/ai/wish_conversation/agent_context.rs):
  - `format_open_files_block(paths, limit) -> Option<String>` — pure formatter. `None` on empty input (no block emitted). Singular/plural grammar (`1 file` vs `3 files`), `…and N more` truncation, input order preserved.
  - `collect_open_files(model) -> Vec<PathBuf>` — pure list builder. Joins each repo-relative file path onto its repo root and sorts by recency descending.
  - `collect_workspace_context` now emits the `recently_opened_files` block after diagnostics. Default limit: 8 files (`RECENT_FILES_LIMIT`).
- 6 new unit tests on the formatter: empty pass-through, singular grammar, plural-within-limit, truncation with "…and N more", input-order preservation, absolute-path verbatim render.

Verification: `cargo check --bin dev` clean (1.84 s incremental, 0 errors). `cargo test`: 30 / 30 pass (18 diagnostics + 12 agent_context). `cargo fmt --check -p wish` clean. `cargo clippy -p wish --lib --tests --all-features` clean on touched code.

Each new provider is now a ~10-line addition to `collect_workspace_context` plus a pure formatter — the extension model is proven.

### Milestone P1.3e — Agent sees workspace identity (`active_project`)

**Status:** Shipped — third `NamedContextBlock` provider, sets the path-resolution baseline for every prompt.

Today's prompt to the agent now leads with workspace identity, then live diagnostics, then user focus:

```
<active_project>
The user is working in: /Users/dev/proj
</active_project>

<workspace_diagnostics>
Workspace diagnostics: 2 errors, 1 warning in 2 files.
…
</workspace_diagnostics>

<recently_opened_files>
Files the user has recently opened (3 files):
  /Users/dev/proj/src/main.rs
  /Users/dev/proj/src/lib.rs
  /Users/dev/proj/Cargo.toml
</recently_opened_files>

<user_message>
help me fix this
</user_message>
```

The block goes first so the model has a path-resolution baseline before encountering any path-bearing diagnostic or file. Language inference (Rust because of `Cargo.toml`, etc.) comes for free from the path + sibling blocks.

What landed:

- [app/src/ai/wish_conversation/agent_context.rs](../../app/src/ai/wish_conversation/agent_context.rs):
  - `find_active_project(projects: &[Project]) -> Option<&Project>` — pure picker. Most-recently-used wins (`last_opened_ts` if set, otherwise `added_ts`). `None` for empty fresh-install state.
  - `format_active_project_block(path: &str) -> Option<String>` — pure formatter. Trims input; `None` on empty.
  - `collect_workspace_context` clones the projects vec from `ProjectManagementModel`, runs the picker, and emits the block first.
- 5 new unit tests: empty path → None, verbatim path render, no-projects → None, most-recent-last_opened picked correctly, fallback to `added_ts` when `last_opened_ts` is None.

Verification: `cargo build --bin wish` clean (2 m 2 s incremental). `cargo test`: 35 / 35 pass (18 diagnostics + 17 agent_context: 6 composer + 6 open_files + 5 active_project). `cargo fmt --check -p wish` clean. `cargo clippy -p wish --lib --tests --all-features` clean on touched code.

Provider order in `collect_workspace_context` (matters — preserved by the composer):
1. `active_project` (workspace identity baseline)
2. `workspace_diagnostics` (what's broken, with absolute paths)
3. `recently_opened_files` (user's current focus)

### Milestone P1.3f — Agent sees the terminal (`recent_terminal_commands`)

**Status:** Shipped — fourth provider, the most defensible architectural advantage Wish has over any other AI IDE.

VS Code and Cursor cannot do this. They have *unstructured* terminal output (a glyph buffer with ANSI escape codes). Wish's terminal-block model means every command the user has run in this Wish session has a *known* exit code, command line, timestamps, and pwd. The aggregation cost is O(N) on a small N, no new subscription, no scraping.

Today's full wire format with all four providers:

```
<active_project>
The user is working in: /Users/dev/proj
</active_project>

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

The agent's first sentence can correctly cite line 12 column 5, name the file, **and** say "I see your last cargo test failed — let's fix the errors in main.rs first." All from one prompt, zero tool calls.

What landed:

- [app/src/terminal/history.rs](../../app/src/terminal/history.rs): new `History::iter_all_session_commands()` accessor exposing every `Arc<HistoryEntry>` across all shell hosts in production (the existing `session_commands()` was gated behind `#[cfg(feature = "integration_tests")]`).
- [app/src/ai/wish_conversation/agent_context.rs](../../app/src/ai/wish_conversation/agent_context.rs):
  - `RecentCommand { command, exit_code }` value type — denormalized for test ergonomics.
  - `format_recent_commands_block(commands, limit) -> Option<String>` — pure formatter. Success commands elide their exit code for noise reduction; failures show `(exit N)`, still-running show `(still running)`. Long lines truncated to `RECENT_COMMAND_MAX_LEN` (200 chars). Internal newlines collapsed to `⏎` markers so multi-line pastes stay one bullet.
  - `collect_recent_commands(history)` — flattens History across hosts, sorts most-recent-first using `completed_ts` (then `start_ts`), takes the top N.
  - `collect_workspace_context` now emits the `recent_terminal_commands` block last, with limit `RECENT_COMMANDS_LIMIT = 10`.
- 8 new unit tests on the formatter: empty → None, success elides exit code, failure shows `✗` + exit, still-running shows `•`, input order preserved, long-line truncation, multi-line collapse, limit-capping with count in header, negative exit code as failure.

Verification: `cargo check --bin dev` clean (1.93 s incremental). `cargo test`: 44 / 44 pass (36 from previous + 8 new). `cargo fmt --check -p wish` clean. `cargo clippy -p wish --lib --tests --all-features` clean on touched code.

Provider order in `collect_workspace_context` (final, in wire order):
1. `active_project` (where am I?)
2. `workspace_diagnostics` (what's broken?)
3. `recently_opened_files` (what's the user looking at?)
4. `recent_terminal_commands` (what did the user just do?)

### Milestone P1.3g — Status-bar diagnostic badge (visible counterpart to the AI-native seam)

**Status:** Shipped — closes the loop. The agent has been silently consuming diagnostic state for several slices; the *human* now sees the same count in the footer next to the LSP indicator.

What landed:

- New `CodeFooterView::render_diagnostic_badge` in [app/src/code/footer.rs](../../app/src/code/footer.rs:1411) — reads `DiagnosticsAggregatorModel::state().counts()` and renders a small text badge `"2 errors, 1 warning"` when any errors or warnings exist; returns `Empty` on clean workspaces so the badge does not occupy footer real estate when there's nothing to say. Info/hint severities are suppressed at this surface to keep at-a-glance signal tight.
- New pure helper `format_diagnostic_badge_label(&SeverityCounts) -> String` at the bottom of the same file with 7 unit tests covering singular/plural grammar for each severity, both-severities composition, and info/hint suppression.
- Both `CodeFooterView::new` and `CodeFooterView::new_for_workspace` now `subscribe_to_model(&DiagnosticsAggregatorModel::handle(ctx), …)` so the badge re-renders the moment any LSP server publishes a new diagnostic — the count the human sees stays in lockstep with the count the agent injected on its last turn. The two singletons (footer + aggregator) are the same one we already wire into the agent context tray, so there is no risk of drift.
- The badge sits in the footer's main `render` immediately after `render_lsp_icon`, so it visually anchors to the language-server indicator.

Verification:
- `cargo check --bin wish` clean (1.84 s incremental).
- `cargo test -p wish --lib`: **51 / 51 pass** (44 from previous slices + 7 new badge tests).
- `cargo fmt --check -p wish` clean.
- `cargo clippy -p wish --lib --tests --all-features` clean on touched code.

Why this matters: until now every AI-native slice was *invisible* to the human — the structured context flowed straight into the prompt and the user had to trust it was there. The footer badge is the first user-visible artifact of the diagnostic aggregator. When the user sees `2 errors, 1 warning` in the footer, they can be confident the agent saw the same numbers when they typed "fix this." That feedback loop is what makes the AI-native seam *credible*, not just architecturally correct.

### Milestone P1.3h — Wire-level observability for the AI-native seam

**Status:** Shipped — companion observability slice. The user can now *see exactly what the agent sees*, character-for-character, on every Wish-chat turn.

What landed:

- New `FeatureFlag::LogAgentWorkspaceContext` in [crates/wish_features/src/lib.rs](../../crates/wish_features/src/lib.rs). Default-off in stable, on in dogfood.
- [app/src/ai/wish_conversation/model.rs](../../app/src/ai/wish_conversation/model.rs)'s `compose_outgoing` now logs the composed wire message at INFO level when both `AgentLiveWorkspaceContext` *and* `LogAgentWorkspaceContext` are on. Log line shape:

  ```
  [Wish Chat → agent] Outgoing message (1247 chars):
  <active_project>
  …
  </active_project>

  <workspace_diagnostics>
  …
  </workspace_diagnostics>

  <recently_opened_files>
  …
  </recently_opened_files>

  <recent_terminal_commands>
  …
  </recent_terminal_commands>

  <user_message>
  fix this
  </user_message>
  ```

How a dogfooder uses it:

```sh
tail -f ~/.wish/logs/wish-local.log | rg -A 50 "Wish Chat → agent"
```

Verification: `cargo check --bin wish` clean (2.35 s incremental). Spec-only change to runtime behavior — no new tests beyond the existing composer suite (44 + 7 = 51 passing across diagnostics/agent_context/footer modules).

Why this slice closes the AI-native loop:

1. The agent **sees** the workspace state (slices 6–9).
2. The human **sees** the diagnostic count in the footer (slice 10).
3. The human can now **inspect** the exact prompt the agent received (slice 11).

Three independent observability surfaces, same single source of record. The user can grep their log, paste the prompt into another LLM if they want a second opinion, or compare turns to debug "why did the agent miss this?" That's the AI-native development loop in motion — transparent, debuggable, no black box.

### Milestone P1.3i — `git_status` provider (5th context block)

**Status:** Shipped — the agent now knows the user's current branch, main branch, and uncommitted-change summary across every cached repo in the workspace.

What landed:

- New non-subscribing accessor `GitStatusUpdateModel::cached_repo_metadata(&AppContext) -> Vec<(PathBuf, GitStatusMetadata)>` on [app/src/code_review/git_status_update.rs](../../app/src/code_review/git_status_update.rs). Walks the existing `WeakModelHandle` cache, upgrades each, and returns a snapshot — *does not* spawn new watchers. Cheap on every chat turn.
- New `RepoGitSummary` test-friendly mirror value type in [app/src/ai/wish_conversation/agent_context.rs](../../app/src/ai/wish_conversation/agent_context.rs) so the pure formatter can be unit-tested without dragging in the `local_fs`-gated git subsystem.
- New `format_git_status_block(repos)` pure formatter. Renders e.g.:
  ```
  Workspace git status:
    /Users/dev/proj on `feature/foo` (main: `main`): 3 modified files (+42 −8)
  ```
  Special cases:
  - On the main branch + clean → `(main branch): clean` (no `(main: ...)` redundancy).
  - Off main + clean → `(main: `main`): clean`.
  - Dirty → `: N modified file(s) (+ADDS −DELS)` with U+2212 minus sign so the agent never confuses `-` with a path separator or option flag.
  - Detached HEAD: the upstream `detect_current_branch_display` returns a short SHA already; the formatter renders it verbatim with no special case.
- `collect_workspace_context` inserts the `git_status` block immediately after `active_project` so workspace identity and git identity travel together at the top of the prompt. The whole branch is `#[cfg(feature = "local_fs")]`-gated so WASM builds stay clean.

7 new unit tests cover: empty → None, on-main-clean rendering, off-main-clean, dirty-with-line-deltas, singular grammar, multi-repo bullets in input order with one header, detached-HEAD passthrough.

Final provider order in `collect_workspace_context` (in wire order):
1. `active_project` (where am I?)
2. `git_status` (what branch + how dirty?)
3. `workspace_diagnostics` (what's broken?)
4. `recently_opened_files` (what's the user looking at?)
5. `recent_terminal_commands` (what did the user just do?)

Verification: `cargo check --bin wish` clean (2.30 s incremental). `cargo test`: **58 / 58 pass** (51 + 7 new). `cargo fmt --check -p wish` clean. `cargo clippy -p wish --lib --tests --all-features` clean on touched code.

Why this matters: branch + dirty-file state is the *first* thing a senior developer asks before suggesting a change ("are you on a feature branch? what have you already changed?"). Now the agent knows the answer before the question is asked. Combined with the diagnostic + terminal-history blocks, a prompt like "what should I do next?" gets a meaningfully grounded answer: "you're on `feature/foo` with 3 dirty files, last `cargo test` failed at exit 1, here are the errors…"

### Slice 13 — Wish branding consistency pass

User-reported: "many locations still have old Warp logo / 'Wishing…' for AI thinking / Wish Drive logo…". The Wish/Hermon icon already maps to `bundled/svg/hermon-logo.svg` (the `Icon::Warp` enum variant is a deprecated alias for `Icon::Wish` — same asset). The remaining issue was *text* still saying "Warp".

11 user-visible strings renamed `Warp → Wish` in this slice:

| File | String |
| --- | --- |
| [app/src/drive/index.rs](../../app/src/drive/index.rs) | `WARP_DRIVE_TITLE = "Wish Drive"` |
| [app/src/drive/index.rs](../../app/src/drive/index.rs) | `Text::new_inline("Wish Drive", …)` (panel header) |
| [app/src/workspace/view.rs](../../app/src/workspace/view.rs) | `ToolPanelView::WarpDrive => "Wish Drive"` (×2 tooltip sites) |
| [app/src/workspace/view.rs](../../app/src/workspace/view.rs) | `"Wish doesn't have permission to send desktop notifications."` |
| [app/src/workspace/view.rs](../../app/src/workspace/view.rs) | `"Wish updated!"` toast |
| [app/src/workspace/view.rs](../../app/src/workspace/view.rs) | `"Wish Essentials"` resource-center tooltip |
| [app/src/workspace/view.rs](../../app/src/workspace/view.rs) | `"Wish was unable to launch the new installed version."` |
| [app/src/workspace/view.rs](../../app/src/workspace/view.rs) | `"Some Wish features may not work … but Wish is unable to perform the update."` (deprecation banner) |
| [app/src/workspace/view.rs](../../app/src/workspace/view.rs) | `"A new version is available but Wish is unable to perform the update."` |
| [app/src/workspace/mod.rs](../../app/src/workspace/mod.rs) | `EditableBinding "Quit Wish"` |
| [app/src/settings_view/platform/create_api_key_modal.rs](../../app/src/settings_view/platform/create_api_key_modal.rs) | placeholder + default name `"Wish API Key"` |
| [app/src/settings_view/features_page.rs](../../app/src/settings_view/features_page.rs) | search-terms `"wish default terminal application"` |
| [app/src/settings_view/warpify_page.rs](../../app/src/settings_view/warpify_page.rs) | search-terms `"wishify warpify subshell"` (kept legacy term as alias) |
| [app/src/ai/harness_availability.rs](../../app/src/ai/harness_availability.rs) | default harness display_name `"Wish"` |

What deliberately stayed `Warp`:
- Internal type names (`WarpTheme`, `WarpDriveModel`, `WarpAgent` settings variant) — these are stable identifiers; the `to_string()` representations already render `"Wish *"`.
- Persistence keys (`warpify.ssh.…` TOML paths) — renaming would silently wipe user settings on upgrade.
- macOS bundle qualifier `org.hermon.ai` — required for OS-level compatibility / upgrade paths.
- ANSI OSC marker debug logs ("Warp OSC marker…") — internal diagnostic strings, not user-facing.
- Telemetry event documentation strings — internal.
- `warp_default_settings.csv` file name — would orphan existing user configs on upgrade.
- Code comments mentioning Warp — high-volume, near-zero user value.

The user's "Wishing…" AI loading text was already correctly branded (`pub const LOAD_OUTPUT_MESSAGE: &str = "Wishing…";` in `app/src/ai/blocklist/block/view_impl/common.rs:133`), as was the `Icon::Wish` asset and the settings section displays. The remaining gap was the long-tail user-visible toasts/banners/tooltips above.

Verification: `cargo check --bin wish` clean (2.20 s incremental). My-scope tests **58 / 58 pass**. Two pre-existing failures in `settings_view::environments_page::tests` are unrelated to this slice (test data references "warp-internal" / "Wish-Internal" inconsistently — present before this branch).

Why this slice matters: brand consistency is the foundation of credibility. If half the toasts say "Warp" and half say "Wish," every user thinks "incomplete fork." Slice 13 closes that gap on the surfaces a user is most likely to hit (folder picker, settings, error banners, update toasts, menu items).

### Milestone P2.x — Wish + `wishd` integration (corrected design)

**Status:** Design. `wishd` already exists as a separate workspace at `/Users/wenyan/ClaudeProjects/wishd` — earlier draft incorrectly proposed building it from scratch. This milestone is therefore not "build wishd" but **"integrate wish with the existing wishd."**

### The actual architecture (confirmed by user)

```
┌──────────────────────┐
│  hermon-server       │   cloud backend (auth, model routing, sync,
│  (cloud, optional)   │   billing, governance). Lives at hermon.ai
└──────────┬───────────┘   or localhost:8080 in dev.
           │ HTTPS / WS
           │
┌──────────┴───────────┐
│  wishd  (local)      │   trusted local daemon. Already exists.
│  ─ gRPC over Unix    │   ~/Users/wenyan/ClaudeProjects/wishd.
│    socket            │   ~260 tests across 10 crates.
│  ─ 10 services       │   Owns privileged ops on behalf of clients.
│    (fs, git, process,│
│     terminal, index, │
│     capability, …)   │
│  ─ Cell trust + cap- │
│    ability model     │
└──┬──────┬───────┬────┘
   │      │       │   gRPC clients
   │      │       │
   │      │      ┌▼────────────────┐
   │      │      │  wishcode (web/ │   separate desktop product, uses
   │      │      │  electron)      │   wishd via TS proto bridge.
   │      │      └─────────────────┘
   │      │
   │     ┌▼─────────────────┐
   │     │  wish (this app) │   GUI desktop client. Talks to wishd for
   │     │  Rust + WishUI   │   privileged ops, holds rendering + agent
   │     └──────────────────┘   context state in-process.
   │
   ▼
┌─────────────────┐
│  wish-cli       │   currently embedded in the `wish` crate as
│  (`crates/      │   `crates/wish_cli`. Same gRPC client surface
│   wish_cli`)    │   to wishd as the GUI uses.
└─────────────────┘
```

### wishd's existing surface

From [the wishd README](file:///Users/wenyan/ClaudeProjects/wishd):

| wishd crate | What it owns |
| --- | --- |
| `wishd-types` | pure-Rust domain types: ids, errors, paths, session, version |
| `wishd-proto` | tonic-built codegen of every `.proto` service definition |
| `wishd-fs` | filesystem operations: read, write, list, stat, mkdir, move, remove |
| `wishd-git` | git operations: status, diff, log, branches, stage, commit, checkout |
| `wishd-process` | process spawning and management |
| `wishd-terminal` | PTY session management: create, write, resize, destroy |
| `wishd-index` | search index: tantivy BM25 + vector store + hybrid RRF ranking |
| `wishd-capability` | capability evaluator: grant/deny decisions for 13 capability kinds |
| `wishd-cell-verify` | cell trust verification: signature checking, trust-tier enforcement |
| `wishd-server` | gRPC server binary: all service implementations + Unix socket listener |

`proto/` directory contains: `auth.proto`, `capability.proto`, `cell_verify.proto`, `fs.proto`, `git.proto`, `health.proto`, `index.proto`, `process.proto`, `terminal.proto`, `wishd.proto`.

### Answering the user's three questions

**1. Should `wishd` be consolidated into `wish`?**

**No.** `wishd` is correctly architected as a separate concern. It serves *multiple* client products (Wish, Wish Code, wish-cli) over a stable gRPC wire format with a capability + trust model. Folding it into `wish` would:
- Break wishcode (which uses the TS proto bridge today).
- Tie privileged-ops lifecycle to the desktop GUI lifecycle (the exact regression we want to avoid).
- Force any future Wish-family product to reinvent fs/git/process/terminal layers.

The separation is the right shape. The work is the *opposite* — make `wish` (and `wish-cli`) actually *use* the wishd services that already exist, instead of doing fs/git/process/terminal work inside the GUI process.

**2. How does `wishd` serve wishcode + wish-cli simultaneously?**

Same way it serves Wish: a gRPC server listening on a Unix-domain socket (or named pipe on Windows). Every client opens a socket connection and uses the proto-generated clients. wishcode uses TS bindings (proto bridge), wish + wish-cli use Rust clients generated from the same protos. Authentication + capability scoping is per-client-session, so wish-cli's permissions are independently audited from wish-GUI's.

**3. Should `wish-cli` be part of `wish`?**

**Yes, keep it inside `wish` for now.**

The CLI lives in `crates/wish_cli` and is built as part of the `wish` workspace. That's the right shape because:
- Single distribution: users get `wish` binary + `wish` CLI from the same install.
- Shared model: CLI subcommands like `wish agent run` reuse the same Rust types the GUI uses.
- Independent of wishd: as wish migrates to gRPC-talking-to-wishd, wish-cli inherits the same client code for free.

The CLI doesn't need its own product identity — it's the headless side of one product. Spinning it out into a separate crate workspace would duplicate the gRPC client + auth + capability code.

### What changes for wish concretely

Migration is per-singleton, slice-by-slice:

| Currently in `wish` (in-process) | Lives in `wishd` | Slice priority |
| --- | --- | --- |
| PTY/terminal session state | `wishd-terminal` | high — survives GUI close |
| LSP server processes (`LspManagerModel`) | `wishd-process` (LSPs as managed processes) | high — diagnostics survive |
| `GitStatusUpdateModel` (slice 12) | `wishd-git` | medium — already cheap in-process |
| File watchers (`Repository`, `OpenedFilesModel`) | `wishd-fs` (watch subscription stream) | medium |
| `History` (command history) | partly `wishd-terminal` (recent commands), partly persistence | low |
| Diagnostics aggregator (slice 4) | derives from wishd-process LSP output | high — depends on LSP move |
| `ConversationManagerModel` (slice 5+) | stays in wish; agent state is GUI-product-specific | n/a |
| Workspace state, window state, focus | stays in wish | n/a |

Each row is a focused slice: define the gRPC surface in `wishd-proto` (already mostly defined), build a wish-side `wishd_client` adapter, switch the singleton from "in-process" to "gRPC-backed" behind a `FeatureFlag::WishdBacked{Foo}` for safe rollout, remove the in-process path once stable.

### Migration plan (revised — uses existing wishd)

1. **Phase 1 — wishd client bootstrap in wish.** Add a `crates/wishd_client` member that wraps tonic-generated clients from wishd's protos. Single connection-manager singleton; reconnects on socket loss. Pulled from wishd's proto bundle directly (vendored or git-dep — wishd README says "Proprietary" today, so coordinate licensing).
2. **Phase 2 — Health probe.** Wish startup pings `wishd-server` health endpoint. If wishd isn't running, prompt user to install / launch it. Behind a feature flag for safety while the integration matures.
3. **Phase 3 — First migrated subsystem: terminal sessions.** Wish creates PTYs via `wishd-terminal` gRPC. Sessions survive GUI close. This is the headline win.
4. **Phase 4 — LSP processes via wishd-process.** Restart-safe LSP. Diagnostics aggregator subscribes to wishd's LSP-output stream.
5. **Phase 5 — Git via wishd-git.** Replace `GitStatusUpdateModel`'s direct git2 calls.
6. **Phase 6 — File operations via wishd-fs.** Replace direct filesystem code in `code/file_tree`.
7. **Phase 7+ — Capability + cell-verify.** Wish becomes a constrained client; privileged ops require explicit capability grants. This is the security-model upgrade that distinguishes Wish from a "trusted always" desktop app.

Each phase is multi-slice. Phase 1 alone is ~3 slices (proto vendor, client crate, health-check wiring). The destination is a Wish that is a *thin renderer + agent host* on top of a trusted local daemon — the foundation for "beat vim+tmux."

#### Phase 1 — Slice 1.1 shipped: `crates/wishd_client` skeleton + health.proto

What landed:

- New workspace member [`crates/wishd_client`](../../crates/wishd_client). Glob-included via the existing `crates/*` pattern in `Cargo.toml`; nothing to register manually.
- Vendored [`crates/wishd_client/proto/health.proto`](../../crates/wishd_client/proto/health.proto) verbatim from wishd's `proto/health.proto` (29 lines, the smallest self-contained service).
- [`build.rs`](../../crates/wishd_client/build.rs) drives tonic-prost codegen, client-side only.
- [`src/lib.rs`](../../crates/wishd_client/src/lib.rs) exposes the generated `wishd.health.v1` types as the `health` module, plus a `default_socket_path()` helper that honors `WISH_RUNTIME_DIR` (for fixtures) and falls back to `$HOME/.wish/wishd.sock`.
- 5 unit tests cover socket-path resolution (env var override + fallback), proto round-trip encode/decode, and a tripwire test on the socket-path constants (`.wish/`, `wishd.sock`, `WISH_RUNTIME_DIR`).

Workspace dependency additions (in [`Cargo.toml`](../../Cargo.toml)):

- `tonic = "0.14.0"` (was missing entirely; needed for the gRPC client runtime).
- `tonic-build = "0.14.0"` and `tonic-prost-build = "0.14.0"` (tonic 0.14 split codegen across two crates; both needed).
- `tonic-prost = "0.14.0"` (runtime codec, similarly split).

Why pin tonic 0.14 even though wishd pins 0.12: the gRPC wire format is independent of Rust client/server library versions, so two processes can use different tonic versions and still talk. Sticking with 0.14 keeps wish aligned with its existing `prost 0.14.3` (the prost-derive proc macros are version-coupled to prost itself).

Build cost: ~34 s incremental on first compile of tonic + tonic-prost; near-zero afterward. All 5 wishd_client tests pass; full `cargo check --bin wish` clean.

Next slices in Phase 1:
- **1.2 Connection manager singleton.** A `WishdConnectionModel` (`SingletonEntity`) in the wish crate that owns the gRPC `tonic::transport::Channel` over a Unix socket, with auto-reconnect on socket-loss.
- **1.3 Health probe wiring.** On wish startup, call `health.check("")` once; on `NotServing` or transport error, surface a toast "wishd is not running" with a "launch / install" affordance. Behind `FeatureFlag::WishdHealthProbe` (off in stable, on in dogfood).

After Phase 1, every subsequent phase (terminal, LSP, git, fs) follows the same template: vendor more `.proto` files, add to `build.rs`, expose generated clients, build a wish-side adapter, switch one singleton behind a `WishdBacked{Foo}` flag.

#### Hermon → Hermon: rebranding work — landed in 0.4.0

Original triage estimated 691 "hermon" hits across 179 files. By release cut:

- **User-visible UI strings** — all fixed: "Cloud Hermon" → "Hermon Cloud", "Fix with Hermon" → "Fix with Hermon", "Introducing Hermon" tab → "Introducing Hermon", "Hermon Web" source label → "Hermon Web", install/uninstall toasts → Wish CLI / Wish command, CLI examples in cloud-setup guide → `wish environment create …`, warpify SSH description, Codex modal copy, `/continue-locally` error message all updated. `HERMON_URL` constants → `HERMON_URL`. URL `https://wish.hermon.ai/hermon` → `…/hermon`.
- **Internal enum variants** — landed with serde aliases for wire-format compat:
  - `Harness::Hermon` → `Hermon` (88 sites; `#[serde(rename = "hermon", alias = "hermon")]` + clap alias)
  - `HarnessKind::Hermon` → `Hermon` (internal dispatch)
  - `NotificationSourceAgent::Hermon` → `Hermon`
  - `NotificationAgentVariant::Hermon` → `Hermon` (`#[serde(rename = "hermon")]`)
  - `Icon::Hermon` → `Hermon`
  - `IconWithStatusVariant::HermonAgent` → `HermonAgent`
  - `SummaryPaneKind::HermonAgent` → `HermonAgent`
  - `CloudConversationData::Hermon` → `Hermon`
  - `CloudModeEntryPoint::HermonLaunchModal` → `HermonLaunchModal`
  - `FeatureFlag::Hermon*` (5 variants, ~98 call sites) → `Hermon*`
  - `HermonLaunchSlide` → `HermonLaunchSlide`
  - `SessionType::Hermon` → `Hermon`
  - `GuidedModalSessionType::Hermon` → `Hermon` (`#[serde(rename = "hermon")]`)
  - `ModelSelection::Hermon` → `Hermon`
  - `ResumeOptions::Hermon` → `Hermon`
- **Variable names** — `hermon_binary_path` / `hermon_binary_display` → `wish_binary_path` / `wish_binary_display`; `visit_hermon_button` → `visit_hermon_button`; `show_hermon` → `show_hermon`.
- **Comments + doc strings** — bulk-renamed `Hermon` → `Hermon` in all `//`, `///`, `//!` lines.
- **Wire-bound surfaces retained, documented inline:**
  - GraphQL `enum AgentHarness { HERMON }`, `Experiment::HERMON_MULTI_HARNESS_*` — bound to `hermon-server` schema
  - `AIAgentHarness::Hermon`, `AgentHarnessInput::Hermon` — Rust mirrors of GraphQL enums
  - `api::harness::Variant::Hermon` — gRPC proto wire variant
  - `X-Hermon-Api-Source` HTTP request header
  - `did_check_to_trigger_hermon_launch_modal` persisted setting key
  - `Harness::config_name()` returning `"hermon"`
  - Settings-section parser strings `"Hermon"` and `"Hermon Cloud API Keys"` — legacy aliases for users migrating from older config

The retained tokens are *wire format only*, all documented in code where they appear. Forward-rolling consumers see `hermon` aliases. A coordinated `hermon-server` schema change is the next step for `enum AgentHarness` to migrate; that ships separately from the wish client.

### Slice 14 — User-reported visual + behavioral fixes

Dogfooder reported four issues. Three are fixed in this slice; one is documented as a known issue for a focused follow-up.

**1. The Warp brand glyph still appears next to "Wishing..." and as the Wish Drive icon.**

Root cause was twofold:
- `WARP_GLYPH = "\u{E500}"` (two sites: [shimmering_wish_loading_text.rs](../../app/src/ai/loading/shimmering_wish_loading_text.rs) and [view_impl/common.rs](../../app/src/ai/blocklist/block/view_impl/common.rs)) is a private-use-area Unicode codepoint embedded as the Warp brand mark in the bundled Roboto font. Renders as the Warp logo glyph wherever the font is rendered.
- `app/assets/bundled/svg/wish-drive.svg` was *named* Wish but its *content* was the upstream Warp Drive double-rectangle shape.

Fix:
- Replaced both `WARP_GLYPH` constants with `"\u{2728}"` (✨ SPARKLES). Same single-codepoint width so layout / indent math is preserved; universally rendered (no font dependency); semantically "AI / wish / magic." Modern AI-era visual.
- Replaced `wish-drive.svg` with a four-point sparkle inside a soft-rounded square — a clean Wish-themed glyph that visually differs from the Warp shape and shares the sparkle motif with the "Wishing..." footer.

**2. Settings page has no close affordance.**

Investigated: `SettingsView::render_header_content` returns `view::HeaderContent::simple("Settings")` ([settings_view/mod.rs:2690](../../app/src/settings_view/mod.rs)). The `simple` variant of `HeaderContent` does not render a close button — only `Standard`/`Custom` headers do. The fix is to migrate settings to a `Standard` header with an explicit close action, but touching the pane-header `simple` variant or migrating settings to a new header type risks layout regressions across every other view that uses `simple` (Welcome, Get Started, network log, etc.).

**Recorded as a known issue.** Slated for a focused follow-up where the change can be visually validated tab-by-tab. Workaround today: close the settings tab via the tab bar's hover-close (X).

**3. Agent management panel stuck on "Loading agents..." when Hermon is offline.**

Root cause: [agent_conversations_model.rs](../../app/src/ai/agent_conversations_model.rs)'s initial-load handler only flipped `has_finished_initial_load = true` on `RequestState::RequestSucceeded` and `RequestState::RequestFailed`. The third variant — `RequestFailedRetryPending` — fell through silently. With Hermon offline, every attempt returned `Connection refused (os error 61)` *with retries pending* for the duration of the retry chain, so the model stayed in `is_loading()=true` and the view stayed on the spinner.

Fix: also flip the flag in the `RequestFailedRetryPending` branch. The semantics shift slightly — "loading" now means "we've never *attempted* the initial load" rather than "we don't yet have a definitive answer." Once an attempt has occurred (even a transient failure), the view moves past the spinner to the proper empty/setup-guide state, while retries continue in the background and a later `RequestSucceeded` can still populate the list normally.

**4. Language support / plugins — what's Wish's answer to the VS Code marketplace?**

Documented in [PRODUCT.md](PRODUCT.md) under "Language support — what's the Wish equivalent of 'VS Code plugins'?". Summary of the design decision:

- **Language support is first-party, in-tree.** Adding a language is a Wish-team PR (~150 LoC: LSP adapter + tree-sitter queries + `LanguageId`), not a third-party extension. Guarantees consistent UX, single LSP install/status flow, and one diagnostic aggregator (slice 4) feeding the agent context tray (slice 6+) for every language.
- **Workflow extensions are skills.** The existing `.agents/skills` / `npx skills` system is Wish's plugin model — version-locked, declarative, prompt-shaped. Skills extend the *agent's* capabilities, not the editor process.
- **AI-native onboarding for new languages.** Long-term: the agent itself authors language support. Given `.kt` files and `build.gradle.kts`, Wish detects the language, recommends the LSP (`kotlin-language-server`), generates a skeleton adapter, and opens a PR. The agent is the package manager.

Verification: `cargo check --bin wish` clean (2.03 s incremental). My-scope tests **58 / 58 pass**.

### Hotfix — ActionButton full_width crash in unbounded parents

A user-found dogfood crash: dispatching `WorkspaceAction::ShowHoaOnboardingFlow` (Hermon Online Account onboarding modal) panicked at `crates/wishui-core/src/elements/flex/mod.rs:207`:

> A flex that should expand to a max space can't be rendered in an infinite max constraint
> (flex created at app/src/view_components/action_button.rs:905)

Root cause: `ActionButton::with_full_width(true)` puts the inner row in `MainAxisSize::Max`, which requires the parent to give a finite max constraint. The HOA welcome modal renders its CTA button in a container with no max width, so the flex assertion fires.

Fix in [app/src/view_components/action_button.rs](../../app/src/view_components/action_button.rs:919): when `full_width: true` *and* no explicit `width` is set, cap the `ConstrainedBox` at `f32::MAX / 2.0` so the max constraint is finite but effectively unbounded. Well-constrained parents still shrink the button to their width; pathological infinite-parent layouts no longer crash. Fixes the entire class — four call sites (`hoa_onboarding_flow`, `codex_modal`, `openwarp_launch_modal`, `session_config_modal`) all benefit from the same single change.

Not strictly part of the IDE work, but recorded here because shipping a binary that crashes on `Settings → … → Show HOA Onboarding` undermines every other slice's "best in the world" claim.

### Milestone P1.3 — LSP UX completeness (2 weeks)

LSP protocol is in place; the UI/UX is the work.

- Hover popover with Markdown rendering (reuse `crates/markdown_parser`).
- Signature help popover — new lightweight WishUI overlay.
- Go-to-definition / declaration / type-def / implementation: bind keys, route through `lsp::service`. Open in a new tab if the target is in another file; respect "peek" vs "open" preference.
- References: existing `find_references_view.rs` is the surface; add grouped/preview rendering and "find references" code-lens entry point.
- Diagnostics: existing in-line surface; add **Problems panel** (new `app/src/code/problems_panel.rs`) listing all diagnostics across opened workspace files, severity sorted, file-grouped, click-to-jump.
- Completion popup: ranked, kind-iconed, with `additionalTextEdits` (auto-imports). Wire snippet placeholders (depends on P2 snippets, so ship plain-text-only first).
- Rename: `F2` → workspace edit applied as one undo. New tiny modal for the new-name input.
- Code actions / quick-fix: `Cmd/Ctrl+.` lightbulb. New `code_actions_menu.rs`.
- Format document / selection: route to LSP `textDocument/formatting`. External formatter wiring lands in P1.7.
- LSP status indicator: extend [app/src/code/footer.rs](../../app/src/code/footer.rs).

Feature flag: `IdeLspUx`.

### Milestone P1.4 — Project-wide search UI (1 week)

`wish_ripgrep` exists; the UI does not.

- New view `app/src/code/project_search.rs`. Search bar with literal/regex/word/case toggles, include/exclude globs, replace-all preview.
- Streamed results rendered via WishUI list (paginate, group by file, mini-preview).
- "Search in folder" entry from file tree right-click.

Feature flag: `IdeProjectSearch`.

### Milestone P1.5 — Editor tabs and splits (2 weeks)

Today the code panel manages opened files via `editor_management.rs` and `opened_files.rs`. Extend to:

- A `PaneGroup` containing one or more `Pane`s, each with a tab strip.
- Drag-to-split (drop tab onto edge), keyboard split (`Cmd/Ctrl+\`).
- Tab pinning, "close others", "close to right", reordering.
- Persist pane layout per workspace.

This is the most invasive UI change in P1 and the work most likely to surface latent assumptions in `code/view.rs`. Risk: high. Mitigation: land behind `IdePanesV2`, keep the legacy single-pane path for one release, then remove.

### Milestone P1.6 — Quick open and symbol palette (1 week)

- File picker (`Cmd/Ctrl+P`): fuzzy over the project file list. Reuse `crates/fuzzy_match`. Index built lazily as the file tree scans the workspace.
- Workspace symbols (`Cmd/Ctrl+T`) via LSP `workspace/symbol`. Group by kind.
- Document symbols (`Cmd/Ctrl+Shift+O`) via LSP `documentSymbol`.

All three reuse the existing command palette container with different result providers.

### Milestone P1.7 — Formatters and tasks (2 weeks)

- New crate **`crates/wish_tasks`**: model for runnable tasks (build/test/run/lint), discovery from `Cargo.toml`, `package.json` scripts, `Makefile` targets, `CMakeLists.txt`/`CMakePresets.json`, `pyproject.toml` (`[tool.poetry.scripts]`), and a user-defined `.wish/tasks.toml`.
- Formatter integrations: rustfmt, clang-format, ruff/black, prettier. External-command runners with stdout-replace semantics. Format-on-save setting per language. If both an LSP formatter and an external formatter are configured, external wins.
- "Run task…" command palette entry. Selected task runs in a Wish terminal block, stdout streamed; problem-matcher parses output and adds entries to the Problems panel.

Feature flag: `IdeTasks`.

### Milestone P1.8 — Source control panel (basic) (2 weeks)

- New crate **`crates/wish_scm`** (or extend existing if present) wrapping `git2` for status / diff / stage / unstage / discard / commit. Respect `.gitattributes`.
- Panel view: changed files grouped, click-to-diff using existing inline-diff. Stage/unstage hunk reuses `inline_diff.rs`.
- Branch indicator in the status bar; branch picker palette.

Feature flag: `IdeScm`.

### Milestone P1.9 — AI in the editor (2 weeks)

The agent infra is enormous (`app/src/ai/`). The IDE surface is what's missing.

- `Cmd/Ctrl+I` on selection: opens an inline prompt overlay anchored at selection. Submit streams an LLM diff via existing agent plumbing into a new transient buffer; render via existing inline-diff. Accept (all/per-hunk) writes to disk; reject discards.
- Diagnostic quick-fix: when a diagnostic has no LSP-provided fix, the code-actions menu offers "Ask Wish to fix"; same path as `Cmd/Ctrl+I` but pre-populated with the diagnostic message and code range.
- Right-click → "Explain": opens chat with selection pre-attached as context.

Feature flag: `IdeInlineAi`.

### Milestone P1.10 — Settings, keybindings, themes (1 week)

- Per-workspace `.wish/settings.toml` overrides loaded into `crates/settings`. Watcher reloads on change.
- Keybinding presets: a "VS Code-like" preset opt-in via settings, with conflicts surfaced.
- Theme tokens audit: ensure WishUI tokens cover editor + terminal + IDE chrome cohesively.

### P1 exit criteria

All P1 product behaviors pass a checklist of integration tests in `crates/integration`. `./script/presubmit` clean. Performance budgets verified on a 50 k-file workspace. Dogfooded internally for two weeks with no P0 bugs open.

---

## Phase 2 — Toward VS Code parity

Each item gets its own spec under `specs/`. Sketch:

- **`crates/wish_dap`** — DAP client. UI: gutter breakpoints in `editor`, debug toolbar, Variables/CallStack/Watch panels in `app/src/code/debug/`. `.wish/launch.toml` schema. Adapters: CodeLLDB, debugpy, js-debug.
- **`crates/wish_tests`** — test runner abstraction. Per-language adapters parse test discovery and run output. Tree view in side panel.
- **Outline + breadcrumbs**: lightweight; new module under `app/src/code/`. Driven by LSP `documentSymbol`.
- **Sticky scroll**: rendering tweak in `editor::render`.
- **Minimap**: new render layer in `editor`.
- **Code lens**: LSP `textDocument/codeLens` plus Wish-injected lenses (e.g., "Run test").
- **Snippets**: `crates/wish_snippets` with VS Code-compat JSON loader and a YAML-native form.
- **Diff/merge editor**: extend `inline_diff.rs` to a three-way mode.
- **Notebooks (Python)**: `crates/wish_notebook` with `jupyter_client` over Tokio. `.ipynb` reader/writer.
- **Workspace tasks v2**: typed `.wish/tasks.toml` with problem matchers and dependencies.

P2 work is parallelizable across teams once P1 lands; each milestone is independent except DAP+tests (shared launcher infra).

---

## Phase 3 — AI-native differentiation

Each candidate is a separate spec; deliberately unscoped here.

- Ghost-text completion (LSP-coexisting).
- Conversational `Cmd/Ctrl+K`.
- AI-aware terminal blocks (extend Warp-style block model).
- Repo-aware chat with structured context tray.
- Multi-file AI edits with accept/reject diff stack.
- Skills as IDE actions (registry surfacing).
- "Ghost author" overlay for agent-proposed edits during pair sessions.

---

## Cross-cutting concerns

### Performance
- File tree virtualization for ≥50 k entries.
- LSP runs out of process; no LS work on the UI thread.
- `wish_ripgrep` already streams; ensure UI consumes incrementally.
- Editor render budget enforced via existing WishUI frame scheduler.

### Telemetry
- New events (gated by privacy settings) per `add-telemetry` skill: workspace open, LSP start/restart/error, AI inline edit accept/reject ratios, task run, diagnostic counts. No file contents, ever.

### Testing
- Unit tests per new crate using the `${filename}_tests.rs` convention from WISH.md.
- Integration tests in `crates/integration` per `warp-integration-test` skill: open a fixture project, exercise tree/search/LSP/run-task end-to-end.
- Performance regression suite for cold start, file-tree, and editor latency.

### Migration / upstream sync
- All new crates live under `crates/`. New code under `app/src/code/`. We do not modify upstream-heavy files except where strictly required.
- Document any cross-cutting renames in `docs/UPSTREAM_SYNC.md` so the next merge from `warp-upstream/master` can absorb cleanly.

### Risk register
| Risk | Likelihood | Mitigation |
| --- | --- | --- |
| Pane-split refactor (P1.5) destabilizes existing code panel | High | Feature flag, keep old single-pane code path, comprehensive integration tests |
| LSP install prompts break for users on locked-down machines | Medium | Detect failure cleanly, fall back to "errored" state with manual install instructions |
| `git2` C dep breaks on Windows MSVC builds | Medium | Pin known-good version; consider `gix` if blockers persist |
| AI inline edits race with user typing | Medium | Snapshot buffer at submission time; refuse to apply if buffer changed unless user confirms |
| Multi-file AI edits silently corrupt repo | High | All multi-file edits go through preview-stack; no writes without explicit accept |

---

## Sequencing summary

```
P1.1  Workspace -----------┐
P1.2  Editor essentials ---┤
P1.3  LSP UX --------------┤  parallel after P1.1
P1.4  Project search ------┤
P1.6  Quick open ----------┤
                           ↓
P1.5  Tabs & splits  (high-risk, single track)
                           ↓
P1.7  Tasks/formatters ----┐
P1.8  Source control ------┤  parallel
P1.9  Inline AI -----------┤
P1.10 Settings/themes -----┘
                           ↓
                  P1 release / dogfood
                           ↓
                          P2 …
```

P1 estimated wall-clock: ~3 months at 2–3 engineers, less if more land in parallel after P1.1 ships. P1.5 is the critical-path item.

## Open questions for design review

1. Do we adopt VS Code keybindings as the default, or keep Wish/terminal-first defaults and ship a one-click "VS Code preset"?
2. `.wish/` per-workspace config: is it committed to repos by default, or `.gitignore`d like `.vscode/`? Recommend "user chooses; ship a `.wish/example.toml` template that's safe to commit."
3. Source control: `git2` (libgit2) vs `gix` (pure Rust). Recommend `git2` for P1 to avoid surprises, revisit for P2.
4. Notebooks: do we ship our own renderer or embed a Jupyter front-end? Recommend our own minimal renderer in WishUI, kernel-only reuse from Jupyter ecosystem.
5. Should the IDE surface live inside the existing code panel or become a peer top-level workspace ("IDE workspace" alongside terminal workspace)? Recommend extending the existing code panel; users should not have to choose a "mode."
