//! The Tensorium — multi-dimensional substrate for every Wish world.
//!
//! See `wish-design/wish-plan-20260514/01-strategy/06-the-tensorium.md`
//! for the strategic frame. The short version:
//!
//! > AI lives in N-dimensional tensors. Wish's 2D canvas and 3D scene
//! > are just two projections. The Tensorium is the N-dim substrate
//! > where every `SemanticId` has a position.
//!
//! v0.5.0 ships the type seam: `TensorAxis`, `TensorAxisKind`,
//! `Tensorium`, plus `project_2d` / `project_3d` projections.
//! Worlds opt in by populating a `Tensorium`; the renderer consumes
//! the projections in v0.6.0+.

use crate::semantic_id::SemanticId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// What kind of axis is this? Drives projection defaults and any
/// per-axis UI affordances (e.g., a temporal axis gets a timeline
/// slider in v0.6.0+).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TensorAxisKind {
    /// Length, position, distance — Euclidean spatial axes.
    Spatial,
    /// Wall-clock or logical time; monotonic on the natural ordering.
    Temporal,
    /// Discrete buckets (species, element, color, branch name).
    Categorical,
    /// Real-valued, ordinal but not necessarily monotonic.
    Continuous,
    /// Latent dimension from a learned model (LLM embedding, t-SNE,
    /// UMAP, autoencoder). Not human-interpretable in isolation.
    Latent,
    /// Frequency-domain (Hz, spectrum, mode).
    Frequency,
    /// Symbolic / ordinal name space (mood, sentiment, rank).
    Symbolic,
}

/// One named axis of the Tensorium.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TensorAxis {
    pub name: String,
    pub kind: TensorAxisKind,
    /// Inclusive bounds, when the axis is bounded. `None` = unbounded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<(f32, f32)>,
    /// Optional human-readable unit (e.g., "meters", "tokens",
    /// "kcal/mol", "USD"). Display only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

impl TensorAxis {
    pub fn new(name: impl Into<String>, kind: TensorAxisKind) -> Self {
        Self {
            name: name.into(),
            kind,
            bounds: None,
            unit: None,
        }
    }

    pub fn with_bounds(mut self, lo: f32, hi: f32) -> Self {
        self.bounds = Some((lo, hi));
        self
    }

    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }
}

/// The N-dimensional substrate. Every `SemanticId` that has been
/// positioned carries a `Vec<f32>` whose length matches `axes.len()`.
/// IDs may be present without positions yet (the projection layer
/// treats them as "unplaced" and falls back to other layout).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Tensorium {
    pub axes: Vec<TensorAxis>,
    /// Per-entity positions, keyed by `SemanticId.to_string()` for
    /// stable serialization across renames.
    #[serde(default)]
    pub points: HashMap<String, Vec<f32>>,
}

impl Tensorium {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare a new axis. Returns its index, which is the position
    /// in every entity's row vector.
    pub fn add_axis(&mut self, axis: TensorAxis) -> usize {
        let idx = self.axes.len();
        self.axes.push(axis);
        // Existing rows extend with NaN for the new dimension; the
        // projection layer treats NaN as "missing" and skips that
        // entity in the projection.
        for v in self.points.values_mut() {
            v.push(f32::NAN);
        }
        idx
    }

    pub fn axis_index(&self, name: &str) -> Option<usize> {
        self.axes.iter().position(|a| a.name == name)
    }

    pub fn axis(&self, name: &str) -> Option<&TensorAxis> {
        self.axes.iter().find(|a| a.name == name)
    }

    /// Place a `SemanticId` at the given coordinate. Coordinate length
    /// must equal `axes.len()` (any extra entries are truncated; any
    /// missing entries are filled with NaN).
    pub fn set_position(&mut self, id: &SemanticId, coord: Vec<f32>) {
        let mut coord = coord;
        coord.resize(self.axes.len(), f32::NAN);
        self.points.insert(id.to_string(), coord);
    }

    pub fn position(&self, id: &SemanticId) -> Option<&Vec<f32>> {
        self.points.get(&id.to_string())
    }

    /// Project to a 2D plane along two named axes. Entities without
    /// a value on either axis are skipped.
    pub fn project_2d(&self, x_axis: &str, y_axis: &str) -> Vec<(SemanticId, (f32, f32))> {
        let Some(xi) = self.axis_index(x_axis) else {
            return Vec::new();
        };
        let Some(yi) = self.axis_index(y_axis) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(self.points.len());
        for (sid_str, row) in &self.points {
            let x = row.get(xi).copied().unwrap_or(f32::NAN);
            let y = row.get(yi).copied().unwrap_or(f32::NAN);
            if !x.is_finite() || !y.is_finite() {
                continue;
            }
            if let Some(sid) = parse_semantic_id(sid_str) {
                out.push((sid, (x, y)));
            }
        }
        out
    }

    /// Project to a 3D space along three named axes. Same skip rule
    /// as `project_2d`.
    pub fn project_3d(
        &self,
        x_axis: &str,
        y_axis: &str,
        z_axis: &str,
    ) -> Vec<(SemanticId, (f32, f32, f32))> {
        let Some(xi) = self.axis_index(x_axis) else {
            return Vec::new();
        };
        let Some(yi) = self.axis_index(y_axis) else {
            return Vec::new();
        };
        let Some(zi) = self.axis_index(z_axis) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(self.points.len());
        for (sid_str, row) in &self.points {
            let x = row.get(xi).copied().unwrap_or(f32::NAN);
            let y = row.get(yi).copied().unwrap_or(f32::NAN);
            let z = row.get(zi).copied().unwrap_or(f32::NAN);
            if !x.is_finite() || !y.is_finite() || !z.is_finite() {
                continue;
            }
            if let Some(sid) = parse_semantic_id(sid_str) {
                out.push((sid, (x, y, z)));
            }
        }
        out
    }

    /// Number of positioned entities.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Number of declared axes.
    pub fn dim(&self) -> usize {
        self.axes.len()
    }
}

/// Best-effort parse of a `SemanticId.to_string()` representation.
/// Falls back to a `Realm::Custom("?")` if the structure doesn't
/// match.
fn parse_semantic_id(s: &str) -> Option<SemanticId> {
    use crate::semantic_id::Realm;
    // Format: realm:kind:stable_key  or  realm:kind:stable_key#instance
    let (head, instance) = match s.split_once('#') {
        Some((h, i)) => (h, Some(i.to_string())),
        None => (s, None),
    };
    let mut parts = head.splitn(3, ':');
    let realm = parts.next()?;
    let kind = parts.next()?;
    let stable_key = parts.next()?;
    let realm = match realm {
        "code" => Realm::Code,
        "repo" => Realm::Repo,
        "terminal" => Realm::Terminal,
        "diagnostics" => Realm::Diagnostics,
        "agent" => Realm::Agent,
        "world" => Realm::World,
        "scene" => Realm::Scene,
        "canvas" => Realm::Canvas,
        "asset" => Realm::Asset,
        "service" => Realm::Service,
        "npc" => Realm::Npc,
        "quest" => Realm::Quest,
        "finance" => Realm::Finance,
        other => Realm::Custom(other.to_string()),
    };
    let mut sid = SemanticId::new(realm, kind, stable_key);
    if let Some(inst) = instance {
        sid = sid.with_instance(inst);
    }
    Some(sid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic_id::Realm;

    #[test]
    fn add_axis_returns_index_and_pads_existing_rows() {
        let mut t = Tensorium::new();
        let a = t.add_axis(TensorAxis::new("x", TensorAxisKind::Spatial));
        let b = t.add_axis(TensorAxis::new("y", TensorAxisKind::Spatial));
        assert_eq!(a, 0);
        assert_eq!(b, 1);
        assert_eq!(t.dim(), 2);
    }

    #[test]
    fn set_position_pads_short_coords_with_nan() {
        let mut t = Tensorium::new();
        t.add_axis(TensorAxis::new("x", TensorAxisKind::Spatial));
        t.add_axis(TensorAxis::new("y", TensorAxisKind::Spatial));
        t.add_axis(TensorAxis::new("z", TensorAxisKind::Spatial));
        let id = SemanticId::new(Realm::Code, "function", "a::b");
        t.set_position(&id, vec![1.0, 2.0]); // only x and y
        let v = t.position(&id).unwrap();
        assert_eq!(v[0], 1.0);
        assert_eq!(v[1], 2.0);
        assert!(v[2].is_nan());
    }

    #[test]
    fn project_2d_skips_entities_missing_a_value() {
        let mut t = Tensorium::new();
        t.add_axis(TensorAxis::new("risk", TensorAxisKind::Continuous));
        t.add_axis(TensorAxis::new("return", TensorAxisKind::Continuous));
        let a = SemanticId::new(Realm::Finance, "asset", "alpha");
        let b = SemanticId::new(Realm::Finance, "asset", "beta");
        let c = SemanticId::new(Realm::Finance, "asset", "gamma");
        t.set_position(&a, vec![0.1, 0.05]);
        t.set_position(&b, vec![0.4, 0.12]);
        t.set_position(&c, vec![f32::NAN, 0.9]); // missing risk
        let proj = t.project_2d("risk", "return");
        let names: Vec<String> = proj.iter().map(|(s, _)| s.stable_key.clone()).collect();
        assert!(names.contains(&"alpha".to_string()));
        assert!(names.contains(&"beta".to_string()));
        assert!(
            !names.contains(&"gamma".to_string()),
            "gamma had NaN, should be skipped"
        );
    }

    #[test]
    fn project_3d_returns_triple() {
        let mut t = Tensorium::new();
        for n in ["x", "y", "z"] {
            t.add_axis(TensorAxis::new(n, TensorAxisKind::Spatial));
        }
        let id = SemanticId::new(Realm::Scene, "npc", "merchant_liu");
        t.set_position(&id, vec![1.0, 2.0, 3.0]);
        let p = t.project_3d("x", "y", "z");
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].1, (1.0, 2.0, 3.0));
    }

    #[test]
    fn project_with_unknown_axis_returns_empty() {
        let mut t = Tensorium::new();
        t.add_axis(TensorAxis::new("x", TensorAxisKind::Spatial));
        let id = SemanticId::new(Realm::Code, "fn", "a");
        t.set_position(&id, vec![1.0]);
        let p = t.project_2d("x", "nope");
        assert!(p.is_empty());
    }

    #[test]
    fn serde_roundtrip_preserves_axes_and_points() {
        let mut t = Tensorium::new();
        t.add_axis(
            TensorAxis::new("time", TensorAxisKind::Temporal)
                .with_bounds(0.0, 1.0)
                .with_unit("seconds"),
        );
        t.add_axis(TensorAxis::new("energy", TensorAxisKind::Continuous).with_unit("kcal"));
        let id = SemanticId::new(Realm::Custom("physics".into()), "particle", "p1");
        t.set_position(&id, vec![0.3, 7.5]);
        let json = serde_json::to_string(&t).unwrap();
        let back: Tensorium = serde_json::from_str(&json).unwrap();
        assert_eq!(back.axes.len(), 2);
        assert_eq!(back.axes[0].name, "time");
        assert!(back.axes[0].bounds.is_some());
        assert_eq!(back.axes[1].unit.as_deref(), Some("kcal"));
        assert_eq!(back.position(&id), Some(&vec![0.3, 7.5]));
    }
}
