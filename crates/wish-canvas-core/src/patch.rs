//! CanvasPatch — the mutation primitive for `Canvas`.

use crate::types::{
    Canvas, CanvasEdge, CanvasEdgeId, CanvasNode, CanvasNodeId, LayoutMode, NodeStatus, NodeStyle,
    Rect, Selection,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum CanvasPatchOp {
    AddNode(CanvasNode),
    RemoveNode { id: CanvasNodeId },
    UpdateNode { id: CanvasNodeId, delta: NodeDelta },
    AddEdge(CanvasEdge),
    RemoveEdge { id: CanvasEdgeId },
    SetLayout(LayoutMode),
    SetSelection(Selection),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeDelta {
    pub label: Option<String>,
    pub bounds: Option<Rect>,
    pub style: Option<NodeStyle>,
    pub status: Option<NodeStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasPatch {
    pub id: String,
    pub ops: Vec<CanvasPatchOp>,
}

impl CanvasPatch {
    pub fn new(ops: Vec<CanvasPatchOp>) -> Self {
        Self {
            id: format!("cpatch_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)),
            ops,
        }
    }
}

#[derive(Debug, Error)]
pub enum CanvasPatchError {
    #[error("node not found: {0}")]
    NodeNotFound(CanvasNodeId),
    #[error("edge not found: {0}")]
    EdgeNotFound(CanvasEdgeId),
}

impl Canvas {
    pub fn apply_patch(&mut self, patch: &CanvasPatch) -> Result<(), CanvasPatchError> {
        for op in &patch.ops {
            match op {
                CanvasPatchOp::AddNode(n) => {
                    self.upsert_node(n.clone());
                }
                CanvasPatchOp::RemoveNode { id } => {
                    if self.nodes.remove(id).is_none() {
                        return Err(CanvasPatchError::NodeNotFound(*id));
                    }
                    self.edges.retain(|_, e| e.from != *id && e.to != *id);
                }
                CanvasPatchOp::UpdateNode { id, delta } => {
                    let node = self
                        .nodes
                        .get_mut(id)
                        .ok_or(CanvasPatchError::NodeNotFound(*id))?;
                    if let Some(label) = &delta.label {
                        node.label = label.clone();
                    }
                    if let Some(b) = &delta.bounds {
                        node.bounds = *b;
                    }
                    if let Some(s) = &delta.style {
                        node.style = s.clone();
                    }
                    if let Some(s) = &delta.status {
                        node.status = s.clone();
                    }
                }
                CanvasPatchOp::AddEdge(e) => {
                    self.upsert_edge(e.clone());
                }
                CanvasPatchOp::RemoveEdge { id } => {
                    if self.edges.remove(id).is_none() {
                        return Err(CanvasPatchError::EdgeNotFound(*id));
                    }
                }
                CanvasPatchOp::SetLayout(mode) => {
                    self.layout = mode.clone();
                }
                CanvasPatchOp::SetSelection(sel) => {
                    self.selection = sel.clone();
                }
            }
        }
        Ok(())
    }
}
