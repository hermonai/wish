use pathfinder_geometry::vector::vec2f;

use crate::rendering;

use super::*;

#[test]
fn test_hit_rect_recording() {
    let mut scene = Scene::new(1., rendering::Config::default());
    assert_eq!(ZIndex::new(0), scene.z_index());

    scene.draw_rect_with_hit_recording(RectF::new(vec2f(0., 0.), vec2f(100., 100.)));
    assert_eq!(ZIndex::new(0), scene.z_index());
    assert!(!scene.is_covered(Point::new(10., 10., ZIndex::new(0))));

    scene.start_layer(ClipBounds::ActiveLayer);
    scene.draw_rect_with_hit_recording(RectF::new(vec2f(50., 50.), vec2f(100., 100.)));
    assert_eq!(ZIndex::new(1), scene.z_index());
    assert!(!scene.is_covered(Point::new(10., 10., ZIndex::new(0))));
    assert!(scene.is_covered(Point::new(60., 60., ZIndex::new(0))));

    scene.start_layer(ClipBounds::ActiveLayer);
    scene.draw_rect_with_hit_recording(RectF::new(vec2f(0., 0.), vec2f(100., 100.)));
    assert_eq!(ZIndex::new(2), scene.z_index());
    assert!(scene.is_covered(Point::new(10., 10., ZIndex::new(0))));
    assert!(scene.is_covered(Point::new(60., 60., ZIndex::new(1))));
}

#[test]
fn test_nested_clip_bounds_with_intersection() {
    let mut scene = Scene::new(1., rendering::Config::default());

    let bounds1 = RectF::new(Vector2F::zero(), Vector2F::new(10., 10.));
    scene.start_layer(ClipBounds::BoundedBy(bounds1));

    let bounds2 = RectF::new(Vector2F::new(5., 5.), Vector2F::new(10., 10.));
    scene.start_layer(ClipBounds::BoundedByActiveLayerAnd(bounds2));

    assert_eq!(
        scene.active_layer().clip_bounds,
        Some(RectF::new(Vector2F::new(5., 5.), Vector2F::new(5., 5.)))
    );
}

#[test]
fn test_nested_clip_bounds_with_no_intersection() {
    let mut scene = Scene::new(1., rendering::Config::default());

    let bounds1 = RectF::new(Vector2F::zero(), Vector2F::new(10., 10.));
    scene.start_layer(ClipBounds::BoundedBy(bounds1));

    let bounds2 = RectF::new(Vector2F::new(100., 100.), Vector2F::new(10., 10.));
    scene.start_layer(ClipBounds::BoundedByActiveLayerAnd(bounds2));

    // We should have explicit bounds for this layer.  (None represents a lack
    // of clipping, not clipping they layer down to nothingness.)
    assert!(scene.active_layer().clip_bounds.is_some());
    // The clip bounds should have an area of zero.
    assert!(scene.active_layer().clip_bounds.unwrap().is_empty());
}

#[test]
fn test_click_through_layer_does_not_cover_lower_layers() {
    let mut scene = Scene::new(1., rendering::Config::default());

    scene.start_layer(ClipBounds::ActiveLayer);
    scene.set_active_layer_click_through();
    scene.draw_rect_with_hit_recording(RectF::new(vec2f(0., 0.), vec2f(100., 100.)));

    assert!(!scene.is_covered(Point::new(10., 10., ZIndex::new(0))));
}

#[test]
fn draw_line_emits_rects_proportional_to_length() {
    let mut scene = Scene::new(1., rendering::Config::default());
    scene.draw_line(
        vec2f(0., 0.),
        vec2f(100., 0.),
        2.0,
        pathfinder_color::ColorU::white(),
    );
    // Step = width * 0.6 = 1.2, n = ceil(100/1.2) = 84, so 84+1 = 85 squares.
    let layer = scene.layers().next().unwrap();
    assert!(
        layer.rects.len() >= 50,
        "expected many rects for a 100-unit line, got {}",
        layer.rects.len()
    );
    assert!(
        layer.rects.len() <= 200,
        "rect count should be bounded, got {}",
        layer.rects.len()
    );
}

#[test]
fn draw_line_skips_degenerate_segments() {
    let mut scene = Scene::new(1., rendering::Config::default());
    scene.draw_line(
        vec2f(10., 10.),
        vec2f(10., 10.1),
        1.0,
        pathfinder_color::ColorU::white(),
    );
    let layer = scene.layers().next().unwrap();
    assert_eq!(layer.rects.len(), 0, "lines shorter than 0.5 should no-op");
}

#[test]
fn draw_polyline_chains_segments() {
    let mut scene = Scene::new(1., rendering::Config::default());
    let pts = vec![vec2f(0., 0.), vec2f(50., 0.), vec2f(50., 50.)];
    scene.draw_polyline(&pts, 2.0, pathfinder_color::ColorU::white());
    let layer = scene.layers().next().unwrap();
    // Two 50-unit segments should produce roughly twice the rects of one.
    assert!(
        layer.rects.len() > 60,
        "polyline should chain segments, got {}",
        layer.rects.len()
    );
}

#[test]
fn draw_circle_emits_one_rounded_rect() {
    let mut scene = Scene::new(1., rendering::Config::default());
    scene.draw_circle(vec2f(50., 50.), 10.0, pathfinder_color::ColorU::white());
    let layer = scene.layers().next().unwrap();
    assert_eq!(layer.rects.len(), 1);
    let r = &layer.rects[0];
    assert_eq!(r.bounds.origin(), vec2f(40., 40.));
    assert_eq!(r.bounds.size(), vec2f(20., 20.));
}

#[test]
fn draw_circle_zero_radius_no_ops() {
    let mut scene = Scene::new(1., rendering::Config::default());
    scene.draw_circle(vec2f(0., 0.), 0.0, pathfinder_color::ColorU::white());
    let layer = scene.layers().next().unwrap();
    assert_eq!(layer.rects.len(), 0);
}

#[test]
fn draw_arrow_adds_two_head_segments_to_base_line() {
    let mut scene = Scene::new(1., rendering::Config::default());
    // Compare a 50-unit arrow with the same-length line.
    scene.draw_line(
        vec2f(0., 0.),
        vec2f(50., 0.),
        2.0,
        pathfinder_color::ColorU::white(),
    );
    let line_only_rects = scene.layers().next().unwrap().rects.len();
    let mut scene = Scene::new(1., rendering::Config::default());
    scene.draw_arrow(
        vec2f(0., 0.),
        vec2f(50., 0.),
        2.0,
        pathfinder_color::ColorU::white(),
        10.0,
    );
    let arrow_rects = scene.layers().next().unwrap().rects.len();
    assert!(
        arrow_rects > line_only_rects,
        "arrow should add head segments: arrow={arrow_rects} vs line_only={line_only_rects}"
    );
}

#[test]
fn draw_arrow_skips_head_when_segment_too_short() {
    let mut scene = Scene::new(1., rendering::Config::default());
    // 5-unit line with 10-unit head: too short, head suppressed.
    scene.draw_arrow(
        vec2f(0., 0.),
        vec2f(5., 0.),
        1.0,
        pathfinder_color::ColorU::white(),
        10.0,
    );
    let n = scene.layers().next().unwrap().rects.len();
    // Just the base line (no head).
    assert!(
        n > 0 && n < 30,
        "short arrow should be just the base line, got {n}"
    );
}

#[test]
fn draw_arrow_zero_head_size_acts_as_line() {
    let mut scene = Scene::new(1., rendering::Config::default());
    scene.draw_arrow(
        vec2f(0., 0.),
        vec2f(50., 0.),
        2.0,
        pathfinder_color::ColorU::white(),
        0.0,
    );
    let arrow_rects = scene.layers().next().unwrap().rects.len();
    let mut scene = Scene::new(1., rendering::Config::default());
    scene.draw_line(
        vec2f(0., 0.),
        vec2f(50., 0.),
        2.0,
        pathfinder_color::ColorU::white(),
    );
    let line_rects = scene.layers().next().unwrap().rects.len();
    assert_eq!(
        arrow_rects, line_rects,
        "zero head size should be equivalent to draw_line"
    );
}

#[test]
fn draw_rect_outline_emits_four_line_segments() {
    let mut scene = Scene::new(1., rendering::Config::default());
    scene.draw_rect_outline(
        RectF::new(vec2f(0., 0.), vec2f(100., 50.)),
        2.0,
        pathfinder_color::ColorU::white(),
    );
    let n = scene.layers().next().unwrap().rects.len();
    // 4 sides; each emits ~step-many rects. Should be > one side's worth.
    assert!(n > 100, "outline should emit four full sides, got {n}");
}

#[test]
fn draw_grid_emits_axes_per_cell() {
    let mut scene = Scene::new(1., rendering::Config::default());
    // 100x100 grid with cell=25 → 5 verticals × 5 horizontals = 10 lines.
    scene.draw_grid(
        RectF::new(vec2f(0., 0.), vec2f(100., 100.)),
        25.0,
        1.0,
        pathfinder_color::ColorU::white(),
    );
    let n = scene.layers().next().unwrap().rects.len();
    // Each ~100-unit line stipple is ~166 rects; 10 lines is several
    // hundred. Just confirm it's substantial.
    assert!(n > 500, "grid should emit many rects, got {n}");
}

#[test]
fn draw_grid_no_ops_on_zero_cell() {
    let mut scene = Scene::new(1., rendering::Config::default());
    scene.draw_grid(
        RectF::new(vec2f(0., 0.), vec2f(100., 100.)),
        0.0,
        1.0,
        pathfinder_color::ColorU::white(),
    );
    let n = scene.layers().next().unwrap().rects.len();
    assert_eq!(n, 0, "cell_size=0 should no-op");
}
