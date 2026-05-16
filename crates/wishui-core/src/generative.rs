//! **Generative UI substrate** — the AI-emitted descriptor parser.
//!
//! This module is the bridge between:
//!   - AI agents that emit UI as JSON descriptors, and
//!   - WishUI's retained Scene graph.
//!
//! An agent emits something like:
//! ```json
//! {
//!   "primitives": [
//!     { "kind": "rect", "x": 10, "y": 10, "w": 100, "h": 50, "fill": "#4a9eff", "radius": 4 },
//!     { "kind": "arrow", "from": [10, 35], "to": [110, 35], "width": 2, "color": "#ffb86c", "head": 8 },
//!     { "kind": "circle", "center": [60, 35], "radius": 6, "fill": "#ffffff" },
//!     { "kind": "grid", "rect": [0, 0, 400, 200], "cell": 25, "width": 1, "color": "#1a2030" }
//!   ]
//! }
//! ```
//!
//! and Wish renders it natively. **No agent-side rendering code, no
//! per-agent UI knowledge** — the descriptor is the contract.
//!
//! # Wave 23g of the WishUI Generative-UI roadmap
//!
//! Strategic frame: `wish-design/.../01-strategy/10-wishui-generative-ui.md`.
//! This file implements the descriptor parser; the `GenerativeElement`
//! (a WishUI `Element` that holds + paints a descriptor) lives in the
//! app layer because it needs the `Element` trait, which is also in
//! wishui-core but conventionally consumed via `wishui`.

use crate::scene::Scene;
use pathfinder_color::ColorU;
use pathfinder_geometry::rect::RectF;
use pathfinder_geometry::vector::{vec2f, Vector2F};
use serde::{Deserialize, Serialize};

/// A composable UI description that an AI agent can emit. Renders to
/// a [`Scene`] via [`paint_descriptor`].
///
/// The descriptor is intentionally minimal — every primitive that
/// can be drawn must also be specifiable as a `UiPrimitive` variant.
/// Adding a new visual capability is a two-step change:
///   1. Add a `UiPrimitive` variant.
///   2. Handle it in [`paint_descriptor`].
/// All callers — Rust code, AI agents, MCP tools — pick it up.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UiDescriptor {
    /// Ordered list of primitives. Later primitives paint over
    /// earlier ones.
    pub primitives: Vec<UiPrimitive>,
}

impl UiDescriptor {
    pub fn empty() -> Self {
        Self { primitives: Vec::new() }
    }

    pub fn from_primitives(primitives: Vec<UiPrimitive>) -> Self {
        Self { primitives }
    }

    /// Parse a JSON string into a descriptor. Useful for AI agents
    /// that emit JSON output.
    pub fn from_json(s: &str) -> Result<Self, GenerativeError> {
        serde_json::from_str(s).map_err(GenerativeError::Json)
    }

    /// Serialize this descriptor as JSON (pretty-printed). Useful for
    /// recording a Scene to inspect or replay.
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// One drawable element. Discriminated union; serde tag is `kind`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiPrimitive {
    /// Filled rectangle. Optional rounded corners.
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        #[serde(default)]
        fill: UiColor,
        #[serde(default)]
        radius: f32,
    },
    /// Stroked rectangle outline (no fill).
    Outline {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        #[serde(default = "default_stroke_width")]
        width: f32,
        #[serde(default)]
        color: UiColor,
    },
    /// Line segment.
    Line {
        from: [f32; 2],
        to: [f32; 2],
        #[serde(default = "default_stroke_width")]
        width: f32,
        #[serde(default)]
        color: UiColor,
    },
    /// Multi-segment line.
    Polyline {
        points: Vec<[f32; 2]>,
        #[serde(default = "default_stroke_width")]
        width: f32,
        #[serde(default)]
        color: UiColor,
    },
    /// Filled circle.
    Circle {
        center: [f32; 2],
        radius: f32,
        #[serde(default)]
        fill: UiColor,
    },
    /// Directed arrow.
    Arrow {
        from: [f32; 2],
        to: [f32; 2],
        #[serde(default = "default_stroke_width")]
        width: f32,
        #[serde(default)]
        color: UiColor,
        #[serde(default = "default_arrow_head")]
        head: f32,
    },
    /// Background grid.
    Grid {
        /// `[x, y, w, h]`.
        rect: [f32; 4],
        cell: f32,
        #[serde(default = "default_grid_width")]
        width: f32,
        #[serde(default = "default_grid_color")]
        color: UiColor,
    },
    /// Quadratic Bézier curve. `from` → `to` with control point `cp`.
    BezierQuad {
        from: [f32; 2],
        cp: [f32; 2],
        to: [f32; 2],
        #[serde(default = "default_stroke_width")]
        width: f32,
        #[serde(default)]
        color: UiColor,
    },
    /// Cubic Bézier curve. `from` → `to` with control points `cp1`, `cp2`.
    BezierCubic {
        from: [f32; 2],
        cp1: [f32; 2],
        cp2: [f32; 2],
        to: [f32; 2],
        #[serde(default = "default_stroke_width")]
        width: f32,
        #[serde(default)]
        color: UiColor,
    },
    /// Nested group. Primitives inside are translated by `offset`,
    /// useful for composing reusable sub-diagrams.
    Group {
        #[serde(default)]
        offset: [f32; 2],
        primitives: Vec<UiPrimitive>,
    },
    /// **Overlay** — paint `primitives` with a uniform opacity (0.0
    /// hidden, 1.0 fully visible). Used for fade animations, ghost
    /// previews, "agent is editing this" highlights. The opacity is
    /// applied by scaling every color's alpha channel; no new shader
    /// required.
    Overlay {
        #[serde(default = "default_opacity")]
        opacity: f32,
        primitives: Vec<UiPrimitive>,
    },
}

fn default_opacity() -> f32 {
    1.0
}

fn default_stroke_width() -> f32 {
    1.5
}
fn default_arrow_head() -> f32 {
    8.0
}
fn default_grid_width() -> f32 {
    0.75
}
fn default_grid_color() -> UiColor {
    UiColor::Rgba {
        r: 30,
        g: 38,
        b: 50,
        a: 255,
    }
}

/// Color specification — either a CSS-style hex string (`"#rrggbb"` /
/// `"#rrggbbaa"`) or an explicit RGBA quad. The hex form is what AI
/// agents most naturally emit; the RGBA form is what tests use.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum UiColor {
    Hex(String),
    Rgba { r: u8, g: u8, b: u8, a: u8 },
}

impl Default for UiColor {
    /// Default = opaque white.
    fn default() -> Self {
        UiColor::Rgba {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        }
    }
}

impl UiColor {
    /// Resolve to a [`ColorU`]. Returns `None` if the hex string is
    /// malformed; callers can fall back to a default.
    pub fn to_color_u(&self) -> Option<ColorU> {
        match self {
            UiColor::Rgba { r, g, b, a } => Some(ColorU::new(*r, *g, *b, *a)),
            UiColor::Hex(s) => parse_hex_color(s),
        }
    }

    /// Resolve, falling back to opaque white if the hex is bad. Used
    /// by [`paint_descriptor`] so a single malformed color in a
    /// large descriptor doesn't abort the whole render.
    pub fn resolve_or_default(&self) -> ColorU {
        self.to_color_u()
            .unwrap_or_else(|| ColorU::new(255, 255, 255, 255))
    }
}

/// Parse `#rgb`, `#rrggbb`, or `#rrggbbaa` into a [`ColorU`]. Returns
/// `None` on malformed input.
fn parse_hex_color(s: &str) -> Option<ColorU> {
    let s = s.strip_prefix('#').unwrap_or(s);
    let to_byte = |a: char, b: char| -> Option<u8> {
        let hi = a.to_digit(16)? as u8;
        let lo = b.to_digit(16)? as u8;
        Some((hi << 4) | lo)
    };
    let chars: Vec<char> = s.chars().collect();
    match chars.len() {
        // #rgb → #rrggbb
        3 => {
            let r = to_byte(chars[0], chars[0])?;
            let g = to_byte(chars[1], chars[1])?;
            let b = to_byte(chars[2], chars[2])?;
            Some(ColorU::new(r, g, b, 255))
        }
        // #rrggbb
        6 => {
            let r = to_byte(chars[0], chars[1])?;
            let g = to_byte(chars[2], chars[3])?;
            let b = to_byte(chars[4], chars[5])?;
            Some(ColorU::new(r, g, b, 255))
        }
        // #rrggbbaa
        8 => {
            let r = to_byte(chars[0], chars[1])?;
            let g = to_byte(chars[2], chars[3])?;
            let b = to_byte(chars[4], chars[5])?;
            let a = to_byte(chars[6], chars[7])?;
            Some(ColorU::new(r, g, b, a))
        }
        _ => None,
    }
}

/// Errors from parsing or painting a [`UiDescriptor`].
#[derive(Debug, thiserror::Error)]
pub enum GenerativeError {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// Walk a [`UiDescriptor`] and emit drawing commands into the given
/// [`Scene`]. The descriptor is painted **at scene-local coordinates**
/// — the caller is responsible for `origin` offsetting via an outer
/// `Group { offset: [origin_x, origin_y], primitives: ... }` if
/// painting at a specific viewport location.
///
/// This is the function an AI agent transitively invokes when it
/// emits a UI descriptor: WishUI receives the JSON, parses it once,
/// and on every frame the descriptor is replayed into the Scene.
pub fn paint_descriptor(scene: &mut Scene, descriptor: &UiDescriptor) {
    for prim in &descriptor.primitives {
        paint_primitive(scene, prim, Vector2F::zero(), 1.0);
    }
}

/// Scale a color's alpha channel by `opacity` (clamped to [0, 1]). Used
/// by the `Overlay` primitive to apply a uniform fade.
fn apply_opacity(c: ColorU, opacity: f32) -> ColorU {
    let o = opacity.clamp(0.0, 1.0);
    ColorU::new(c.r, c.g, c.b, ((c.a as f32) * o).round() as u8)
}

fn paint_primitive(scene: &mut Scene, prim: &UiPrimitive, offset: Vector2F, opacity: f32) {
    // Resolve color once with both the primitive's color + the
    // current opacity scope. AI-emitted overlays compose by
    // multiplying opacities (group with opacity 0.5 inside overlay
    // with opacity 0.5 → final opacity 0.25). Honest scoping.
    let with_op = |c: &UiColor| apply_opacity(c.resolve_or_default(), opacity);
    match prim {
        UiPrimitive::Rect {
            x,
            y,
            w,
            h,
            fill,
            radius,
        } => {
            let rect = RectF::new(vec2f(*x + offset.x(), *y + offset.y()), vec2f(*w, *h));
            let r = scene
                .draw_rect_without_hit_recording(rect)
                .with_background(crate::elements::Fill::Solid(with_op(fill)));
            if *radius > 0.0 {
                r.with_corner_radius(crate::scene::CornerRadius::with_all(
                    crate::scene::Radius::Pixels(*radius),
                ));
            }
        }
        UiPrimitive::Outline {
            x,
            y,
            w,
            h,
            width,
            color,
        } => {
            scene.draw_rect_outline(
                RectF::new(vec2f(*x + offset.x(), *y + offset.y()), vec2f(*w, *h)),
                *width,
                with_op(color),
            );
        }
        UiPrimitive::Line {
            from,
            to,
            width,
            color,
        } => {
            scene.draw_line(
                vec2f(from[0] + offset.x(), from[1] + offset.y()),
                vec2f(to[0] + offset.x(), to[1] + offset.y()),
                *width,
                with_op(color),
            );
        }
        UiPrimitive::Polyline {
            points,
            width,
            color,
        } => {
            let translated: Vec<Vector2F> = points
                .iter()
                .map(|p| vec2f(p[0] + offset.x(), p[1] + offset.y()))
                .collect();
            scene.draw_polyline(&translated, *width, with_op(color));
        }
        UiPrimitive::Circle {
            center,
            radius,
            fill,
        } => {
            scene.draw_circle(
                vec2f(center[0] + offset.x(), center[1] + offset.y()),
                *radius,
                with_op(fill),
            );
        }
        UiPrimitive::Arrow {
            from,
            to,
            width,
            color,
            head,
        } => {
            scene.draw_arrow(
                vec2f(from[0] + offset.x(), from[1] + offset.y()),
                vec2f(to[0] + offset.x(), to[1] + offset.y()),
                *width,
                with_op(color),
                *head,
            );
        }
        UiPrimitive::Grid {
            rect,
            cell,
            width,
            color,
        } => {
            scene.draw_grid(
                RectF::new(
                    vec2f(rect[0] + offset.x(), rect[1] + offset.y()),
                    vec2f(rect[2], rect[3]),
                ),
                *cell,
                *width,
                with_op(color),
            );
        }
        UiPrimitive::BezierQuad {
            from,
            cp,
            to,
            width,
            color,
        } => {
            scene.draw_bezier_quad(
                vec2f(from[0] + offset.x(), from[1] + offset.y()),
                vec2f(cp[0] + offset.x(), cp[1] + offset.y()),
                vec2f(to[0] + offset.x(), to[1] + offset.y()),
                *width,
                with_op(color),
            );
        }
        UiPrimitive::BezierCubic {
            from,
            cp1,
            cp2,
            to,
            width,
            color,
        } => {
            scene.draw_bezier_cubic(
                vec2f(from[0] + offset.x(), from[1] + offset.y()),
                vec2f(cp1[0] + offset.x(), cp1[1] + offset.y()),
                vec2f(cp2[0] + offset.x(), cp2[1] + offset.y()),
                vec2f(to[0] + offset.x(), to[1] + offset.y()),
                *width,
                with_op(color),
            );
        }
        UiPrimitive::Group { offset: o, primitives } => {
            let new_offset = vec2f(offset.x() + o[0], offset.y() + o[1]);
            for child in primitives {
                paint_primitive(scene, child, new_offset, opacity);
            }
        }
        UiPrimitive::Overlay {
            opacity: o,
            primitives,
        } => {
            let nested = opacity * o;
            for child in primitives {
                paint_primitive(scene, child, offset, nested);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering;

    fn fresh_scene() -> Scene {
        Scene::new(1.0, rendering::Config::default())
    }

    #[test]
    fn parse_hex_3_digit() {
        assert_eq!(parse_hex_color("#f00"), Some(ColorU::new(255, 0, 0, 255)));
        assert_eq!(parse_hex_color("#abc"), Some(ColorU::new(170, 187, 204, 255)));
    }

    #[test]
    fn parse_hex_6_digit() {
        assert_eq!(
            parse_hex_color("#4a9eff"),
            Some(ColorU::new(0x4a, 0x9e, 0xff, 255))
        );
        assert_eq!(parse_hex_color("4a9eff"), Some(ColorU::new(0x4a, 0x9e, 0xff, 255)));
    }

    #[test]
    fn parse_hex_8_digit_alpha() {
        assert_eq!(
            parse_hex_color("#4a9eff80"),
            Some(ColorU::new(0x4a, 0x9e, 0xff, 0x80))
        );
    }

    #[test]
    fn parse_hex_rejects_malformed() {
        assert_eq!(parse_hex_color("#xyz"), None);
        assert_eq!(parse_hex_color("#12345"), None);
        assert_eq!(parse_hex_color(""), None);
    }

    #[test]
    fn descriptor_roundtrips_through_json() {
        let d = UiDescriptor::from_primitives(vec![
            UiPrimitive::Rect {
                x: 10.0,
                y: 10.0,
                w: 100.0,
                h: 50.0,
                fill: UiColor::Hex("#4a9eff".to_string()),
                radius: 4.0,
            },
            UiPrimitive::Arrow {
                from: [0.0, 0.0],
                to: [100.0, 0.0],
                width: 2.0,
                color: UiColor::default(),
                head: 8.0,
            },
        ]);
        let json = d.to_json_pretty();
        let back = UiDescriptor::from_json(&json).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn json_with_unknown_fields_uses_defaults() {
        // AI agents may emit minimal descriptors — verify the defaults
        // kick in correctly.
        let json = r#"{
            "primitives": [
                { "kind": "rect", "x": 0, "y": 0, "w": 10, "h": 10 },
                { "kind": "line", "from": [0,0], "to": [10,10] },
                { "kind": "circle", "center": [5,5], "radius": 3 }
            ]
        }"#;
        let d = UiDescriptor::from_json(json).unwrap();
        assert_eq!(d.primitives.len(), 3);
        // First rect's fill should default to opaque white.
        if let UiPrimitive::Rect { fill, radius, .. } = &d.primitives[0] {
            assert_eq!(fill.resolve_or_default(), ColorU::new(255, 255, 255, 255));
            assert_eq!(*radius, 0.0);
        } else {
            panic!("expected rect");
        }
    }

    #[test]
    fn paint_descriptor_emits_rects_for_primitives() {
        let d = UiDescriptor::from_primitives(vec![
            UiPrimitive::Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
                fill: UiColor::default(),
                radius: 0.0,
            },
            UiPrimitive::Circle {
                center: [5.0, 5.0],
                radius: 4.0,
                fill: UiColor::default(),
            },
            UiPrimitive::Line {
                from: [0.0, 0.0],
                to: [50.0, 0.0],
                width: 1.0,
                color: UiColor::default(),
            },
        ]);
        let mut scene = fresh_scene();
        paint_descriptor(&mut scene, &d);
        let n = scene.layers().next().unwrap().rects.len();
        // 1 rect + 1 rounded-rect-as-circle + many stippled line rects.
        assert!(n > 50, "expected many rects from descriptor, got {n}");
    }

    #[test]
    fn group_offsets_translate_children() {
        // Two identical rects, one in a group with offset.
        let inner = UiPrimitive::Rect {
            x: 0.0,
            y: 0.0,
            w: 5.0,
            h: 5.0,
            fill: UiColor::default(),
            radius: 0.0,
        };
        let d = UiDescriptor::from_primitives(vec![
            inner.clone(),
            UiPrimitive::Group {
                offset: [100.0, 50.0],
                primitives: vec![inner.clone()],
            },
        ]);
        let mut scene = fresh_scene();
        paint_descriptor(&mut scene, &d);
        let rects = &scene.layers().next().unwrap().rects;
        assert_eq!(rects.len(), 2);
        // First rect at origin, second translated.
        assert_eq!(rects[0].bounds.origin(), vec2f(0.0, 0.0));
        assert_eq!(rects[1].bounds.origin(), vec2f(100.0, 50.0));
    }

    #[test]
    fn nested_groups_compose_offsets() {
        let leaf = UiPrimitive::Rect {
            x: 1.0,
            y: 1.0,
            w: 1.0,
            h: 1.0,
            fill: UiColor::default(),
            radius: 0.0,
        };
        let d = UiDescriptor::from_primitives(vec![UiPrimitive::Group {
            offset: [10.0, 0.0],
            primitives: vec![UiPrimitive::Group {
                offset: [5.0, 20.0],
                primitives: vec![leaf],
            }],
        }]);
        let mut scene = fresh_scene();
        paint_descriptor(&mut scene, &d);
        let r = &scene.layers().next().unwrap().rects[0];
        // 1 + 10 + 5 = 16 on x; 1 + 0 + 20 = 21 on y.
        assert_eq!(r.bounds.origin(), vec2f(16.0, 21.0));
    }

    #[test]
    fn empty_descriptor_paints_nothing() {
        let mut scene = fresh_scene();
        paint_descriptor(&mut scene, &UiDescriptor::empty());
        assert_eq!(scene.layers().next().unwrap().rects.len(), 0);
    }

    #[test]
    fn apply_opacity_scales_alpha() {
        let c = ColorU::new(100, 100, 100, 200);
        assert_eq!(apply_opacity(c, 1.0).a, 200);
        assert_eq!(apply_opacity(c, 0.5).a, 100);
        assert_eq!(apply_opacity(c, 0.0).a, 0);
        // Clamp out-of-range.
        assert_eq!(apply_opacity(c, 2.0).a, 200);
        assert_eq!(apply_opacity(c, -1.0).a, 0);
    }

    #[test]
    fn overlay_fades_descendants() {
        let opaque = UiPrimitive::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
            fill: UiColor::Rgba {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
            radius: 0.0,
        };
        // Wrap in Overlay with opacity 0.5 → alpha should drop to ~128.
        let d = UiDescriptor::from_primitives(vec![UiPrimitive::Overlay {
            opacity: 0.5,
            primitives: vec![opaque],
        }]);
        let mut scene = fresh_scene();
        paint_descriptor(&mut scene, &d);
        let r = &scene.layers().next().unwrap().rects[0];
        // Read back the alpha from the Fill — fade applied.
        if let crate::elements::Fill::Solid(c) = &r.background {
            assert!(c.a > 120 && c.a < 135, "overlay should fade alpha to ~128, got {}", c.a);
        } else {
            panic!("expected solid fill");
        }
    }

    #[test]
    fn nested_overlays_compose_multiplicatively() {
        // Outer 0.5 × inner 0.5 = 0.25 → alpha 255 * 0.25 ≈ 64.
        let inner = UiPrimitive::Rect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            fill: UiColor::Rgba {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
            radius: 0.0,
        };
        let d = UiDescriptor::from_primitives(vec![UiPrimitive::Overlay {
            opacity: 0.5,
            primitives: vec![UiPrimitive::Overlay {
                opacity: 0.5,
                primitives: vec![inner],
            }],
        }]);
        let mut scene = fresh_scene();
        paint_descriptor(&mut scene, &d);
        let r = &scene.layers().next().unwrap().rects[0];
        if let crate::elements::Fill::Solid(c) = &r.background {
            assert!(c.a > 55 && c.a < 75, "nested overlay should multiply: 0.25 * 255 ≈ 64, got {}", c.a);
        }
    }

    #[test]
    fn bezier_quad_paints_curve_via_polyline() {
        let mut scene = fresh_scene();
        scene.draw_bezier_quad(
            vec2f(0., 0.),
            vec2f(50., 100.),
            vec2f(100., 0.),
            2.0,
            ColorU::new(255, 255, 255, 255),
        );
        // 24 segments * stippled rects per segment = lots of rects.
        let n = scene.layers().next().unwrap().rects.len();
        assert!(n > 100, "bezier should emit many rects, got {n}");
    }

    #[test]
    fn bezier_cubic_descriptor_renders() {
        let d = UiDescriptor::from_primitives(vec![UiPrimitive::BezierCubic {
            from: [0.0, 0.0],
            cp1: [25.0, 50.0],
            cp2: [75.0, -50.0],
            to: [100.0, 0.0],
            width: 2.0,
            color: UiColor::default(),
        }]);
        let mut scene = fresh_scene();
        paint_descriptor(&mut scene, &d);
        let n = scene.layers().next().unwrap().rects.len();
        assert!(n > 100, "cubic bezier descriptor should render, got {n}");
    }

    #[test]
    fn ai_emitted_descriptor_example_renders() {
        // The canonical example from the module-level doc — proves
        // the docs match reality. Uses `r##"…"##` because the JSON
        // contains `#hex` color strings.
        let json = r##"{
          "primitives": [
            { "kind": "rect", "x": 10, "y": 10, "w": 100, "h": 50, "fill": "#4a9eff", "radius": 4 },
            { "kind": "arrow", "from": [10, 35], "to": [110, 35], "width": 2, "color": "#ffb86c", "head": 8 },
            { "kind": "circle", "center": [60, 35], "radius": 6, "fill": "#ffffff" },
            { "kind": "grid", "rect": [0, 0, 400, 200], "cell": 25, "width": 1, "color": "#1a2030" }
          ]
        }"##;
        let d = UiDescriptor::from_json(json).expect("descriptor parses");
        let mut scene = fresh_scene();
        paint_descriptor(&mut scene, &d);
        // Should emit substantial draw commands (grid alone is dozens
        // of stippled lines).
        let n = scene.layers().next().unwrap().rects.len();
        assert!(n > 200, "doc-example descriptor should emit >200 rects, got {n}");
    }
}
