## Roadmap — URE × wishUI (2D · 3D · Tensors)

> **Why this doc exists.** One session can't ship a real Universal Reality
> Engine (URE) on top of wishUI without something giving (depth, breadth,
> or stability). This roadmap breaks the work into ~7 sessions, each one
> a self-contained slice that compiles, tests, and ships. The earlier
> sessions build *substrate*; later sessions build *panes* that hang off
> that substrate; the last sessions wire the four desktop items we still
> owe (Wish Projects pane, legacy-agent routing, native Architecture
> renderer panel, deep wishcode↔Hermon integration).

### Architecture in one screen

```
                                         ┌───────────────────────────┐
                                         │  wish-world-model         │
                                         │  WishWorld · SemanticId   │
                                         └────────────┬──────────────┘
                                                      │ project
                                                      ▼
        ┌─────────────────────────┐   layout    ┌───────────────────┐
        │ wish-canvas-core (2D)   │◀────────────┤ wish-world-studio │
        │ Canvas · CanvasNode     │             │ world → canvas    │
        │ Tensor · TensorSpec*    │             └───────────────────┘
        └────┬────────────┬───────┘
             │            │
   draws ▼            slices ▼
   ┌────────────────┐  ┌───────────────────┐         ┌───────────────┐
   │ wish-canvas    │  │ wish-tensor-view* │         │  wish-render  │
   │ (egui pane)    │  │ 1D · 2D · 3D      │         │ eframe + wgpu │
   └────────────────┘  └─────────┬─────────┘         │ scene3d · …   │
                                 │                   └───────┬───────┘
                                 │  uploads textures         │ hosts
                                 ▼                           ▼
                       ┌─────────────────────────────────────────────┐
                       │  Wish app — PaneContent::{                  │
                       │     UreCanvas, Tensor, Architecture,        │
                       │     Projects, Terminal, …                   │
                       │  }                                          │
                       └─────────────────────────────────────────────┘
```

`*` = added by this roadmap.

### Session arc

Each session is one PR / one commit on `master`. Order matters — later
sessions assume the substrate from earlier ones.

| # | Title | Crate(s) touched | Outcome | Status |
|---|---|---|---|---|
| **1** | **Tensor substrate** | `wish-canvas-core` | `TensorSpec`, `TensorDType`, `TensorRef`, slicing math, `CanvasNodeKind::Tensor`. Pure data + tests, no UI. | ✅ shipped |
| **2** | **Tensor sampling + golden constructors** | `wish-canvas-core` | `read_f32`, bilinear `sample_2d_bilinear`, `stats` / `stats_for_slice`, `zeros_f32` / `linspace_f32` / `eye_f32` / `from_fn_f32`. Renderer-ready data layer. | ✅ shipped |
| **3** | **Tensor-aware canvas rendering** | `wish/app::canvas_pane` | `WishCanvasElement` renders `CanvasNodeKind::Tensor` as an inline heatmap — rank-1 row / rank-2 grid / rank-≥3 first plane, color-mapped through min/max stats, capped at 32×32 cells with nearest-neighbor downsample. | ✅ shipped |
| 4 | URE canvas pane (proper PaneContent) | `wish-canvas`, `wish-app` | New `PaneContent::UreCanvas` that hosts a `Canvas` in a Wish pane (not just an embedded element). |   |
| 4 | 3D toggle on canvas pane | `wish-render`, `wish-canvas`, `wish-app` | `View → 3D` projects canvas nodes via `scene3d::Camera3D`. Force-directed layout in 3D. |   |
| 5 | Tensor view pane | new `wish-tensor-view`, `wish-app` | `PaneContent::Tensor` — 1D line, 2D heatmap, 3D voxel slices over a `TensorRef`. |   |
| 6 | Wish Projects pane | `wish-app` (per `DESIGN_PROJECTS_PANE.md`) | `PaneContent::Projects`, runs SDLC into adjacent terminal panes. |   |
| 7 | Native Architecture renderer | `wish-app` over `wish-render` | `PaneContent::Architecture` — 3D directory cubes / dep graph. |   |
| 8 | Legacy-agent → Hermon MVP | `wish-app` (per `DESIGN_LEGACY_AGENT_HERMON_ROUTING.md`) | `AmQuerySuggestions` proxied through Hermon, behind a feature flag. |   |
| 8+ | Full legacy coverage, time-series tensors, tensor diff | TBD | Stretch — only if 1–7 are stable. |   |

### Invariants kept across all sessions

- **No GraphQL leaks into pane code.** Sessions 5, 7 use the same Hermon
  REST client used by `wish project` / `wish auth`.
- **`wish-canvas-core` stays UI-free.** Sessions 2, 4 own all egui code.
- **Every session lands with `cargo test` green on its crate.** A failing
  session blocks the next.
- **No new top-level workspace deps without a recorded reason** in the
  PR/commit. The dep graph stays small enough to audit.
- **Tensors don't allocate by default.** `TensorRef` is the canonical
  shape — inline data is opt-in for the 1MiB-or-less smoke tests.

### What ships at the end

A Wish window where a single workspace tab can carry: a `UreCanvas`
pane showing the project as a semantic graph, an `Architecture` pane
showing the same repo as 3D cubes, a `Tensor` pane showing an embedding
matrix or attention heatmap, a `Projects` pane listing SDLC bookmarks
with one-click run, and a `Terminal` pane catching the output — all
talking to the same `WishWorld` underneath. wishcode's sidebar lights
up against the same Hermon `projects` table so the two clients stay
in sync.

That's URE — not a product name, an actual usable substrate.
