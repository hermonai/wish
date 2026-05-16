//! Input → CanvasPatch translation. Pure logic; pointer events are
//! provided by `wishui` integration code in `v0.5.0-step-05`.

use serde::{Deserialize, Serialize};
use wish_canvas_core::{
    patch::{CanvasPatch, CanvasPatchOp},
    types::{Canvas, CanvasNodeId, Selection},
};

/// A high-level canvas input event. The WishUI integration translates
/// raw pointer/keyboard events into these.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CanvasInput {
    Pan { dx: f32, dy: f32 },
    Zoom { factor: f32 },
    Select { node_id: CanvasNodeId },
    DeselectAll,
    DoubleClick { node_id: CanvasNodeId },
}

/// Apply a [`CanvasInput`] to a canvas, returning a [`CanvasPatch`] if
/// the input mutates the canvas state. Pan/zoom are applied directly to
/// the viewport (no patch needed).
pub fn handle_input(canvas: &mut Canvas, input: CanvasInput) -> Option<CanvasPatch> {
    match input {
        CanvasInput::Pan { dx, dy } => {
            canvas.pan(dx, dy);
            None
        }
        CanvasInput::Zoom { factor } => {
            canvas.zoom(factor);
            None
        }
        CanvasInput::Select { node_id } => {
            let sel = Selection {
                nodes: vec![node_id],
                edges: Vec::new(),
            };
            Some(CanvasPatch::new(vec![CanvasPatchOp::SetSelection(sel)]))
        }
        CanvasInput::DeselectAll => Some(CanvasPatch::new(vec![CanvasPatchOp::SetSelection(
            Selection::default(),
        )])),
        CanvasInput::DoubleClick { .. } => None, // handled by the wishui integration → Reveal-in-Editor
    }
}
