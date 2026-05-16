//! Wish Scene3D — the v0.5.0 "early seam" of `wish-scene-renderer`.
//!
//! A self-contained 3D scene renderer that doesn't pull in glam or
//! nalgebra. Pure inline math: orbit camera, perspective projection,
//! depth sort, billboard-style entity rendering, a ground grid, and
//! scene-edge wiring (from `WorldScene::entity_ids`).
//!
//! Drawing still happens through egui's 2D painter — we just project
//! 3D points to 2D screen space first. That keeps the dependency
//! surface identical to the 2D viewer and lets us ship a real
//! orbit-camera scene **today**. The full direct-wgpu pipeline lands
//! in v0.6.0 as `wish-scene-renderer`.

use eframe::egui::{self, Color32, FontId, Pos2, Rect, Rounding, Stroke, Vec2};
use wish_canvas_core::types::{CanvasNode, CanvasNodeId};
use wish_world_model::{Component, WishWorld, WorldEntity};

#[derive(Debug, Clone, Copy)]
pub struct Camera3D {
    /// Target the camera orbits around (world-space).
    pub target: [f32; 3],
    /// Yaw in radians (rotation around world up).
    pub yaw: f32,
    /// Pitch in radians (signed; positive looks down).
    pub pitch: f32,
    /// Distance from target to camera origin (world units).
    pub distance: f32,
    /// Vertical field of view, radians.
    pub fov_y: f32,
}

impl Default for Camera3D {
    fn default() -> Self {
        Self {
            target: [0.0, 0.0, 0.0],
            yaw: std::f32::consts::FRAC_PI_4,
            pitch: 0.45,
            distance: 30.0,
            fov_y: 1.0,
        }
    }
}

impl Camera3D {
    pub fn eye(&self) -> [f32; 3] {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        [
            self.target[0] + self.distance * cp * sy,
            self.target[1] + self.distance * sp,
            self.target[2] + self.distance * cp * cy,
        ]
    }

    /// Compose view * projection into a 4x4 row-major matrix.
    pub fn view_projection(&self, aspect: f32) -> [[f32; 4]; 4] {
        let eye = self.eye();
        let up = [0.0_f32, 1.0, 0.0];
        let view = look_at(eye, self.target, up);
        let near = 0.5_f32;
        let far = 1000.0_f32;
        let proj = perspective(self.fov_y, aspect, near, far);
        mat4_mul(proj, view)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProjectedNode {
    pub canvas_id: CanvasNodeId,
    pub screen: Pos2,
    /// View-space depth (positive = in front of camera).
    pub depth: f32,
    /// World-space ground point (y=0 below the entity).
    pub ground_screen: Pos2,
}

/// Render a 3D scene of the world's entities into the given egui
/// painter. Returns the projected list (sorted nearest-last) so the
/// caller can do click-picking against the same projected positions.
pub fn render(
    painter: &egui::Painter,
    viewport: Rect,
    world: &WishWorld,
    canvas_nodes: &std::collections::HashMap<CanvasNodeId, CanvasNode>,
    camera: &Camera3D,
    selected: Option<CanvasNodeId>,
) -> Vec<ProjectedNode> {
    // Background.
    painter.rect_filled(viewport, 0.0, Color32::from_rgb(11, 14, 19));

    let aspect = (viewport.width() / viewport.height()).max(0.01);
    let vp = camera.view_projection(aspect);

    // Ground grid.
    draw_ground_grid(painter, viewport, &vp);

    // Project each entity (those that have a Transform component) to
    // screen space and depth-sort.
    let mut projected: Vec<ProjectedNode> = Vec::with_capacity(world.entities.len());
    for entity in world.entities.values() {
        let Some([x, y, z]) = entity_translation(entity) else {
            continue;
        };
        let Some((screen, depth)) = project_point([x, y, z], &vp, viewport) else {
            continue;
        };
        let Some((ground, _)) = project_point([x, 0.0, z], &vp, viewport) else {
            continue;
        };
        // Find the canvas node bound to this entity's SemanticId so we
        // can pick the same color palette + click-routing as 2D.
        let Some(canvas_id) = canvas_nodes
            .values()
            .find(|n| n.semantic_id == entity.id)
            .map(|n| n.id)
        else {
            continue;
        };
        projected.push(ProjectedNode {
            canvas_id,
            screen,
            depth,
            ground_screen: ground,
        });
    }

    // Draw scene edges first — from `WorldScene::entity_ids`. Each
    // scene's entities form a star around the scene's centroid.
    for scene in world.scenes.values() {
        // Find the projected screens for each entity in the scene.
        let mut points: Vec<(CanvasNodeId, Pos2)> = Vec::new();
        for sid in &scene.entity_ids {
            if let Some(entity) = world.entities.get(&sid.to_string()) {
                if let Some([x, y, z]) = entity_translation(entity) {
                    if let Some((screen, _)) = project_point([x, y, z], &vp, viewport) {
                        if let Some(canvas_id) = canvas_nodes
                            .values()
                            .find(|n| n.semantic_id == entity.id)
                            .map(|n| n.id)
                        {
                            points.push((canvas_id, screen));
                        }
                    }
                }
            }
        }
        if points.len() < 2 {
            continue;
        }
        // Star: connect every point to the centroid.
        let (cx, cy) = points
            .iter()
            .fold((0.0, 0.0), |(ax, ay), (_, p)| (ax + p.x, ay + p.y));
        let centroid = Pos2::new(cx / points.len() as f32, cy / points.len() as f32);
        for (_, p) in &points {
            painter.line_segment(
                [centroid, *p],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 135, 160, 90)),
            );
        }
    }

    // Sort projected entities far-to-near for correct overdraw.
    projected.sort_by(|a, b| {
        b.depth
            .partial_cmp(&a.depth)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for p in &projected {
        let Some(node) = canvas_nodes.get(&p.canvas_id) else {
            continue;
        };

        // Vertical "stake" from the ground projection up to the node.
        painter.line_segment(
            [p.ground_screen, p.screen],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(140, 150, 165, 90)),
        );
        // Ground dot.
        painter.circle_filled(
            p.ground_screen,
            2.5,
            Color32::from_rgba_unmultiplied(97, 175, 239, 140),
        );

        // Billboard rectangle sized by depth.
        let scale = (200.0 / p.depth.max(1.0)).clamp(0.5, 4.0);
        let half_w = 56.0 * scale;
        let half_h = 16.0 * scale;
        let bbox = Rect::from_min_max(
            Pos2::new(p.screen.x - half_w, p.screen.y - half_h),
            Pos2::new(p.screen.x + half_w, p.screen.y + half_h),
        );
        let fill = depth_shade(super::kind_tint(&node.kind), p.depth);
        let is_sel = selected == Some(node.id);
        let stroke = if is_sel {
            Stroke::new(2.5, Color32::from_rgb(97, 175, 239))
        } else {
            Stroke::new(1.0, Color32::from_rgb(60, 70, 84))
        };
        painter.rect(bbox, Rounding::same(4.0), fill, stroke);

        // Label.
        let font_size = (12.0 * scale.min(1.5)).clamp(8.0, 18.0);
        painter.text(
            bbox.left_top() + Vec2::new(6.0 * scale, 3.0 * scale),
            egui::Align2::LEFT_TOP,
            &node.label,
            FontId::proportional(font_size),
            Color32::from_rgb(220, 232, 244),
        );
    }

    projected
}

/// Click-pick a 3D node by screen distance to its projected billboard
/// center. Returns the nearest within `radius` pixels.
pub fn pick_at(projected: &[ProjectedNode], at: Pos2, radius: f32) -> Option<CanvasNodeId> {
    let r2 = radius * radius;
    let mut best: Option<(CanvasNodeId, f32)> = None;
    for p in projected {
        let dx = p.screen.x - at.x;
        let dy = p.screen.y - at.y;
        let d2 = dx * dx + dy * dy;
        if d2 <= r2 && best.map(|(_, b)| d2 < b).unwrap_or(true) {
            best = Some((p.canvas_id, d2));
        }
    }
    best.map(|(id, _)| id)
}

// -- linear algebra primitives -----------------------------------------

fn look_at(eye: [f32; 3], target: [f32; 3], up: [f32; 3]) -> [[f32; 4]; 4] {
    let f = normalize(sub(target, eye));
    let s = normalize(cross(f, up));
    let u = cross(s, f);
    [
        [s[0], u[0], -f[0], 0.0],
        [s[1], u[1], -f[1], 0.0],
        [s[2], u[2], -f[2], 0.0],
        [-dot(s, eye), -dot(u, eye), dot(f, eye), 1.0],
    ]
}

fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> [[f32; 4]; 4] {
    let f = 1.0 / (fov_y * 0.5).tan();
    let nf = 1.0 / (near - far);
    [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, (far + near) * nf, -1.0],
        [0.0, 0.0, 2.0 * far * near * nf, 0.0],
    ]
}

fn mat4_mul(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[k][j] * b[i][k];
            }
            out[i][j] = s;
        }
    }
    out
}

fn mat4_mul_vec4(m: [[f32; 4]; 4], v: [f32; 4]) -> [f32; 4] {
    [
        m[0][0] * v[0] + m[1][0] * v[1] + m[2][0] * v[2] + m[3][0] * v[3],
        m[0][1] * v[0] + m[1][1] * v[1] + m[2][1] * v[2] + m[3][1] * v[3],
        m[0][2] * v[0] + m[1][2] * v[1] + m[2][2] * v[2] + m[3][2] * v[3],
        m[0][3] * v[0] + m[1][3] * v[1] + m[2][3] * v[2] + m[3][3] * v[3],
    ]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / l, v[1] / l, v[2] / l]
}

/// Project a world-space point to screen space via a view-projection
/// matrix. Returns None if the point lies behind the camera.
fn project_point(p: [f32; 3], vp: &[[f32; 4]; 4], viewport: Rect) -> Option<(Pos2, f32)> {
    let clip = mat4_mul_vec4(*vp, [p[0], p[1], p[2], 1.0]);
    let w = clip[3];
    if w <= 0.01 {
        return None;
    }
    let ndc_x = clip[0] / w;
    let ndc_y = clip[1] / w;
    let depth = w; // positive => in front
    let sx = viewport.min.x + (ndc_x * 0.5 + 0.5) * viewport.width();
    let sy = viewport.min.y + (1.0 - (ndc_y * 0.5 + 0.5)) * viewport.height();
    Some((Pos2::new(sx, sy), depth))
}

fn draw_ground_grid(painter: &egui::Painter, viewport: Rect, vp: &[[f32; 4]; 4]) {
    let extent = 20i32;
    let step = 2.0_f32;
    let color = Color32::from_rgba_unmultiplied(60, 70, 85, 130);
    let major = Color32::from_rgba_unmultiplied(100, 115, 135, 180);
    for i in -extent..=extent {
        let x = i as f32 * step;
        let a = project_point([x, 0.0, -(extent as f32) * step], vp, viewport);
        let b = project_point([x, 0.0, (extent as f32) * step], vp, viewport);
        if let (Some((p0, _)), Some((p1, _))) = (a, b) {
            let c = if i == 0 { major } else { color };
            painter.line_segment([p0, p1], Stroke::new(1.0, c));
        }
        let z = i as f32 * step;
        let a = project_point([-(extent as f32) * step, 0.0, z], vp, viewport);
        let b = project_point([(extent as f32) * step, 0.0, z], vp, viewport);
        if let (Some((p0, _)), Some((p1, _))) = (a, b) {
            let c = if i == 0 { major } else { color };
            painter.line_segment([p0, p1], Stroke::new(1.0, c));
        }
    }
}

/// Apply a simple depth-fog tint to a base color so distant things look distant.
fn depth_shade(base: Color32, depth: f32) -> Color32 {
    let fog_start = 6.0_f32;
    let fog_end = 80.0_f32;
    let t = ((depth - fog_start) / (fog_end - fog_start)).clamp(0.0, 0.8);
    let bg = (14.0, 17.0, 22.0);
    let r = base.r() as f32 * (1.0 - t) + bg.0 * t;
    let g = base.g() as f32 * (1.0 - t) + bg.1 * t;
    let b = base.b() as f32 * (1.0 - t) + bg.2 * t;
    Color32::from_rgba_unmultiplied(r as u8, g as u8, b as u8, base.a().max(220))
}

fn entity_translation(entity: &WorldEntity) -> Option<[f32; 3]> {
    for c in &entity.components {
        if let Component::Transform(t) = c {
            return Some(t.translation);
        }
    }
    None
}

/// Compute a world-space centroid for a `WishWorld`'s positioned entities.
/// Used to auto-target the camera when the scene loads.
pub fn world_centroid_and_extent(world: &WishWorld) -> ([f32; 3], f32) {
    let mut count = 0u32;
    let mut sum = [0.0_f32; 3];
    let mut mn = [f32::INFINITY; 3];
    let mut mx = [f32::NEG_INFINITY; 3];
    for entity in world.entities.values() {
        if let Some(t) = entity_translation(entity) {
            count += 1;
            for i in 0..3 {
                sum[i] += t[i];
                if t[i] < mn[i] {
                    mn[i] = t[i];
                }
                if t[i] > mx[i] {
                    mx[i] = t[i];
                }
            }
        }
    }
    if count == 0 {
        return ([0.0, 0.0, 0.0], 20.0);
    }
    let c = [
        sum[0] / count as f32,
        sum[1] / count as f32,
        sum[2] / count as f32,
    ];
    let extent = (mx[0] - mn[0]).max(mx[2] - mn[2]).max(2.0);
    (c, extent.max(8.0) * 1.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wish_world_model::{
        Component, EntityKind, Realm, SemanticId, Transform, WishWorld, WorldEntity, WorldKind,
    };

    fn entity_at(name: &str, t: [f32; 3]) -> WorldEntity {
        WorldEntity {
            id: SemanticId::new(Realm::Npc, "npc", name),
            kind: EntityKind::Npc,
            display_name: name.into(),
            components: vec![Component::Transform(Transform {
                translation: t,
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            })],
            source_ref: None,
            agent_ref: None,
            status: wish_world_model::EntityStatus::Ok,
            agent_editable: true,
            provenance_head: None,
        }
    }

    #[test]
    fn centroid_of_origin_only_is_origin() {
        let mut w = WishWorld::new("t", WorldKind::GenericProject);
        w.upsert_entity(entity_at("a", [0.0, 0.0, 0.0]));
        let (c, ext) = world_centroid_and_extent(&w);
        assert_eq!(c, [0.0, 0.0, 0.0]);
        assert!(ext >= 8.0);
    }

    #[test]
    fn centroid_of_spread_world_is_their_mean() {
        let mut w = WishWorld::new("t", WorldKind::GenericProject);
        w.upsert_entity(entity_at("a", [-10.0, 0.0, 0.0]));
        w.upsert_entity(entity_at("b", [10.0, 0.0, 0.0]));
        let (c, ext) = world_centroid_and_extent(&w);
        assert!(c[0].abs() < 0.01);
        assert!(ext >= 20.0 * 1.5 * 0.99);
    }

    #[test]
    fn perspective_projection_of_in_front_point_returns_some() {
        let cam = Camera3D::default();
        let vp = cam.view_projection(16.0 / 9.0);
        let rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1280.0, 720.0));
        let p = project_point([0.0, 0.0, 0.0], &vp, rect);
        assert!(p.is_some());
        let (pos, depth) = p.unwrap();
        assert!(depth > 0.0);
        // Should land inside the viewport for a default camera centered on origin.
        assert!(pos.x >= 0.0 && pos.x <= 1280.0);
        assert!(pos.y >= 0.0 && pos.y <= 720.0);
    }

    #[test]
    fn pick_at_returns_nearest_within_radius() {
        let projected = vec![
            ProjectedNode {
                canvas_id: 1,
                screen: Pos2::new(100.0, 100.0),
                depth: 10.0,
                ground_screen: Pos2::new(100.0, 200.0),
            },
            ProjectedNode {
                canvas_id: 2,
                screen: Pos2::new(200.0, 100.0),
                depth: 12.0,
                ground_screen: Pos2::new(200.0, 200.0),
            },
        ];
        assert_eq!(pick_at(&projected, Pos2::new(102.0, 99.0), 20.0), Some(1));
        assert_eq!(pick_at(&projected, Pos2::new(198.0, 101.0), 20.0), Some(2));
        assert_eq!(pick_at(&projected, Pos2::new(500.0, 500.0), 20.0), None);
    }
}
