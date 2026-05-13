# Wish as a general-purpose, AI-native IDE

## Summary

Wish is already a Warp-derived terminal + agent workspace with a code panel, LSP, and tree-sitter wired in. This spec defines how Wish becomes good enough for day-to-day programming work in **C/C++, Rust, Python, TypeScript, and JavaScript** — first matching the *basic* expectations a developer has when opening VS Code, and then growing toward feature parity, all expressed through WishUI and the AI-native terminal model rather than a port of VS Code's UI.

Two non-negotiables shape every behavior in this doc:

- **Local-first**: every IDE workflow listed here works without a Hermon login or network. Cloud-only behavior must be additive.
- **AI-native**: AI is not a side panel bolted on. It lives inside the editor, the terminal, the diagnostics, and the command palette — but it never blocks a deterministic action. The user can always do the same thing without AI.

The work is organized into three phases. **P1 (Basic IDE)** is the minimum that lets a working developer use Wish for a full day on a real C/C++/Rust/Python/TS/JS project without falling back to another editor. **P2 (Toward VS Code parity)** closes the gap on the workflows VS Code users miss most. **P3 (AI-native differentiation)** is what makes Wish worth choosing over VS Code, expressed only in WishUI/terminal idioms.

---

## Target languages and what "supported" means

A language is **supported** at level *N* if every behavior at that level works on a fresh checkout, after `./script/run`, with no manual install steps beyond what Wish itself prompts for.

| Language     | LSP                          | Formatter           | Test runner     | Debugger    | Build/run task          |
| ---          | ---                          | ---                 | ---             | ---         | ---                     |
| Rust         | rust-analyzer                | rustfmt             | `cargo test` / `cargo nextest` | CodeLLDB | `cargo build` / `cargo run`     |
| C / C++      | clangd                       | clang-format        | ctest / custom  | CodeLLDB    | `cmake` / `make` / custom |
| Python       | pyright (+ ruff diagnostics) | ruff format / black | pytest / unittest | debugpy   | `python -m`             |
| TypeScript   | typescript-language-server   | prettier            | jest / vitest   | js-debug    | `npm` / `pnpm` / `bun`  |
| JavaScript   | typescript-language-server   | prettier            | jest / vitest   | js-debug    | `npm` / `pnpm` / `bun`  |

Wish must auto-discover these toolchains where they exist (`rustup`, `cargo`, `clangd`, `python3`, `node`, `pnpm`, `bun`, `npm`) and prompt-to-install only the language servers it controls (already implemented for LSP). It must never silently install anything else.

---

## Phase 1 — Basic IDE (target: Wish is usable for a full day of work)

Each item below describes desired user-visible behavior. Implementation lives in TECH.md.

### File and project navigation

1. **Open a folder as a workspace.** From the command palette ("Open folder…") or `wish .` on the CLI, the user picks a directory; the file tree, search index, LSP roots, and recent-projects list all anchor on that root. Closing and reopening Wish restores the last workspace, including expanded folders, open files, cursor position, and selected language servers.
2. **File tree mirrors disk.** New, renamed, deleted, or moved files appear in the tree within 250 ms without manual refresh. Hidden files follow `.gitignore` by default with a toggle to show all. Right-click offers New File, New Folder, Rename, Delete, Reveal in Finder/Explorer, Copy Path, Copy Relative Path.
3. **Quick file open.** `Cmd/Ctrl+P` opens a fuzzy file finder over the workspace, preferring recently opened files, with highlighted matches and a 16 ms-per-keystroke render budget on workspaces up to 200 k files.
4. **Quick symbol open.** `Cmd/Ctrl+T` opens a workspace-wide symbol picker driven by `workspace/symbol`. `Cmd/Ctrl+Shift+O` opens an in-file symbol picker driven by `documentSymbol`. Both group results by kind (function, class, struct, etc.) and respect the active language.
5. **Project-wide search.** `Cmd/Ctrl+Shift+F` opens a search pane backed by `wish_ripgrep`. Supports literal, word, regex, case-sensitive, glob include/exclude, and replace-all with per-match preview. Searching a 1 GB workspace returns first results in under 500 ms.
6. **Editor tabs and splits.** Multiple files open as tabs within a pane. Panes can be split horizontally/vertically (`Cmd/Ctrl+\` and friends). Closing the last tab in a pane closes the pane unless it's the only one. Drag a tab onto an edge to split.

### Editor essentials

7. **Per-language syntax highlighting** for C, C++, Rust, Python, TypeScript, JavaScript, JSON, TOML, YAML, Markdown, and HTML/CSS, driven by tree-sitter (already in `syntax_tree`).
8. **Indent-aware editing.** Auto-indent, smart indent on `Enter`, bracket auto-pair, auto-close strings, comment toggle (`Cmd/Ctrl+/`), block comment toggle, indent / outdent selection, expand / shrink selection by syntax node.
9. **Multi-cursor.** `Alt+Click` adds a cursor; `Cmd/Ctrl+D` adds the next occurrence of the current selection; `Cmd/Ctrl+Shift+L` selects all occurrences. All editing operations apply to all cursors.
10. **Find-in-file.** `Cmd/Ctrl+F` opens an inline find bar with regex, word, case toggles, count, and find-next/prev. `Cmd/Ctrl+H` opens replace.
11. **Go-to-line.** `Cmd/Ctrl+G` jumps to a 1-based line number, optionally `:column`.
12. **Undo / redo** is per-buffer, survives closing and reopening the file within the same session, and respects multi-cursor edits as single ops.
13. **Save behavior.** `Cmd/Ctrl+S` saves; `Cmd/Ctrl+Alt+S` saves all. A "Format on save" setting per language runs the configured formatter, falling back to LSP `textDocument/formatting` if no external formatter is configured. Format-on-save never silently corrupts a file: if the formatter fails, the unformatted save still succeeds and a non-blocking notification surfaces the error.

### Language intelligence (LSP)

14. **Hover** shows type, doc, and source link, formatted as Markdown.
15. **Go-to-definition / declaration / type-definition / implementation / references.** `F12` and friends jump in-editor; references open in a side panel with grouped, previewed results.
16. **Diagnostics** show inline as squiggles, in the gutter as severity dots, and in a workspace **Problems panel**. The Problems panel lists every diagnostic, grouped by file, sorted by severity then position; clicking a row opens the file at that range.
17. **Completion** uses LSP, ranks fuzzy matches, shows kind icons and detail/doc, supports `textEdit`, `additionalTextEdits` (for auto-imports), and snippet completions with placeholders.
18. **Signature help** appears on `(` and `,`, highlights the active parameter, and dismisses on matching close.
19. **Rename symbol** (`F2`) uses LSP `rename` and applies the workspace edit atomically with a single undo entry. If the LSP returns no edits or fails, Wish surfaces an error and does not partially apply.
20. **Code actions / quick fix.** A lightbulb appears at the cursor when actions are available; `Cmd/Ctrl+.` lists them. Server-provided fixes apply directly; refactors prompt for input where required.
21. **Format document / format selection** are bound and resolved as in (13).
22. **Run language server and view logs.** A user-visible status indicator shows each active LS's state (starting / ready / errored). The existing LSP logs view remains the source of truth.

### Terminal

23. **Wish's existing native AI terminal** remains the default terminal. The user can open a terminal pinned to the workspace root, not the home dir, with a single shortcut.
24. **Run config from the command palette.** "Run task…" lists discovered build/run/test commands (Cargo, npm scripts, Makefile targets, Python scripts, CMake presets) plus user-defined entries from a per-workspace `wish.tasks.toml`. Picking one runs it in a Wish terminal block.
25. **Output → Problems.** When a recognized compiler emits errors (rustc, clang, gcc, tsc, python tracebacks, ruff, eslint), Wish parses the output and adds entries to the Problems panel that link back to file:line:col. Existing terminal block parsing is the seam.

### Source control (basic)

26. **Source Control panel** lists changed files in the active workspace's git repo, grouped by Untracked / Modified / Staged / Conflicted. Clicking a file opens a diff against `HEAD` or `--staged` as appropriate.
27. **Stage / unstage / discard / commit** are available from the panel, including staging hunks via the inline-diff UI that already exists.
28. **Branch indicator** in the status bar shows the current branch and ahead/behind counts; clicking opens a branch switcher.

### Settings, themes, and keybindings

29. **Per-workspace settings** live in `.wish/settings.toml` at the workspace root. They override user settings for that workspace only. Settings cover formatter choice per language, format-on-save, tab size, end-of-line, exclude globs, and LSP overrides.
30. **Keybindings** are configurable. Wish ships a default set roughly aligned with VS Code's where it doesn't conflict with terminal/agent shortcuts; conflicts are documented and surfaced in the keybindings UI.
31. **Themes** include at least a Wish Dark and Wish Light theme tuned for both editor and terminal, with WishUI tokens exposed so users can customize.

### AI in the basic IDE

32. **Inline AI edit on selection.** Select code, press `Cmd/Ctrl+I`, type a natural-language instruction. Wish streams an inline diff in the editor; user can accept all, reject, or accept hunks individually. AI never writes to disk before user accepts.
33. **AI quick fix on diagnostics.** Any diagnostic with no LSP-provided fix offers an "Ask Wish to fix" code action that runs an inline edit scoped to the diagnostic range.
34. **AI explain.** Right-click selection → "Explain" opens a chat-side panel pre-populated with the selection and file/line context.
35. **Agent Mode** continues to work as today; the new IDE surface is a peer to it, not a replacement.
36. **Live workspace state for the agent.** Whenever the user invokes the agent — inline edit, chat, explain, fix — Wish injects the workspace's *live, deterministic* state as structured context. The agent never has to ask "what's broken?" / "where am I?" / "what was I doing?" — it already knows, in the same shape the human sees. The data is the canonical seam: every block flows from the same source of record (`DiagnosticsAggregatorModel`, `ProjectManagementModel`, `OpenedFilesModel`, `History`) that the panels/UI read from. Human and agent never see a stale or divergent view. **Shipped today as `FeatureFlag::AgentLiveWorkspaceContext`** (on in dogfood). Every Wish-chat user message is silently prefixed with the following tagged blocks (each emitted only when its source has content):
    - `<active_project>` — the workspace root the user has open.
    - `<workspace_diagnostics>` — every LSP diagnostic, sorted by severity then position.
    - `<recently_opened_files>` — the files the user has touched in this session, most recent first.
    - `<recent_terminal_commands>` — the user's last N commands with exit codes and `✓`/`✗`/`•` markers. **Wish-unique** — VS Code and Cursor cannot do this because their terminal output is unstructured.

    On a clean session with none of the above signals, the wire is exactly the user's original message — no preamble, no behavior change. The conversation history records the user's original text; only the wire to the LLM carries the preamble.

### Performance and reliability budgets

- Cold start (no project) ≤ 1.5 s on M-class Mac, ≤ 2.5 s on a 4-core x86 Linux laptop.
- Open a workspace of 50 k files ≤ 1 s to first paint of the file tree.
- Keystroke-to-paint latency in the editor ≤ 16 ms p99 on 50 k-line files for the active viewport.
- LSP startup never blocks the editor; the editor accepts edits while the LS is starting.
- A crashed LSP restarts up to 3 times in 60 s with exponential backoff before falling into "errored" state with a one-click restart.

---

## Phase 2 — Toward VS Code parity

These are the workflows VS Code users miss after a week of using P1.

1. **Debugger (DAP).** A `wish_dap` integration drives CodeLLDB (Rust/C/C++), debugpy (Python), and js-debug (Node). UI surfaces: breakpoints in the gutter, a debug toolbar (continue, step over/in/out, restart, stop), a Variables panel, a Call Stack panel, a Watch panel, a Debug Console that is itself a Wish terminal block. Launch configurations live in `.wish/launch.toml` with per-language templates and a one-click "Generate launch config" for the current file.
2. **Test explorer.** Per-language adapters (cargo test/nextest, pytest, jest/vitest, ctest) feed a tree view of test cases. Per-test run, debug, and "show output." Failures link to file:line. AI offers "Explain failure" and "Propose fix" against the failing test's last output.
3. **Refactoring menu.** LSP-provided refactors are surfaced clearly (extract function, inline, move). Where the LSP doesn't support a refactor, Wish offers an AI-driven refactor that produces a previewed multi-file diff.
4. **Outline / breadcrumbs.** A symbol breadcrumb above the editor shows enclosing scope; an Outline view in the side bar mirrors `documentSymbol`.
5. **Sticky scroll.** When scrolled inside a function, the function header stays pinned at the top of the editor.
6. **Minimap** (toggleable, off by default).
7. **Code lens** for references count, run/debug actions on test functions, and Wish-specific actions ("Ask Wish about this function").
8. **Snippets.** A shared snippet format (compatible with VS Code snippet JSON) plus a Wish-native YAML form. Per-language and per-workspace snippets.
9. **Diff and merge.** A three-way merge editor backed by the same inline-diff plumbing already in `crates/editor`. Triggered automatically on `git merge`/`git rebase` conflicts when invoked from the source control panel.
10. **Notebooks (Python).** A minimal Jupyter-compatible notebook view for `.ipynb` files, kernel discovery via `jupyter kernelspec`, cell-by-cell run, inline matplotlib outputs.
11. **Per-language formatters fully integrated.** rustfmt, clang-format (respecting `.clang-format`), ruff/black, prettier (respecting `.prettierrc`), each runnable on save / on save-typed / on demand.
12. **Workspace tasks.** A `.wish/tasks.toml` schema supersedes the basic auto-discovery of P1, with per-task type (`build`, `test`, `run`, `lint`), problem matchers, and dependencies.

---

## Phase 3 — AI-native differentiation

These behaviors are what make Wish meaningfully different from VS Code, expressed in WishUI and the terminal-block model. None of them ships before P1 is solid; P3 features are listed as candidates, not commitments.

1. **Ghost-text completion** alongside LSP completions, sourced from Wish's local-or-cloud model. The user can configure preference: LSP-first, AI-first, or interleaved. Tab accepts a word; `Cmd/Ctrl+→` accepts the full suggestion.
2. **Conversational command palette.** `Cmd/Ctrl+K` accepts a natural-language instruction that resolves to either a command palette action or an inline AI edit, with a visible preview before anything runs.
3. **AI-aware terminal blocks.** Existing Warp-style command blocks gain a "what does this output mean?" affordance. Errors are auto-summarized, file:line links auto-promote into Problems entries.
4. **Repo-aware chat.** Agent Mode gains a structured "context tray" populated by the IDE: the active file, current selection, recent diagnostics, last failing test, current PR. The user sees and can edit the tray before sending.
5. **Multi-file AI edits.** "Refactor across these files" produces a single previewed diff stack with per-file accept/reject. Built on top of P2's diff/merge plumbing.
6. **Skills as IDE actions.** Existing skills (`spec-driven-implementation`, `rust-unit-tests`, `add-telemetry`, `fix-errors`, `diagnose-ci-failures`, etc.) appear as named, parameterized actions in the IDE command palette, not just inside agent chat.
7. **AI pair programming on the same file.** Wish renders agent-proposed edits as a "ghost author" overlay distinct from the user's cursor; the user can scrub through proposals like git history within the session.

---

## Language support — what's the Wish equivalent of "VS Code plugins"?

VS Code's success rests on a marketplace of third-party extensions. Wish is taking a different bet, and this section is the design statement so the path is intentional, not accidental.

### What Wish has today (P1 + the baked-in surface)

- **First-class LSP support for C, C++, Rust, Python, TypeScript, JavaScript**, plus Go and a `generic` adapter, all in [crates/lsp/src/servers/](../../crates/lsp/src/servers). Auto-install via `crates/lsp/src/install.rs`. New language servers can be added today by writing ~80 lines of Rust per language (config + executable detection).
- **Tree-sitter highlighting + indent queries** for every supported language, in [crates/syntax_tree/src/queries/](../../crates/syntax_tree/src/queries). Adding a language here is a `.scm` query file + a tree-sitter grammar dependency.
- **The skills system** (the `.agents/skills` directory and `npx skills` lockfile) — Wish's existing agentic plugin model. Skills extend the *agent's* capabilities (write tests, review PRs, fix CI, etc.), not the editor's.

### The plugin question

VS Code's model is "lots of plugins, you pick what you want." That gives breadth at the cost of:
- Massive curation and discovery problem (hundreds of "auto-format on save" plugins, all subtly different).
- Wildly inconsistent UX (every plugin reinvents settings UI, command palette entries, status-bar widgets).
- Security surface area (each plugin runs arbitrary JS in your editor process).
- Lots of plugins that should just be… built in. (Rust support, Python support, formatter-on-save.)

Wish bets on the opposite axis: **batteries-included for languages, plug-in extensions for *workflows* (skills).** Concretely:

1. **Language support is a first-party, in-tree concern.** Adding a new language is a Wish-team PR, not a third-party extension. This guarantees:
    - Consistent UX (the LSP UX surface is one code path, not N).
    - One LSP install / status / problems-panel flow.
    - Same diagnostic aggregator (slice 4) for every language. The AI context tray (slice 6+) doesn't care which language served the diagnostic.

2. **Workflow extensions are skills.** Already exists. A skill is a tagged piece of agent prompt + behavior, versioned in a lockfile. Wish's `.agents/skills` directory plus the `npx skills` CLI is the install surface — equivalent to a VS Code extension but operating on the agent's prompt seam, not the editor process.

3. **AI-native language onboarding.** For a language Wish doesn't ship support for yet, the user's option today is "open a PR." The longer-term play is: ask Wish itself to add a language. Given a workspace with `.kt` files and a `build.gradle.kts`, the agent should be able to:
   - Detect the language (extension + manifest)
   - Recommend the LSP server (`kotlin-language-server`)
   - Generate a skeleton LSP adapter in `crates/lsp/src/servers/kotlin.rs`
   - Fetch the tree-sitter grammar
   - Open a PR
   That's Wish's "marketplace" — the agent is the package manager.

### Roadmap for language coverage

Short list of languages to add baked-in, in priority order:

| Language | LSP server | Status |
| --- | --- | --- |
| Rust | rust-analyzer | shipped |
| C / C++ | clangd | shipped |
| Python | pyright | shipped |
| TypeScript / JavaScript | typescript-language-server | shipped |
| Go | gopls | shipped |
| **Kotlin** | kotlin-language-server | next |
| **Swift** | sourcekit-lsp | next |
| **Java** | jdtls | next |
| **C#** | OmniSharp / Roslyn | next |
| **Ruby** | solargraph / ruby-lsp | next |
| **PHP** | intelephense | nice-to-have |
| **Zig** | zls | nice-to-have |
| **Lua** | lua-language-server | nice-to-have |
| **Haskell** | haskell-language-server | nice-to-have |
| **OCaml** | ocaml-lsp | nice-to-have |
| **Elixir** | elixir-ls | nice-to-have |
| **Markdown / MDX** | (no LSP needed — tree-sitter only) | shipped |
| **JSON / YAML / TOML** | (tree-sitter + schemas, no LSP needed) | shipped |
| **HTML / CSS** | vscode-langservers-extracted | nice-to-have |

Each new language is a focused slice of ~150 LoC: an adapter in `crates/lsp/src/servers/`, a `LanguageId` variant in `crates/languages`, and `.scm` query files in `crates/syntax_tree/queries/`. The LSP UX, diagnostic aggregator, agent context tray, problems badge, and AI quick-fix flow are all already wired and language-agnostic.

### Why this is "best in the world" territory

VS Code's plugin model is its biggest strength *and* its biggest weakness. The breadth is breathtaking; the quality is uneven; the security model is poor; the agentic-workflow integration is non-existent (every plugin makes its own AI integration).

Wish's bet:
- **Language support: built-in.** Same UX, same agent context, same quality bar, every language.
- **Workflow extensions: skills.** Agent-prompt-shaped, version-locked, declarative, safe.
- **New language? Ask the agent.** The agent + the skills system + open-source LSP ecosystem mean the user describes what they want and Wish authors the support.

This is the AI-native answer to "VS Code has more plugins": the plugin author *is* the agent.

## The terminal half of the bet — Wish vs. vim + tmux

Wish's heritage is a terminal. The IDE features above are the *new* product layer; the terminal underneath has to stand on its own against vim + tmux + a tiling window manager, which is the current state-of-the-art for serious terminal-first developers. This section documents what's already shipped, what makes Wish defensibly better than vim+tmux for "vibe coding" — and what's still missing.

### What's already shipped (you may have missed these)

**Wishify** — the feature that makes Wish a real upgrade for remote-shell work, not just a local terminal.

When you `ssh user@host` from a Wish tab, the remote shell normally has no idea it's running inside Wish — so you lose all the structured-block magic (exit codes, command boundaries, AI-attachable blocks) the moment you connect. Wishify changes that: on the first command after `ssh`, Wish injects a small bootstrap script into the remote shell that installs the same OSC marker hooks the local shell has. From that point on, every remote command appears as a proper Wish block on your local UI — with exit code, runtime, pwd, git branch — exactly as if you ran it locally. The remote rcfiles are sourced cleanly; the user's shell is unchanged after disconnect.

Settings live in [`settings_view/warpify_page.rs`](../../app/src/settings_view/warpify_page.rs) (titled "Wishify" in the UI). Per-host opt-out, denylists for shells/hosts that can't be safely bootstrapped, and a separate toggle for SSH specifically.

This is shipped today. If you ssh into a remote machine from a Wish terminal, you should already be seeing block boundaries on the remote command output.

**SSH + tmux wrapper** — the optional resumability layer on top of Wishify.

`tmux` is what serious SSH users reach for so a disconnected mosh / `ssh` doesn't kill their session. Wish has a *first-class* SSH-tmux wrapper (gated by `FeatureFlag::SSHTmuxWrapper`, on in dogfood): when you `ssh` to a host, Wish optionally starts (or attaches to) a named tmux session on the remote host transparently. If your link drops, your remote session is preserved; the next time you connect, Wish reattaches without you typing `tmux attach -t …`. The block-aware OSC markers still work *inside* tmux, so you get block UI + session resumability without manual tmux conf.

This means **you don't need to know tmux to get tmux's reliability for remote work**. Settings live alongside Wishify in [`settings/ssh.rs`](../../app/src/settings/ssh.rs).

**Vim emulation** — [`crates/vim`](../../crates/vim) is a from-scratch vim-mode for Wish's editor. Modal editing, motions, operators, common bindings. Toggled per-buffer or globally.

**Native pane splits** — Wish has tmux-style pane splitting built into the workspace ([`crates/pane_group`](../../crates/pane_group)). Horizontal/vertical splits, drag to resize, keyboard-driven navigation. No tmux config required, no `tmux send-keys`-style indirection — splits are native UI elements.

### Why this beats vim + tmux + iTerm for "vibe coding"

vim+tmux is great for two things: speed of keyboard manipulation, and remote session resilience. Wish keeps both of those *and* adds:

1. **The terminal is structured, not a glyph buffer.** Every command is a `Block` with command text, exit code, runtime, pwd, git branch, and output as a structured record. tmux + iTerm have a 2D character grid; Wish has a typed event stream. This is what makes everything else possible.
2. **The agent reads the same structure the human reads.** Slices 4–12 of the IDE work injected `<active_project>`, `<git_status>`, `<workspace_diagnostics>`, `<recently_opened_files>`, and `<recent_terminal_commands>` into every Wish-chat turn. No vim + tmux + Copilot setup can do this — Copilot has no access to your tmux scrollback in a structured form, your git status, or your LSP diagnostics simultaneously.
3. **The remote shell is first-class.** Wishify means `ssh host` works without giving up block UI. SSH+tmux wrapper means it works *and* survives disconnects. vim+tmux requires you to install + configure tmux on every host yourself.
4. **The vim keybindings still work.** [`crates/vim`](../../crates/vim) means you don't pay an ergonomic tax to use Wish over a real vim — you get modal editing in the same editor that has LSP + diagnostics + AI inline edit.
5. **Splits without tmux's config language.** Pane splits are a single keystroke; they remember their layout per workspace; you can drag-resize. Compared to a tmux config + `bind` lines, this is "0 config to set up, 1 keystroke to use."

### What's still missing to truly own the segment

Honest list of gaps vs. a power-user vim+tmux setup:

- **Macros.** Wish's vim mode has basic motions and operators, not full vim macros (`q…q` recording, `@a` playback). Power vim users *will* notice.
- **Customizable statusline.** tmux/vim users heavily customize their statusline. Wish has a fixed footer (LSP indicator, diagnostic badge, version, etc.) — extensible via skills but not via free-form Lua/script today.
- **Multiplexing detached from the GUI.** tmux runs without a GUI; Wish today is a GUI app. A "Wish core" running as a daemon that Wish GUI clients can attach to (so closing the GUI doesn't kill running sessions) is the natural next step.
- **Plugin parity.** Vim/tmux have decades of plugin ecosystems. Wish's answer is "skills + first-party language support," see the language-support section above — but for power-user mechanical customization (custom motions, custom split arrangements, custom statusline widgets), the gap is real.
- **Lua-level scriptability.** Neovim's killer feature for senior users is "I can rewire any behavior in Lua." Wish's settings system is declarative; the scriptability surface for "make this keybinding run my custom Rust closure" is the agent / skill model, not embedded Lua. Different bet; some users will prefer Lua.

These are the right things to prioritize *after* the IDE+AI half is rock solid. The vision is: **a developer's last terminal — one tool for the local editor, the remote shell, the AI conversation, and the workflow extensions, all sharing one structured event stream.**

## Product boundaries — what `wish` is, what it isn't

Three sibling products share one architecture; this section pins down which surface belongs to which product so future work doesn't blur them.

| Product | What it is | Distribution |
| --- | --- | --- |
| **`wish`** | The desktop GUI + headless CLI. The vibe-coding terminal-and-IDE in one binary. Renders, hosts the agent, owns workspace identity. *This is the product this spec documents.* | One desktop install. Includes `wish` GUI app and `wish` CLI subcommands (`crates/wish_cli`). |
| **`wishd`** | Trusted local daemon. gRPC over Unix socket. Owns privileged ops: fs, git, process, terminal, search index, capability + cell-trust evaluation. Started alongside the desktop app. *Lives in a separate repo at `/Users/wenyan/ClaudeProjects/wishd`.* | Bundled with desktop installs; can also be installed standalone for CLI / scripting clients. |
| **`wishcode`** | Web/Electron product on top of wishd. Different rendering surface, same daemon. *Lives in a separate repo.* | Separate install (web app or Electron desktop). |
| **`hermon-server`** | Cloud backend (optional). Auth, model routing, sync, billing, governance. *Lives in a separate repo.* | Hosted service; self-hosting supported for dev. |

`wish-cli` (the CLI subcommands you'd invoke as `wish agent run …`, `wish .`, `wish path/to/file.rs:42:5`) stays **inside the `wish` workspace** — it's the headless side of one product. Same Rust types, same gRPC client to `wishd`, same install. Spinning it out as its own crate workspace would force duplicate auth/capability/client plumbing for zero user benefit.

The integration target: `wish` is a *thin renderer + agent host*, `wishd` is the trusted operations layer, `hermon-server` is the optional cloud control plane. This is the AI-native answer to "Cursor is a fork of VS Code, Continue.dev is a plugin in VS Code": Wish's privileged operations layer is decoupled from the rendering layer, so any future Wish-family product (a web Wishcode, a mobile Wish, a scriptable wish-cli) can share the same trusted local-daemon foundation.

## Out of scope

- Web/browser variant of the IDE (Wish is native).
- 3D/spatial UI, Storm/Finalverse work.
- Plugin/extension marketplace beyond the existing skills system.
- Remote dev (SSH, dev-containers, WSL) — explicitly deferred to a later spec.
- Live share / multiplayer editing — deferred.
- Visual GUI builders, no-code surfaces.

## Success signals

- A Wish engineer can land a multi-file PR in a Wish-supported language end-to-end without opening another editor.
- Time-to-productivity for a new user opening a fresh repo is ≤ 60 seconds: open folder, file tree appears, LSP starts, first edit accepted.
- Crash-free editing sessions ≥ 99.5 % over a rolling 7-day window.
- AI inline edits land an accepted diff ≥ 60 % of the time on tasks scoped to a single function.
