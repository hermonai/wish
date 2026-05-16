//! Ingest `CanvasPatch` streams from the agent runtime.
//!
//! v0.5.0 ships a pure-data sink: it accepts a stream of patches and
//! either applies them immediately (Explain / Operate modes) or queues
//! them for approval (Agent mode).

use std::collections::VecDeque;
use wish_canvas_core::{patch::CanvasPatch, types::Canvas};

use crate::commands::CanvasMode;

#[derive(Debug, Default)]
pub struct AgentSink {
    pub mode: CanvasMode,
    pending: VecDeque<CanvasPatch>,
}

impl AgentSink {
    pub fn new(mode: CanvasMode) -> Self {
        Self { mode, pending: VecDeque::new() }
    }

    /// Receive a patch from the agent runtime.
    pub fn ingest(&mut self, canvas: &mut Canvas, patch: CanvasPatch) -> SinkOutcome {
        match self.mode {
            CanvasMode::Explain | CanvasMode::Operate => match canvas.apply_patch(&patch) {
                Ok(()) => SinkOutcome::Applied,
                Err(e) => SinkOutcome::Error(format!("{e}")),
            },
            CanvasMode::Agent => {
                self.pending.push_back(patch);
                SinkOutcome::PendingApproval
            }
        }
    }

    /// Approve and apply the next pending patch.
    pub fn approve_next(&mut self, canvas: &mut Canvas) -> Option<SinkOutcome> {
        let patch = self.pending.pop_front()?;
        Some(match canvas.apply_patch(&patch) {
            Ok(()) => SinkOutcome::Applied,
            Err(e) => SinkOutcome::Error(format!("{e}")),
        })
    }

    /// Reject the next pending patch.
    pub fn reject_next(&mut self) -> bool {
        self.pending.pop_front().is_some()
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinkOutcome {
    Applied,
    PendingApproval,
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use wish_canvas_core::{
        patch::CanvasPatchOp,
        types::{CanvasNode, CanvasNodeKind, Rect},
    };
    use wish_world_model::SemanticId;

    fn sample_patch() -> CanvasPatch {
        let node = CanvasNode::new(
            SemanticId::code_function("a::b"),
            "b",
            CanvasNodeKind::Function,
            Rect { x: 0.0, y: 0.0, w: 80.0, h: 30.0 },
        );
        CanvasPatch::new(vec![CanvasPatchOp::AddNode(node)])
    }

    #[test]
    fn operate_applies_immediately() {
        let mut canvas = Canvas::new();
        let mut sink = AgentSink::new(CanvasMode::Operate);
        assert_eq!(
            sink.ingest(&mut canvas, sample_patch()),
            SinkOutcome::Applied
        );
        assert_eq!(canvas.nodes.len(), 1);
        assert_eq!(sink.pending_len(), 0);
    }

    #[test]
    fn agent_queues_for_approval() {
        let mut canvas = Canvas::new();
        let mut sink = AgentSink::new(CanvasMode::Agent);
        assert_eq!(
            sink.ingest(&mut canvas, sample_patch()),
            SinkOutcome::PendingApproval
        );
        assert_eq!(canvas.nodes.len(), 0);
        assert_eq!(sink.pending_len(), 1);
        assert_eq!(sink.approve_next(&mut canvas), Some(SinkOutcome::Applied));
        assert_eq!(canvas.nodes.len(), 1);
    }
}
