# Design: native Architecture renderer pane

**Status**: design, implementation not started.

## Scope

Surface the existing `wishui-3d` rendering pipeline as a `PaneContent::Architecture` variant so users can split a window into a 3D scene alongside their terminal/editor. Use it to visualize codebase structure, dependency graphs, and tensor/data shapes (the URE direction discussed earlier in the thread).

## What `wishui-3d` already provides

Looking at the crate today:

- A `Scene` abstraction with cameras, lights, meshes
- Metal/WGPU backend selection
- A `SceneCanvas` widget that renders into a wishui surface

This is enough to host a static-content panel. What's missing for a true "Architecture" view:

- Codebase parser → scene graph (`syn` for Rust, `tree-sitter` for everything else)
- Dependency-edge layout (force-directed or hierarchical)
- Interaction: click a node → jump to file in the editor pane

## File-level plan (MVP — static scene)

| File | Change |
|---|---|
| `app/src/pane_group/pane/content.rs` | `PaneContent::Architecture` variant |
| `app/src/pane_group/pane/architecture_view/mod.rs` | Pane view module |
| `app/src/pane_group/pane/architecture_view/scene_builder.rs` | Walks the current project root, builds a node-per-file scene |
| `app/src/pane_group/pane/architecture_view/view.rs` | Renders the `Scene` into a wishui surface |

The MVP renders one cube per top-level directory, sized by file count. Even that ships visible "wow" value the existing 3D pipeline doesn't get to flex today.

## File-level plan (full — interactive)

Add to the MVP:

- `parser/rust.rs` — `syn`-based dependency edges
- `parser/typescript.rs` — `tree-sitter-typescript` for npm projects
- `layout/force.rs` — force-directed graph layout
- Pick events → emit a `JumpToFile { path, line }` event that the editor pane subscribes to

## Implementation effort

- MVP: ~400 lines + the inevitable wishui-3d API friction, ~1 day if `SceneCanvas` is genuinely renderable from a pane today (untested).
- Full interactive version: ~1500 lines + a parser crate per supported language, ~1 week.

## Why this is "biggest" of the four

The other three deferred items are all client-side wiring against shipped server endpoints. This one needs:

- Parser infrastructure (new dep)
- Layout engine (new dep, or roll a force-directed implementation)
- A way to drive view→view events between panes (architecture pane → editor pane "jump")

Best done as its own multi-round arc once the simpler IDE surfaces (Projects pane, command palette) are landed and stable.
