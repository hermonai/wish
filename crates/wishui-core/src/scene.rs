use crate::elements::Fill;
use crate::geometry::vector::vec2f;
use crate::image_cache::StaticImage;
use crate::{
    elements::Point,
    fonts::{FontId, GlyphId},
    rendering,
};
use ordered_float::OrderedFloat;
use pathfinder_color::ColorU;
use pathfinder_geometry::rect::RectF;
use pathfinder_geometry::vector::Vector2F;
use rstar::{primitives::Rectangle, RTree};
use std::sync::Arc;
use vec1::{vec1, Vec1};

#[derive(Clone)]
pub struct Scene {
    scale_factor: f32,
    rendering_config: rendering::Config,
    active_layer_index_stack: Vec1<ZIndex>,
    layers: Vec1<Layer>,
    overlay_layers: Vec<Layer>,
    #[cfg(debug_assertions)]
    /// Custom panic location, set with [`Scene::set_location_for_panic_logging`]
    panic_location: Option<&'static std::panic::Location<'static>>,
}

#[derive(Clone, Default)]
pub struct Layer {
    hit_map: RTree<Rectangle<[OrderedFloat<f32>; 2]>>,
    pub clip_bounds: Option<RectF>,
    pub rects: Vec<Rect>,
    pub images: Vec<Image>,
    pub glyphs: Vec<Glyph>,
    pub icons: Vec<Icon>,
    pub click_through: bool,
}

/// Clip bounds to use for a layer.
pub enum ClipBounds {
    /// Use the bounds of the active layer.
    ActiveLayer,
    /// Use the specified bounds as the bounds for the new layer.
    ///
    /// Note that this ignores any clip bounds applied to the currently-active
    /// layer.
    BoundedBy(RectF),
    /// Intersect the active layer's bounds and the provided rect
    /// to get the bounds for the new layer.
    BoundedByActiveLayerAnd(RectF),
    /// No clipping
    None,
}

impl Layer {
    fn record_hit_rect(&mut self, rect: RectF) {
        if let Some(intersected) = self
            .clip_bounds
            .map_or(Some(rect), |c| rect.intersection(c))
        {
            self.hit_map.insert(Rectangle::from_corners(
                [intersected.min_x().into(), intersected.min_y().into()],
                [intersected.max_x().into(), intersected.max_y().into()],
            ));
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct GlyphKey {
    pub glyph_id: GlyphId,
    pub font_id: FontId,
    pub font_size: OrderedFloat<f32>,
}

#[derive(Debug, Copy, Clone)]
pub enum GlyphFade {
    /// A horizontal fade from alpha 1 to 0 with start and end positions in screen coordinates
    /// start - where the fade is transparent
    /// end - where the fade is most opaque
    Horizontal { start: f32, end: f32 },
}

impl GlyphFade {
    pub fn horizontal(start: f32, end: f32) -> Self {
        GlyphFade::Horizontal { start, end }
    }
}

#[derive(Clone, Debug)]
pub struct Glyph {
    pub glyph_key: GlyphKey,
    pub position: Vector2F,
    pub fade: Option<GlyphFade>,
    pub color: ColorU,
}

#[derive(Clone, Default)]
pub struct Rect {
    pub bounds: RectF,
    pub drop_shadow: Option<DropShadow>,
    pub corner_radius: CornerRadius,
    pub background: Fill,
    pub border: Border,
}

#[derive(Clone)]
pub struct Image {
    pub bounds: RectF,
    pub asset: Arc<StaticImage>,
    pub opacity: f32,
    pub corner_radius: CornerRadius,
}

#[derive(Clone)]
pub struct Icon {
    pub bounds: RectF,
    pub asset: Arc<StaticImage>,
    pub opacity: f32,
    pub color: ColorU,
}

// These were picked empirically to make the shadows look decent by
// default, but there is nothing special about them.
const DEFAULT_DROP_SHADOW_OFFSET_X: f32 = 0.;
const DEFAULT_DROP_SHADOW_OFFSET_Y: f32 = 10.;
const DEFAULT_DROP_SHADOW_BLUR_RADIUS: f32 = 10.;
const DEFAULT_DROP_SHADOW_SPREAD_RADIUS: f32 = 30.;

#[derive(Clone, Copy)]
pub struct DropShadow {
    pub color: ColorU,

    // How the shadow is offset from the target rect
    pub offset: Vector2F,

    // Controls how tightly sampled the shadow is - the larger the number
    // the more spread out the shadow.
    pub blur_radius: f32,

    // Controls how wide the shadow is outside the target.
    pub spread_radius: f32,
}

impl DropShadow {
    pub fn new_with_standard_offset_and_spread(color: ColorU) -> Self {
        Self {
            color,
            offset: vec2f(DEFAULT_DROP_SHADOW_OFFSET_X, DEFAULT_DROP_SHADOW_OFFSET_Y),
            blur_radius: DEFAULT_DROP_SHADOW_BLUR_RADIUS,
            spread_radius: DEFAULT_DROP_SHADOW_SPREAD_RADIUS,
        }
    }

    pub fn with_offset(mut self, offset: Vector2F) -> Self {
        self.offset = offset;
        self
    }
}

impl Default for DropShadow {
    fn default() -> Self {
        Self::new_with_standard_offset_and_spread(ColorU::new(0, 0, 0, 32))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Border {
    pub width: f32,
    pub color: Fill,
    pub top: bool,
    pub left: bool,
    pub bottom: bool,
    pub right: bool,
    pub dash: Option<Dash>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Dash {
    pub dash_length: f32,
    pub gap_length: f32,

    /// If true, gaps will always be the length specified in `gap_length`.
    /// Otherwise, gap length may be adjusted slightly to guarantee that the
    /// dashed line starts and ends with a dash.
    pub force_consistent_gap_length: bool,
}

impl Border {
    pub fn top_width(&self) -> f32 {
        if self.top {
            self.width
        } else {
            0.0
        }
    }

    pub fn right_width(&self) -> f32 {
        if self.right {
            self.width
        } else {
            0.0
        }
    }

    pub fn bottom_width(&self) -> f32 {
        if self.bottom {
            self.width
        } else {
            0.0
        }
    }

    pub fn left_width(&self) -> f32 {
        if self.left {
            self.width
        } else {
            0.0
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Radius {
    /// Specify a radius in absolute pixels.
    Pixels(f32),
    /// Specify a radius as a percentage of the rectangle's smaller dimension.
    /// For example, using `Percentage(50.)` will produce a pill shape.
    Percentage(f32),
}

impl Default for Radius {
    fn default() -> Self {
        Radius::Pixels(0.)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CornerRadius {
    /// Top left corner radius
    top_left: Option<Radius>,
    /// Top right corner radius
    top_right: Option<Radius>,
    /// Bottom left corner radius
    bottom_left: Option<Radius>,
    /// Bottom right corner radius
    bottom_right: Option<Radius>,
}

impl CornerRadius {
    /// Merge this CornerRadius struct with another.
    /// `Some(r)` takes precedence over `None`.
    /// If both are present, `other`'s values, take precedence over `self`'s existing values.
    pub fn merge(&mut self, other: CornerRadius) {
        self.top_left = other.top_left.or(self.top_left);
        self.top_right = other.top_right.or(self.top_right);
        self.bottom_left = other.bottom_left.or(self.bottom_left);
        self.bottom_right = other.bottom_right.or(self.bottom_right);
    }

    pub fn get_top_left(&self) -> Radius {
        self.top_left.unwrap_or(Radius::Pixels(0.))
    }

    pub fn get_top_right(&self) -> Radius {
        self.top_right.unwrap_or(Radius::Pixels(0.))
    }

    pub fn get_bottom_left(&self) -> Radius {
        self.bottom_left.unwrap_or(Radius::Pixels(0.))
    }

    pub fn get_bottom_right(&self) -> Radius {
        self.bottom_right.unwrap_or(Radius::Pixels(0.))
    }

    pub const fn with_all(radius: Radius) -> Self {
        CornerRadius {
            top_left: Some(radius),
            top_right: Some(radius),
            bottom_left: Some(radius),
            bottom_right: Some(radius),
        }
    }
    pub const fn with_top(radius: Radius) -> Self {
        CornerRadius {
            top_left: Some(radius),
            top_right: Some(radius),
            bottom_left: None,
            bottom_right: None,
        }
    }
    pub const fn with_bottom(radius: Radius) -> Self {
        CornerRadius {
            top_left: None,
            top_right: None,
            bottom_left: Some(radius),
            bottom_right: Some(radius),
        }
    }
    pub const fn with_left(radius: Radius) -> Self {
        CornerRadius {
            top_left: Some(radius),
            top_right: None,
            bottom_left: Some(radius),
            bottom_right: None,
        }
    }
    pub const fn with_right(radius: Radius) -> Self {
        CornerRadius {
            top_left: None,
            top_right: Some(radius),
            bottom_left: None,
            bottom_right: Some(radius),
        }
    }
    pub const fn with_top_left(radius: Radius) -> Self {
        CornerRadius {
            top_left: Some(radius),
            top_right: None,
            bottom_left: None,
            bottom_right: None,
        }
    }
    pub const fn with_top_right(radius: Radius) -> Self {
        CornerRadius {
            top_left: None,
            top_right: Some(radius),
            bottom_left: None,
            bottom_right: None,
        }
    }
    pub const fn with_bottom_left(radius: Radius) -> Self {
        CornerRadius {
            top_left: None,
            top_right: None,
            bottom_left: Some(radius),
            bottom_right: None,
        }
    }
    pub const fn with_bottom_right(radius: Radius) -> Self {
        CornerRadius {
            top_left: None,
            top_right: None,
            bottom_left: None,
            bottom_right: Some(radius),
        }
    }

    /// Filters this [`CornerRadius`] to only have the top corners rounded.
    pub const fn top(self) -> Self {
        CornerRadius {
            top_left: self.top_left,
            top_right: self.top_right,
            bottom_left: None,
            bottom_right: None,
        }
    }

    /// Filters this [`CornerRadius`] to only have the bottom corners rounded.
    pub const fn bottom(self) -> Self {
        CornerRadius {
            top_left: None,
            top_right: None,
            bottom_left: self.bottom_left,
            bottom_right: self.bottom_right,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// Newtype to encapsulate a Z index, which actually represents a layer index in the list of layers
pub enum ZIndex {
    Normal(usize),
    Overlay(usize),
}

impl ZIndex {
    #[cfg(test)]
    pub fn new(layer: usize) -> Self {
        ZIndex::Normal(layer)
    }
}

impl Scene {
    pub fn new(scale_factor: f32, rendering_config: rendering::Config) -> Self {
        Self {
            scale_factor,
            rendering_config,
            active_layer_index_stack: vec1![ZIndex::Normal(0)],
            layers: vec1![Layer::default()],
            overlay_layers: Vec::new(),
            #[cfg(debug_assertions)]
            panic_location: None,
        }
    }

    /// Temporarily set the panic location for the scene. This is cleared
    /// during the next draw call.
    #[cfg(debug_assertions)]
    pub fn set_location_for_panic_logging(
        &mut self,
        panic_location: Option<&'static std::panic::Location<'static>>,
    ) {
        self.panic_location = panic_location;
    }

    fn active_layer(&mut self) -> &mut Layer {
        match *self.active_layer_index_stack.last() {
            ZIndex::Normal(index) => &mut self.layers[index],
            ZIndex::Overlay(index) => &mut self.overlay_layers[index],
        }
    }

    pub fn is_covered(&self, position: Point) -> bool {
        // Does any layer at a higher z-index contain this point?
        let point = [position.x().into(), position.y().into()];
        let predicate = |l: &Layer| !l.click_through && l.hit_map.locate_at_point(&point).is_some();

        match position.z_index() {
            ZIndex::Normal(index) => self
                .layers
                .get((index + 1)..)
                .into_iter()
                .flatten()
                .chain(self.overlay_layers.iter())
                .any(predicate),
            ZIndex::Overlay(index) => self
                .overlay_layers
                .get((index + 1)..)
                .into_iter()
                .flatten()
                .any(predicate),
        }
    }

    // Compute the intersection between the bound of the element and the clip bound
    // on its current layer. The intersection is then checked against the event position
    // to determine whether we should dispatch the event.
    pub fn visible_rect(&self, origin: Point, size: Vector2F) -> Option<RectF> {
        // TODO: Investigate how / when we would pass a z-index that isn't in the scene
        // This appears to be fairly common, based on adding sentry reporting to it, however it
        // doesn't seem to dramatically impact app usage. Perhaps it's something that happens on
        // a view teardown frame?
        let maybe_layer = match origin.z_index() {
            ZIndex::Normal(index) => self.layers.get(index),
            ZIndex::Overlay(index) => self.overlay_layers.get(index),
        };
        let maybe_bounds = maybe_layer.and_then(|layer| layer.clip_bounds);

        let input_rect = RectF::new(origin.xy(), size);
        match maybe_bounds {
            Some(clip_rect) => clip_rect.intersection(input_rect),
            None => Some(input_rect),
        }
    }

    /// Get the Z-Index of the currently-active layer
    pub fn z_index(&self) -> ZIndex {
        *self.active_layer_index_stack.last()
    }

    /// Get the maximum Z-Index in the active layer stack (whether Normal or Overlay).
    pub fn max_active_z_index(&self) -> ZIndex {
        match self.active_layer_index_stack.last() {
            ZIndex::Normal(_) => ZIndex::Normal(self.layers.len() - 1),
            // Safety: If the active layer is an overlay layer, then there must be at least one
            // overlay layer, so subtracting one from the length is valid.
            ZIndex::Overlay(_) => ZIndex::Overlay(self.overlay_layers.len() - 1),
        }
    }

    pub fn start_layer(&mut self, bounds: ClipBounds) {
        let layer = self.create_layer(bounds);

        match *self.active_layer_index_stack.last() {
            ZIndex::Normal(_) => self.push_normal_layer(layer),
            ZIndex::Overlay(_) => self.push_overlay_layer(layer),
        }
    }

    pub(crate) fn start_overlay_layer(&mut self, bounds: ClipBounds) {
        let layer = self.create_layer(bounds);
        self.push_overlay_layer(layer);
    }

    fn create_layer(&mut self, bounds: ClipBounds) -> Layer {
        let clip_bounds = match bounds {
            ClipBounds::ActiveLayer => self.active_layer().clip_bounds,
            ClipBounds::BoundedBy(bounds) => Some(bounds),
            ClipBounds::BoundedByActiveLayerAnd(bounds) => {
                if let Some(current_layer_bounds) = self.active_layer().clip_bounds {
                    // If the current layer has bounds, return the intersection...
                    current_layer_bounds
                        .intersection(bounds)
                        // ...or, if the regions don't overlap, an empty bounding rect.
                        .or(Some(RectF::default()))
                } else {
                    // If the current layer has no bounds, return the bounds
                    // for the new layer.
                    Some(bounds)
                }
            }
            ClipBounds::None => None,
        };

        Layer {
            clip_bounds,
            ..Default::default()
        }
    }

    fn push_normal_layer(&mut self, layer: Layer) {
        self.active_layer_index_stack
            .push(ZIndex::Normal(self.layers.len()));
        self.layers.push(layer);
    }

    fn push_overlay_layer(&mut self, layer: Layer) {
        self.active_layer_index_stack
            .push(ZIndex::Overlay(self.overlay_layers.len()));
        self.overlay_layers.push(layer);
    }

    pub fn set_active_layer_click_through(&mut self) {
        self.active_layer().click_through = true;
    }

    pub fn stop_layer(&mut self) {
        if self.active_layer_index_stack.pop().is_err() {
            panic!("popped the last layer from active_layer_index_stack");
        }
    }

    fn validate_rect(rect: &RectF, location: Option<&'static std::panic::Location<'static>>) {
        #[cfg(debug_assertions)]
        let location_info = location
            .map(|loc| {
                format!(
                    " (element created at {}:{}:{})",
                    loc.file(),
                    loc.line(),
                    loc.column()
                )
            })
            .unwrap_or_default();
        #[cfg(not(debug_assertions))]
        let location_info = "";
        debug_assert!(
            !rect.origin().y().is_infinite(),
            "!rect.origin().y().is_infinite(){location_info}"
        );
        debug_assert!(
            !rect.origin().y().is_nan(),
            "!rect.origin().y().is_nan(){location_info}"
        );

        debug_assert!(
            !rect.size().x().is_infinite(),
            "!rect.size().x().is_infinite(){location_info}"
        );
        debug_assert!(
            !rect.size().x().is_nan(),
            "!rect.size().x().is_nan(){location_info}"
        );
        debug_assert!(
            !rect.size().y().is_infinite(),
            "!rect.size().y().is_infinite(){location_info}"
        );
        debug_assert!(
            !rect.size().y().is_nan(),
            "!rect.size().y().is_nan(){location_info}"
        );
    }

    /// This method draws a rectangle without recording any information about it in the current
    /// layer. Note this should be used with caution. In most cases, what you need is
    /// `draw_rect_with_hit_recording` instead. However, in rare cases this may be useful for
    /// performance reasons when many intermediate rects are drawn. If this is called, it is up to
    /// the caller to also draw a rect (via draw_rect_with_hit_recording) that encompasses the range
    /// of the rects drawn so that layer recording for event dispatching is correctly kept
    /// up-to-date.
    pub fn draw_rect_without_hit_recording(&mut self, rect: RectF) -> &mut Rect {
        #[cfg(debug_assertions)]
        let location = self.panic_location.take();
        #[cfg(not(debug_assertions))]
        let location = None;
        let layer = self.active_layer();
        Self::validate_rect(&rect, location);

        layer.rects.push(Rect {
            bounds: rect,
            ..Default::default()
        });
        layer.rects.last_mut().unwrap()
    }

    pub fn draw_rect_with_hit_recording(&mut self, rect: RectF) -> &mut Rect {
        let layer = self.active_layer();
        layer.record_hit_rect(rect);
        self.draw_rect_without_hit_recording(rect)
    }

    pub fn draw_image(
        &mut self,
        rect: RectF,
        asset: Arc<StaticImage>,
        opacity: f32,
        corner_radius: CornerRadius,
    ) {
        #[cfg(debug_assertions)]
        let location = self.panic_location.take();
        #[cfg(not(debug_assertions))]
        let location = None;
        let layer = self.active_layer();
        Self::validate_rect(&rect, location);

        layer.images.push(Image {
            bounds: rect,
            asset,
            opacity,
            corner_radius,
        });
        layer.record_hit_rect(rect);
    }

    pub fn draw_icon(&mut self, rect: RectF, asset: Arc<StaticImage>, opacity: f32, color: ColorU) {
        #[cfg(debug_assertions)]
        let location = self.panic_location.take();
        #[cfg(not(debug_assertions))]
        let location = None;
        let layer = self.active_layer();
        Self::validate_rect(&rect, location);

        layer.icons.push(Icon {
            bounds: rect,
            asset,
            opacity,
            color,
        });
        layer.record_hit_rect(rect);
    }

    /// Adds a glyph that should be drawn in the scene.
    ///
    /// `position` is the point at which the glyph's left edge meets the
    /// baseline.
    pub fn draw_glyph(
        &mut self,
        position: Vector2F,
        glyph_id: GlyphId,
        font_id: FontId,
        font_size: f32,
        color: ColorU,
    ) -> &mut Glyph {
        // TODO: Support hit testing on glyphs?
        let layer = self.active_layer();
        layer.glyphs.push(Glyph {
            glyph_key: GlyphKey {
                glyph_id,
                font_id,
                font_size: font_size.into(),
            },
            position,
            color,
            fade: None,
        });
        layer.glyphs.last_mut().unwrap()
    }

    /// Draw a line segment between two points with the given stroke
    /// `width` (in scene units) and `color`. This is a v0.5.0
    /// **generative-UI primitive** — it lets any Element produce
    /// arbitrary 2D graph / chart / agent-trace drawings on top of
    /// WishUI's retained scene graph without each caller reinventing
    /// rasterization.
    ///
    /// Implementation: stippled axis-aligned squares along the line
    /// path. Visually approximates a line for v0.5.0; ships
    /// end-to-end today without a new wgpu pipeline. A future wave
    /// (23b in the roadmap) will swap this for a dedicated line
    /// shader without changing the API.
    ///
    /// Cost is `O(line_length / width)` rectangles per call — fine
    /// for canvases up to a few thousand edges; consider batching for
    /// denser graphs.
    pub fn draw_line(
        &mut self,
        from: Vector2F,
        to: Vector2F,
        width: f32,
        color: ColorU,
    ) {
        let dx = to.x() - from.x();
        let dy = to.y() - from.y();
        let length = (dx * dx + dy * dy).sqrt();
        if length < 0.5 {
            return; // degenerate
        }
        // Step size = ~0.6 of the width so squares overlap and the
        // line looks reasonably solid.
        let step = (width * 0.6).max(0.5);
        let n = (length / step).ceil() as usize;
        let n = n.max(1);
        let nf = n as f32;
        let half = width * 0.5;
        for i in 0..=n {
            let t = i as f32 / nf;
            let x = from.x() + dx * t - half;
            let y = from.y() + dy * t - half;
            // Use the no-hit variant — line segments shouldn't
            // participate in event dispatch as separate rects.
            self.draw_rect_without_hit_recording(RectF::new(
                vec2f(x, y),
                vec2f(width, width),
            ))
            .with_background(Fill::Solid(color));
        }
    }

    /// Draw a polyline through a sequence of points, with consistent
    /// stroke `width` and `color`. Convenience wrapper around
    /// [`draw_line`].
    pub fn draw_polyline(
        &mut self,
        points: &[Vector2F],
        width: f32,
        color: ColorU,
    ) {
        for window in points.windows(2) {
            self.draw_line(window[0], window[1], width, color);
        }
    }

    /// Draw a directed line with an arrowhead at the `to` end. The
    /// arrowhead is constructed from two short line segments emanating
    /// back from the tip at ±30°. Useful for dependency-graph edges,
    /// call arrows, agent trace direction.
    ///
    /// `head_size` is the length of each arrowhead segment in scene
    /// units. Pass `0.0` to suppress the arrowhead (equivalent to
    /// [`draw_line`]).
    pub fn draw_arrow(
        &mut self,
        from: Vector2F,
        to: Vector2F,
        width: f32,
        color: ColorU,
        head_size: f32,
    ) {
        self.draw_line(from, to, width, color);
        if head_size <= 0.5 {
            return;
        }
        let dx = to.x() - from.x();
        let dy = to.y() - from.y();
        let length = (dx * dx + dy * dy).sqrt();
        if length < head_size * 1.5 {
            return; // arrow too short to fit a head — skip
        }
        // Unit vector pointing FROM tip back toward base.
        let ux = -dx / length;
        let uy = -dy / length;
        // Rotate ±30° to form the two head segments.
        // cos(30°)=0.866, sin(30°)=0.5
        let cos = 0.866_f32;
        let sin = 0.5_f32;
        let left_x = to.x() + (ux * cos - uy * sin) * head_size;
        let left_y = to.y() + (ux * sin + uy * cos) * head_size;
        let right_x = to.x() + (ux * cos + uy * sin) * head_size;
        let right_y = to.y() + (-ux * sin + uy * cos) * head_size;
        self.draw_line(to, vec2f(left_x, left_y), width, color);
        self.draw_line(to, vec2f(right_x, right_y), width, color);
    }

    /// Draw a circle (or filled disc) centered at `center` with radius
    /// `r`. Implementation: a square rect with corner-radius = r,
    /// which the rect shader rounds into a circle. This is the
    /// **least-cost circle primitive** in WishUI today — no new
    /// pipeline required.
    pub fn draw_circle(
        &mut self,
        center: Vector2F,
        radius: f32,
        color: ColorU,
    ) {
        if radius < 0.25 {
            return;
        }
        let rect = RectF::new(
            vec2f(center.x() - radius, center.y() - radius),
            vec2f(radius * 2.0, radius * 2.0),
        );
        self.draw_rect_without_hit_recording(rect)
            .with_background(Fill::Solid(color))
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(radius)));
    }

    /// Draw a rectangle outline (no fill) with the given stroke
    /// `width`. Useful for selection rings, region highlights,
    /// chart-axis frames. Built from four `draw_line` calls so it
    /// shares the line primitive's improvements automatically.
    pub fn draw_rect_outline(
        &mut self,
        rect: RectF,
        width: f32,
        color: ColorU,
    ) {
        let tl = rect.origin();
        let tr = vec2f(rect.max_x(), rect.min_y());
        let br = vec2f(rect.max_x(), rect.max_y());
        let bl = vec2f(rect.min_x(), rect.max_y());
        self.draw_line(tl, tr, width, color);
        self.draw_line(tr, br, width, color);
        self.draw_line(br, bl, width, color);
        self.draw_line(bl, tl, width, color);
    }

    /// Draw a grid of lines inside `rect` with `cell_size`-spaced
    /// gridlines. The `cell_size` is in scene units; passing 0.0
    /// no-ops. Useful for chart backgrounds, tensor-plot axes,
    /// design-tool snap visualizations.
    ///
    /// Every grid line is drawn via [`draw_line`], so the same
    /// rendering improvements apply.
    pub fn draw_grid(
        &mut self,
        rect: RectF,
        cell_size: f32,
        width: f32,
        color: ColorU,
    ) {
        if cell_size <= 0.5 || rect.width() <= 1.0 || rect.height() <= 1.0 {
            return;
        }
        let mut x = rect.min_x();
        while x <= rect.max_x() + 0.5 {
            self.draw_line(
                vec2f(x, rect.min_y()),
                vec2f(x, rect.max_y()),
                width,
                color,
            );
            x += cell_size;
        }
        let mut y = rect.min_y();
        while y <= rect.max_y() + 0.5 {
            self.draw_line(
                vec2f(rect.min_x(), y),
                vec2f(rect.max_x(), y),
                width,
                color,
            );
            y += cell_size;
        }
    }

    /// Draw a smooth quadratic Bézier curve from `p0` to `p2` with
    /// `p1` as the control point. Approximated by [`POLYLINE_STEPS`]
    /// straight segments via [`draw_polyline`].
    ///
    /// Useful for curved edges in node-graphs, hand-drawn-feel agent
    /// annotations, smooth transitions in chart traces.
    pub fn draw_bezier_quad(
        &mut self,
        p0: Vector2F,
        p1: Vector2F,
        p2: Vector2F,
        width: f32,
        color: ColorU,
    ) {
        const POLYLINE_STEPS: usize = 24;
        let mut pts = Vec::with_capacity(POLYLINE_STEPS + 1);
        for i in 0..=POLYLINE_STEPS {
            let t = i as f32 / POLYLINE_STEPS as f32;
            let one_minus = 1.0 - t;
            // B(t) = (1-t)^2 P0 + 2(1-t)t P1 + t^2 P2
            let a = one_minus * one_minus;
            let b = 2.0 * one_minus * t;
            let c = t * t;
            pts.push(vec2f(
                a * p0.x() + b * p1.x() + c * p2.x(),
                a * p0.y() + b * p1.y() + c * p2.y(),
            ));
        }
        self.draw_polyline(&pts, width, color);
    }

    /// Draw a cubic Bézier curve from `p0` to `p3` with two control
    /// points `p1` and `p2`. Useful for S-shaped curves, complex
    /// edge routing, smooth-arrow connections.
    pub fn draw_bezier_cubic(
        &mut self,
        p0: Vector2F,
        p1: Vector2F,
        p2: Vector2F,
        p3: Vector2F,
        width: f32,
        color: ColorU,
    ) {
        const POLYLINE_STEPS: usize = 32;
        let mut pts = Vec::with_capacity(POLYLINE_STEPS + 1);
        for i in 0..=POLYLINE_STEPS {
            let t = i as f32 / POLYLINE_STEPS as f32;
            let one_minus = 1.0 - t;
            // B(t) = (1-t)^3 P0 + 3(1-t)^2 t P1 + 3(1-t) t^2 P2 + t^3 P3
            let a = one_minus * one_minus * one_minus;
            let b = 3.0 * one_minus * one_minus * t;
            let c = 3.0 * one_minus * t * t;
            let d = t * t * t;
            pts.push(vec2f(
                a * p0.x() + b * p1.x() + c * p2.x() + d * p3.x(),
                a * p0.y() + b * p1.y() + c * p2.y() + d * p3.y(),
            ));
        }
        self.draw_polyline(&pts, width, color);
    }

    /// Draw a sequence of pre-shaped glyphs in one call. Each tuple is
    /// `(glyph_id, font_id, position)`. The `position` is where the
    /// glyph's left edge meets the baseline. All glyphs share the
    /// same `font_size` and `color`.
    ///
    /// This is the **batch glyph primitive** — callers that already
    /// shape text (via [`crate::text_layout::TextLayoutSystem`]) can
    /// emit many glyphs without N round-trips through `draw_glyph`.
    /// A future wave adds a true `draw_text_run` that internally
    /// shapes a `&str` — this primitive is the substrate that
    /// `draw_text_run` will sit on top of.
    pub fn draw_glyphs(
        &mut self,
        glyphs: impl IntoIterator<Item = (GlyphId, FontId, Vector2F)>,
        font_size: f32,
        color: ColorU,
    ) {
        for (glyph_id, font_id, position) in glyphs {
            self.draw_glyph(position, glyph_id, font_id, font_size, color);
        }
    }

    /// Get an iterator over all layers in order, from bottom to top
    pub fn layers(&self) -> impl Iterator<Item = &Layer> {
        self.layers.iter().chain(self.overlay_layers.iter())
    }

    /// Get the total number of layers
    #[cfg(test)]
    pub fn layer_count(&self) -> usize {
        self.layers.len() + self.overlay_layers.len()
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    pub fn rendering_config(&self) -> &rendering::Config {
        &self.rendering_config
    }
}

impl Rect {
    pub fn with_corner_radius(&mut self, radius: CornerRadius) -> &mut Self {
        self.corner_radius.merge(radius);
        self
    }

    pub fn with_border(&mut self, border: Border) -> &mut Self {
        self.border = border;
        self
    }

    pub fn with_background<F>(&mut self, background: F) -> &mut Self
    where
        F: Into<Fill>,
    {
        self.background = background.into();
        self
    }

    pub fn with_drop_shadow(&mut self, drop_shadow: DropShadow) -> &mut Self {
        self.drop_shadow = Some(drop_shadow);
        self
    }
}

impl Glyph {
    pub fn with_fade(&mut self, fade: Option<GlyphFade>) -> &mut Self {
        self.fade = fade;
        self
    }
}

#[cfg(test)]
#[path = "scene_tests.rs"]
mod tests;
