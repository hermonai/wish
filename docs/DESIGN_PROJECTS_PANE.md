# Design: Wish Projects pane

**Status**: design, implementation not started. Companion to the shipped wishcode Projects sidebar.

## Why a "pane" not a sidebar

Wish's UI is built around panes (`PaneContent` variants) — terminal, editor, agent chat. Adding Projects as a left-sidebar entry (like wishcode) would require a separate top-level concept that Wish doesn't have. A dedicated `PaneContent::Projects` is the idiomatic fit and lets users tile Projects alongside a terminal and an editor.

## Scope

End-to-end: list projects from `/v1/projects`, render rows with build/test/run/lint/format buttons, dispatch the right command to the **adjacent terminal pane** (not a captured-output box) so output streams in real-time with the user's normal terminal experience.

## File-level plan

| File | Change |
|---|---|
| `app/src/pane_group/pane/content.rs` | New `PaneContent::Projects` variant |
| `app/src/pane_group/pane/projects_view/mod.rs` | New view module (model + render) |
| `app/src/pane_group/pane/projects_view/model.rs` | `ProjectsModel` (singleton) — owns the list, polls `/v1/projects` every 30s |
| `app/src/pane_group/pane/projects_view/view.rs` | Rendering using the existing `Element` / wishui primitives — list, buttons per row |
| `app/src/server/server_api/projects.rs` | New client struct `ProjectsClient` matching wishcode's TS shape |
| `app/src/auth/auth_state.rs` | (already exposes the bearer; no change) |
| Keybinding addition in `app/src/keybindings/` | `Cmd+Shift+J` opens Projects pane |

## "Run" semantics — pane-local, not captured

Wishcode captures stdout/stderr because it has nowhere to put a live tail. Wish has terminals. A click on **Run** should:

1. Find the **nearest sibling terminal pane** (split right if none exists).
2. Send the resolved shell command to that pane's PTY input.
3. Hand focus to the terminal so keystrokes go to the running process.

This makes the IDE feel like VS Code's "Run in Terminal" without the captured-output dance.

## Auth

Wish stores Hermon bearer via `AuthState::credentials()` (the same path the i18n CLI uses). `ProjectsClient` reads it once at construction. When the user signs out, the model clears its cache and shows an empty state.

## Implementation effort

~600 lines across the seven files above. Touches the pane content enum (affects every match site that exhaustively destructures `PaneContent`, ~40 places). Worth a dedicated round.

## Why this round only ships wishcode

Wishcode's IPC + React stack lets a new sidebar surface land in ~300 lines without touching ten unrelated match sites. The same UX in Wish is genuinely bigger; deferring keeps both diffs reviewable.
