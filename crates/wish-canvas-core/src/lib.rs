//! Wish Canvas Core — UI-agnostic 2D semantic canvas.
//!
//! The retained scene graph that backs the Canvas pane (`wish-canvas`).
//! All rendering and input handling live in `wish-canvas`; this crate
//! is pure data + layout + hit-testing + exports.

pub mod export;
pub mod layout;
pub mod patch;
pub mod types;
pub mod views;

pub use patch::{CanvasPatch, CanvasPatchOp, NodeDelta};
pub use types::{
    Canvas, CanvasEdge, CanvasEdgeId, CanvasId, CanvasNode, CanvasNodeId, CanvasNodeKind,
    CanvasView, EdgeKind, EdgeStyle, LayoutMode, NodeStatus, NodeStyle, Rect, Selection, Viewport,
};

#[cfg(test)]
mod tests {
    use super::*;
    use wish_world_model::SemanticId;

    #[test]
    fn smoke_apply_patch_and_hit_test() {
        let mut c = Canvas::new();
        let node = CanvasNode::new(
            SemanticId::code_function("a::b"),
            "b",
            CanvasNodeKind::Function,
            Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 40.0,
            },
        );
        let node_id = node.id;
        let patch = CanvasPatch::new(vec![CanvasPatchOp::AddNode(node)]);
        c.apply_patch(&patch).expect("apply");
        assert_eq!(c.nodes.len(), 1);
        assert_eq!(c.hit_test((50.0, 20.0)), Some(node_id));
        assert_eq!(c.hit_test((500.0, 500.0)), None);
    }

    #[test]
    fn smoke_layout_force_directed_spreads_disconnected_nodes() {
        let mut c = Canvas::new();
        for i in 0..30 {
            c.upsert_node(CanvasNode::new(
                SemanticId::code_function(&format!("g_{i}")),
                format!("g_{i}"),
                CanvasNodeKind::Function,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 80.0,
                    h: 30.0,
                },
            ));
        }
        c.layout = LayoutMode::ForceDirected;
        layout::run(&mut c);
        // After FR layout, nodes should be spread well beyond their box size.
        let xs: Vec<f32> = c.nodes.values().map(|n| n.bounds.x).collect();
        let ys: Vec<f32> = c.nodes.values().map(|n| n.bounds.y).collect();
        let x_span = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
            - xs.iter().cloned().fold(f32::INFINITY, f32::min);
        let y_span = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
            - ys.iter().cloned().fold(f32::INFINITY, f32::min);
        assert!(
            x_span > 150.0 && y_span > 150.0,
            "force-directed layout failed to spread nodes (span x={x_span}, y={y_span})"
        );
    }

    #[test]
    fn smoke_layout_layered() {
        let mut c = Canvas::new();
        for i in 0..10 {
            c.upsert_node(CanvasNode::new(
                SemanticId::code_function(&format!("fn_{i}")),
                format!("fn_{i}"),
                CanvasNodeKind::Function,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 80.0,
                    h: 30.0,
                },
            ));
        }
        c.layout = LayoutMode::Layered;
        layout::run(&mut c);
        // After layout, nodes should be spread out (not all at origin).
        let xs: std::collections::HashSet<i32> =
            c.nodes.values().map(|n| n.bounds.x as i32).collect();
        assert!(xs.len() > 1, "layered layout should spread nodes");
    }
}
