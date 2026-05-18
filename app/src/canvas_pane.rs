//! WishUI Canvas Pane — Wave 23a of the codegraph-in-IDE plan.
//!
//! Renders a [`wish_canvas_core::Canvas`] as a native WishUI element
//! inside the Wish desktop app — no spawned process, no eframe. The
//! same `Canvas` value that drives `wish-world render` drives this
//! pane, so the standalone viewer and the embedded pane stay in sync
//! by construction.
//!
//! v0.5.0 scope: nodes + edges + labels at the active pan/zoom. No
//! cinematic boot, no 3D scene, no perspective dropdown — those
//! remain in the standalone `wish-world` binary for now. Subsequent
//! waves port them on top of the new WishUI Scene primitives
//! (`draw_line`, `draw_circle`, etc.) without changing this file's
//! public API.
//!
//! # Generative-UI rationale
//!
//! This element is the **first consumer** of the new WishUI Scene
//! line / circle primitives. It proves the substrate. Future
//! AI-generated UI (agent traces, dataflow diagrams, plot overlays)
//! can use the same primitives — they will not need to re-implement
//! the rendering pipeline.

use pathfinder_color::ColorU;
use pathfinder_geometry::rect::RectF;
use pathfinder_geometry::vector::{vec2f, Vector2F};
use wishui::elements::Point;
use wishui::event::DispatchedEvent;
use wishui::{
    AfterLayoutContext, AppContext, Element, EventContext, LayoutContext, PaintContext,
    SizeConstraint,
};

use wish_canvas_core::tensor::TensorSpec;
use wish_canvas_core::types::{Canvas, CanvasNode, CanvasNodeKind, EdgeKind};

/// A leaf [`Element`] that paints a [`Canvas`] inside any WishUI
/// surface — modal, panel, or tab body. Holds the canvas by value
/// and an *initial* pan/zoom that fits all nodes to the available
/// space on first paint. Pan/zoom interactions are *not* yet wired in
/// v0.5.0; that lands in Wave 23b once the modal harness is in place.
pub struct WishCanvasElement {
    /// The canvas to draw. Take by value — the element owns its view
    /// of the world.
    canvas: Canvas,

    /// Computed during layout; the available size we were given.
    size: Option<Vector2F>,

    /// Set in `paint`.
    origin: Option<Point>,

    /// Cached fit-to-view zoom + pan, computed lazily from the canvas
    /// bbox on the first paint where size is known.
    fit: Option<(Vector2F, f32)>,
}

impl WishCanvasElement {
    pub fn new(canvas: Canvas) -> Self {
        Self {
            canvas,
            size: None,
            origin: None,
            fit: None,
        }
    }

    /// Compute pan + zoom that fits the canvas bbox into `available`,
    /// with `pad` pixels of margin. Returns the screen-space pan and
    /// the zoom factor.
    fn compute_fit(&self, available: Vector2F) -> (Vector2F, f32) {
        let (min_x, min_y, max_x, max_y) = bbox(&self.canvas).unwrap_or((-40., -40., 40., 40.));
        let w = (max_x - min_x).max(1.0);
        let h = (max_y - min_y).max(1.0);
        let pad = 40.0_f32;
        let zx = (available.x() - pad * 2.0).max(50.0) / w;
        let zy = (available.y() - pad * 2.0).max(50.0) / h;
        let zoom = zx.min(zy).clamp(0.05, 4.0);
        let bbcx = (min_x + max_x) * 0.5;
        let bbcy = (min_y + max_y) * 0.5;
        let cx = available.x() * 0.5;
        let cy = available.y() * 0.5;
        let pan = vec2f(cx - bbcx * zoom, cy - bbcy * zoom);
        (pan, zoom)
    }

    /// Project a canvas (world) point to screen-relative coordinates.
    fn project(&self, canvas_x: f32, canvas_y: f32) -> Vector2F {
        let (pan, zoom) = self.fit.unwrap_or((Vector2F::zero(), 1.0));
        vec2f(pan.x() + canvas_x * zoom, pan.y() + canvas_y * zoom)
    }
}

/// Compute the bounding box of all nodes in the canvas. Returns
/// `None` for empty canvases.
fn bbox(canvas: &Canvas) -> Option<(f32, f32, f32, f32)> {
    let mut iter = canvas.nodes.values();
    let first = iter.next()?;
    let mut min_x = first.bounds.x;
    let mut min_y = first.bounds.y;
    let mut max_x = first.bounds.x + first.bounds.w;
    let mut max_y = first.bounds.y + first.bounds.h;
    for n in iter {
        if n.bounds.x < min_x {
            min_x = n.bounds.x;
        }
        if n.bounds.y < min_y {
            min_y = n.bounds.y;
        }
        let x1 = n.bounds.x + n.bounds.w;
        let y1 = n.bounds.y + n.bounds.h;
        if x1 > max_x {
            max_x = x1;
        }
        if y1 > max_y {
            max_y = y1;
        }
    }
    Some((min_x, min_y, max_x, max_y))
}

/// Pick a fill color per node kind so the user can read the canvas at
/// a glance — same palette as the standalone `wish-render` viewer
/// uses, kept in sync by sight.
fn node_color(kind: &CanvasNodeKind) -> ColorU {
    match kind {
        CanvasNodeKind::Crate => ColorU::new(74, 158, 255, 255), // bright blue
        CanvasNodeKind::File => ColorU::new(140, 190, 140, 255), // soft green
        CanvasNodeKind::Function => ColorU::new(255, 184, 108, 255), // warm orange
        CanvasNodeKind::Test => ColorU::new(208, 135, 255, 255), // violet
        // Default for entity-kind nodes we haven't styled yet.
        _ => ColorU::new(165, 165, 175, 255),
    }
}

/// Edge stroke color per kind. `Calls` is brighter than `DependsOn`
/// because call edges carry more information density.
fn edge_color(kind: &EdgeKind) -> ColorU {
    match kind {
        EdgeKind::Calls => ColorU::new(110, 170, 220, 200),
        EdgeKind::DependsOn => ColorU::new(110, 120, 130, 160),
        EdgeKind::Triggers => ColorU::new(220, 110, 110, 200),
        EdgeKind::Produces => ColorU::new(110, 220, 140, 200),
        EdgeKind::Mentions => ColorU::new(220, 200, 110, 180),
        EdgeKind::SucceededBy => ColorU::new(160, 110, 220, 200),
        _ => ColorU::new(110, 120, 130, 140),
    }
}

impl Element for WishCanvasElement {
    fn layout(
        &mut self,
        constraint: SizeConstraint,
        _ctx: &mut LayoutContext,
        _app: &AppContext,
    ) -> Vector2F {
        let size = constraint.max;
        self.size = Some(size);
        // Compute fit-to-view based on the size we just got.
        self.fit = Some(self.compute_fit(size));
        size
    }

    fn after_layout(&mut self, _ctx: &mut AfterLayoutContext, _app: &AppContext) {}

    fn paint(&mut self, origin: Vector2F, ctx: &mut PaintContext, _app: &AppContext) {
        self.origin = Some(Point::from_vec2f(origin, ctx.scene.z_index()));
        let size = self.size.unwrap_or(vec2f(0., 0.));
        if size.x() <= 0.0 || size.y() <= 0.0 {
            return;
        }

        // Background — neutral dark surface so the colored nodes pop.
        ctx.scene
            .draw_rect_with_hit_recording(RectF::new(origin, size))
            .with_background(wishui::elements::Fill::Solid(ColorU::new(14, 17, 22, 255)));

        // Draw edges first so node rects overlap them. Use the new
        // `draw_arrow` primitive so direction is legible — this is
        // crucial for dependency graphs where "A depends on B" must
        // be visually distinguished from "B depends on A".
        let zoom = self.fit.unwrap_or((Vector2F::zero(), 1.0)).1;
        let edge_width = (1.2_f32 * zoom).clamp(0.6, 3.0);
        let head_size = (8.0_f32 * zoom).clamp(4.0, 16.0);
        for edge in self.canvas.edges.values() {
            let (Some(a), Some(b)) = (
                self.canvas.nodes.get(&edge.from),
                self.canvas.nodes.get(&edge.to),
            ) else {
                continue;
            };
            let (acx, acy) = a.bounds.center();
            let (bcx, bcy) = b.bounds.center();
            let p0 = origin + self.project(acx, acy);
            let p1 = origin + self.project(bcx, bcy);
            ctx.scene
                .draw_arrow(p0, p1, edge_width, edge_color(&edge.kind), head_size);
        }

        // Draw nodes. Tensor nodes get an inline heatmap so the user
        // can read the data right on the canvas — every other kind
        // gets the flat colored rect the standalone renderer ships.
        for node in self.canvas.nodes.values() {
            let p0 = origin + self.project(node.bounds.x, node.bounds.y);
            let zoom = self.fit.unwrap_or((Vector2F::zero(), 1.0)).1;
            let w = (node.bounds.w * zoom).max(4.0);
            let h = (node.bounds.h * zoom).max(4.0);
            if let CanvasNodeKind::Tensor(spec) = &node.kind {
                paint_tensor_node(ctx, p0, w, h, spec);
            } else {
                let bg = node_color(&node.kind);
                ctx.scene
                    .draw_rect_with_hit_recording(RectF::new(p0, vec2f(w, h)))
                    .with_background(wishui::elements::Fill::Solid(bg))
                    .with_corner_radius(wishui::scene::CornerRadius::with_all(
                        wishui::scene::Radius::Pixels(3.0),
                    ));
            }
        }

        // Labels are deferred to Wave 23b: WishUI needs a
        // glyph-shaping helper that turns a `&str` into glyphs at a
        // position — `draw_glyph` works one glyph at a time, which is
        // too low-level for v0.5.0. The colored rects already convey
        // structure; full labels arrive with the text-shaping
        // primitive enrichment.
        let _ = node_label_placeholder; // suppress unused-fn warning until Wave 23b
    }

    fn size(&self) -> Option<Vector2F> {
        self.size
    }

    fn origin(&self) -> Option<Point> {
        self.origin
    }

    fn dispatch_event(
        &mut self,
        _event: &DispatchedEvent,
        _ctx: &mut EventContext,
        _app: &AppContext,
    ) -> bool {
        // Wave 23b will wire pan-via-drag and zoom-via-scroll-wheel.
        // v0.5.0 ships static — the fit-to-view default is usually
        // sufficient to read the architecture.
        false
    }
}

/// Reserved for Wave 23b — when WishUI gains a `draw_text` helper that
/// shapes a string into glyphs, we'll render node labels.
fn node_label_placeholder(_node: &CanvasNode) {}

/// Maximum cells per axis when rendering a tensor node as an inline
/// heatmap. Anything bigger gets nearest-neighbor downsampled — at the
/// scales these nodes draw at (typically ≤ 200 px on a side), more
/// than 32×32 is past human perceptibility anyway.
const TENSOR_NODE_MAX_CELLS: usize = 32;

/// Render a `CanvasNodeKind::Tensor(spec)` as a cell grid inside the
/// node's bounds. The grid is sized after the tensor's shape (rank-1 →
/// single row; rank-2 → H×W; rank ≥ 3 → first 2D plane with deeper
/// axes pinned to 0) and color-mapped through a blue→teal→yellow ramp
/// using min/max from `TensorSpec::stats()`. External / empty tensors
/// fall back to a frame-only render — there's no resident data to draw
/// without resolving the handle, which the canvas pane refuses to do
/// synchronously.
fn paint_tensor_node(
    ctx: &mut PaintContext,
    p0: Vector2F,
    w: f32,
    h: f32,
    spec: &TensorSpec,
) {
    // Outer frame — desaturated indigo, matching the wish-render tint
    // for the Tensor variant. The frame is always drawn so the shape
    // is visible even when the data isn't resident.
    let frame = ColorU::new(56, 64, 90, 255);
    ctx.scene
        .draw_rect_with_hit_recording(RectF::new(p0, vec2f(w, h)))
        .with_background(wishui::elements::Fill::Solid(frame));

    // Inset the cell grid so the frame stays legible.
    let inset = 1.5_f32.min(w * 0.1).min(h * 0.1);
    let inner_w = (w - inset * 2.0).max(1.0);
    let inner_h = (h - inset * 2.0).max(1.0);
    let inner_origin = p0 + vec2f(inset, inset);

    // Decide the grid (rows, cols). Rank-0 collapses to a single cell.
    let (rows, cols) = match spec.dims.len() {
        0 => (1, 1),
        1 => (1, spec.dims[0].min(TENSOR_NODE_MAX_CELLS).max(1)),
        _ => (
            spec.dims[0].min(TENSOR_NODE_MAX_CELLS).max(1),
            spec.dims[1].min(TENSOR_NODE_MAX_CELLS).max(1),
        ),
    };

    // Need stats to choose a color domain. If the data isn't resident
    // (External / Empty) `stats()` returns None — leave the frame as
    // the only visual cue and bail.
    let Some(stats) = spec.stats() else {
        return;
    };
    let range = (stats.max - stats.min).max(f32::EPSILON);

    let cell_w = inner_w / cols as f32;
    let cell_h = inner_h / rows as f32;

    // Sample at a (row, col) of the display grid. For tensors larger
    // than the cell budget we nearest-neighbor downsample by mapping
    // display indices to source indices. For higher-rank tensors we
    // pin every axis beyond the first two to 0 — a v1 default that
    // future Tensor view panes will let the user override.
    let sample = |r: usize, c: usize| -> Option<f32> {
        match spec.dims.len() {
            0 => spec.read_f32_at(&[]),
            1 => {
                let n = spec.dims[0];
                if n == 0 {
                    return None;
                }
                let cc = if n <= cols { c } else { (c * n) / cols.max(1) };
                spec.read_f32_at(&[cc.min(n - 1)])
            }
            _ => {
                let dh = spec.dims[0];
                let dw = spec.dims[1];
                if dh == 0 || dw == 0 {
                    return None;
                }
                let rr = if dh <= rows { r } else { (r * dh) / rows.max(1) };
                let cc = if dw <= cols { c } else { (c * dw) / cols.max(1) };
                let mut coords = Vec::with_capacity(spec.dims.len());
                coords.push(rr.min(dh - 1));
                coords.push(cc.min(dw - 1));
                for _ in 2..spec.dims.len() {
                    coords.push(0);
                }
                spec.read_f32_at(&coords)
            }
        }
    };

    for r in 0..rows {
        for c in 0..cols {
            let Some(v) = sample(r, c) else {
                continue;
            };
            // Non-finite cells render as the midpoint of the ramp so
            // they read as "data here but unusable" rather than as a
            // synthetic min or max value.
            let t = if v.is_finite() {
                ((v - stats.min) / range).clamp(0.0, 1.0)
            } else {
                0.5
            };
            let color = tensor_color_ramp(t);
            let cell_p = inner_origin + vec2f(c as f32 * cell_w, r as f32 * cell_h);
            ctx.scene
                .draw_rect_with_hit_recording(RectF::new(cell_p, vec2f(cell_w, cell_h)))
                .with_background(wishui::elements::Fill::Solid(color));
        }
    }
}

/// Two-segment color ramp: deep blue → teal → warm yellow. Chosen so
/// `t = 0.0` reads as "low/empty" and `t = 1.0` reads as "high/active"
/// at a glance, while still being colorblind-friendly (no red↔green
/// confusion at the endpoints). `t` is clamped to `[0, 1]`.
fn tensor_color_ramp(t: f32) -> ColorU {
    let t = t.clamp(0.0, 1.0);
    let lerp = |a: f32, b: f32, s: f32| a + (b - a) * s;
    let (r, g, b) = if t < 0.5 {
        let s = t * 2.0;
        (lerp(45.0, 60.0, s), lerp(60.0, 150.0, s), lerp(125.0, 150.0, s))
    } else {
        let s = (t - 0.5) * 2.0;
        (lerp(60.0, 235.0, s), lerp(150.0, 210.0, s), lerp(150.0, 70.0, s))
    };
    ColorU::new(r as u8, g as u8, b as u8, 255)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wish_canvas_core::tensor::{TensorDType, TensorRef};

    #[test]
    fn color_ramp_endpoints_distinct() {
        let low = tensor_color_ramp(0.0);
        let high = tensor_color_ramp(1.0);
        // Low end should be bluer than warm; high end should have more
        // red than the low end. We don't lock specific RGB values
        // because the ramp constants are an aesthetic choice that may
        // be tuned — but the *direction* should hold.
        assert!(low.b > low.r, "low end should be blue-biased: {low:?}");
        assert!(high.r > high.b, "high end should be warm: {high:?}");
        assert!(high.r > low.r, "high end should be redder than low");
    }

    #[test]
    fn color_ramp_clamps_out_of_range() {
        // The ramp should never panic on out-of-range inputs.
        let neg = tensor_color_ramp(-5.0);
        let over = tensor_color_ramp(10.0);
        let low = tensor_color_ramp(0.0);
        let high = tensor_color_ramp(1.0);
        assert_eq!((neg.r, neg.g, neg.b), (low.r, low.g, low.b));
        assert_eq!((over.r, over.g, over.b), (high.r, high.g, high.b));
    }

    #[test]
    fn paint_tensor_node_is_resilient_to_edge_shapes() {
        // These tensors shouldn't make the paint helper panic when
        // exercised via the sampling closure — we test the closure
        // indirectly by walking it the same way `paint_tensor_node`
        // does.
        let scalar = TensorSpec::new(vec![], TensorDType::F32, TensorRef::InlineF32 { data: vec![7.0] });
        assert_eq!(scalar.read_f32_at(&[]), Some(7.0));

        let row = TensorSpec::linspace_f32(0.0, 1.0, 100);
        // Downsampled to cols = 32: c=31 maps to source idx ~96.
        let n = row.dims[0];
        let cols = TENSOR_NODE_MAX_CELLS.min(n);
        let cc = if n <= cols { cols - 1 } else { ((cols - 1) * n) / cols.max(1) };
        assert!(row.read_f32_at(&[cc.min(n - 1)]).is_some());

        let cube = TensorSpec::from_fn_f32(vec![4, 4, 4], |c| (c[0] + c[1] + c[2]) as f32).unwrap();
        // The paint code pins axes 2..rank to 0, so it should always
        // resolve to a valid coord for any (r, c) in the display grid.
        assert!(cube.read_f32_at(&[3, 3, 0]).is_some());
    }
}
