//! Canvas types.

use crate::tensor::TensorSpec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wish_world_model::SemanticId;

pub type CanvasId = String;
pub type CanvasNodeId = u64;
pub type CanvasEdgeId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, (px, py): (f32, f32)) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w * 0.5, self.y + self.h * 0.5)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub pan_x: f32,
    pub pan_y: f32,
    pub scale: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            pan_x: 0.0,
            pan_y: 0.0,
            scale: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutMode {
    Manual,
    #[default]
    Layered,
    ForceDirected,
    Grid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanvasNodeKind {
    File,
    Function,
    Crate,
    Package,
    Module,
    Service,
    Agent,
    ToolCall,
    PlanStep,
    Test,
    Commit,
    Branch,
    Diff,
    TerminalBlock,
    DocumentSection,
    Npc,
    Quest,
    /// A tensor entity — embedding matrix, attention head, layer
    /// activation, etc. The `TensorSpec` carries shape + dtype + data
    /// handle; the renderer (`wish-canvas` / `wish-tensor-view`) decides
    /// whether to draw it as a tile, a sparkline, or a heatmap.
    Tensor(TensorSpec),
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    #[default]
    Ok,
    Warning,
    Error,
    Pending,
    Running,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeStyle {
    /// RGBA, 0..=255.
    pub fill: [u8; 4],
    pub border: [u8; 4],
    pub border_width: f32,
    pub corner_radius: f32,
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self {
            fill: [40, 44, 52, 255],
            border: [97, 175, 239, 255],
            border_width: 1.0,
            corner_radius: 6.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasNode {
    pub id: CanvasNodeId,
    pub semantic_id: SemanticId,
    pub kind: CanvasNodeKind,
    pub label: String,
    pub bounds: Rect,
    pub style: NodeStyle,
    pub status: NodeStatus,
}

impl CanvasNode {
    pub fn new(
        semantic_id: SemanticId,
        label: impl Into<String>,
        kind: CanvasNodeKind,
        bounds: Rect,
    ) -> Self {
        let id = stable_hash_u64(&format!("{semantic_id}"));
        Self {
            id,
            semantic_id,
            kind,
            label: label.into(),
            bounds,
            style: NodeStyle::default(),
            status: NodeStatus::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Imports,
    Calls,
    DependsOn,
    Produces,
    Triggers,
    Mentions,
    Tests,
    Spawned,
    SucceededBy,
    FailedBy,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeStyle {
    pub color: [u8; 4],
    pub width: f32,
    /// Dashed line if non-zero.
    pub dash_period: f32,
}

impl Default for EdgeStyle {
    fn default() -> Self {
        Self {
            color: [97, 175, 239, 160],
            width: 1.0,
            dash_period: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasEdge {
    pub id: CanvasEdgeId,
    pub from: CanvasNodeId,
    pub to: CanvasNodeId,
    pub kind: EdgeKind,
    pub style: EdgeStyle,
}

impl CanvasEdge {
    pub fn new(from: CanvasNodeId, to: CanvasNodeId, kind: EdgeKind) -> Self {
        let id = stable_hash_u64(&format!("{from}->{to}:{kind:?}"));
        Self {
            id,
            from,
            to,
            kind,
            style: EdgeStyle::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Selection {
    pub nodes: Vec<CanvasNodeId>,
    pub edges: Vec<CanvasEdgeId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Canvas {
    pub id: CanvasId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_ref: Option<String>,
    pub nodes: HashMap<CanvasNodeId, CanvasNode>,
    pub edges: HashMap<CanvasEdgeId, CanvasEdge>,
    pub viewport: Viewport,
    pub layout: LayoutMode,
    pub selection: Selection,
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

impl Canvas {
    pub fn new() -> Self {
        Self {
            id: format!("canvas_{}", chrono::Utc::now().timestamp_micros()),
            world_ref: None,
            nodes: HashMap::new(),
            edges: HashMap::new(),
            viewport: Viewport::default(),
            layout: LayoutMode::default(),
            selection: Selection::default(),
        }
    }

    pub fn upsert_node(&mut self, node: CanvasNode) {
        self.nodes.insert(node.id, node);
    }

    pub fn upsert_edge(&mut self, edge: CanvasEdge) {
        self.edges.insert(edge.id, edge);
    }

    pub fn hit_test(&self, point: (f32, f32)) -> Option<CanvasNodeId> {
        self.nodes
            .values()
            .filter(|n| n.bounds.contains(point))
            .max_by(|a, b| {
                (a.bounds.w * a.bounds.h)
                    .partial_cmp(&(b.bounds.w * b.bounds.h))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|n| n.id)
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.viewport.pan_x += dx;
        self.viewport.pan_y += dy;
    }

    pub fn zoom(&mut self, factor: f32) {
        self.viewport.scale = (self.viewport.scale * factor).clamp(0.05, 20.0);
    }

    /// Pan/zoom so that the node bound to `semantic_id` is centered.
    pub fn reveal(&mut self, semantic_id: &SemanticId) -> Option<CanvasNodeId> {
        let node = self
            .nodes
            .values()
            .find(|n| n.semantic_id == *semantic_id)?;
        let (cx, cy) = node.bounds.center();
        self.viewport.pan_x = -cx;
        self.viewport.pan_y = -cy;
        Some(node.id)
    }
}

/// Which view to project from a WishWorld.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanvasView {
    RepoMap,
    AgentDag,
    CommandTimeline,
    GitWorldline,
    Custom(String),
}

fn stable_hash_u64(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}
