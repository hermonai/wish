//! Wish Agent Visualizer — projects agent runs into Canvas DAGs and Scene
//! swarm topologies.
//!
//! v0.5.0 ships the **type surface** and a synchronous `build_dag` that
//! turns a recorded run into a canvas. Live subscription to the `ai/`
//! agent runtime arrives in `v0.5.0-step-07`.

use serde::{Deserialize, Serialize};
use wish_canvas_core::{
    layout,
    patch::{CanvasPatch, CanvasPatchOp},
    types::{Canvas, CanvasEdge, CanvasNode, CanvasNodeKind, EdgeKind, LayoutMode, Rect},
};
use wish_world_model::{Realm, SemanticId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRun {
    pub session_id: String,
    pub root_intent: String,
    pub steps: Vec<AgentStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStep {
    pub id: String,
    pub parent_id: Option<String>,
    pub kind: AgentStepKind,
    pub label: String,
    pub status: AgentStepStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStepKind {
    Plan,
    ToolCall,
    ModelCall,
    SubAgentSpawn,
    Decision,
    Wait,
    HumanGate,
    FileEdit,
    TestRun,
    Deploy,
    Approval,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStepStatus {
    Pending,
    Running,
    Ok,
    Warning,
    Error,
}

/// One agent visualizer event. v0.5.0 implementations: `CanvasPatch`
/// only. Scene patches land in v0.6.0.
#[derive(Debug, Clone)]
pub enum VisualizerEvent {
    Canvas(CanvasPatch),
    Status { step_id: String, status: AgentStepStatus },
}

/// Build an `AgentDag` canvas from a recorded run.
pub fn build_dag(run: &AgentRun) -> Canvas {
    let mut canvas = Canvas::new();
    canvas.layout = LayoutMode::Layered;
    let bounds = Rect { x: 0.0, y: 0.0, w: 200.0, h: 40.0 };

    // Root node for the run itself.
    let root_id = SemanticId::new(Realm::Agent, "run", &run.session_id);
    let root_node = CanvasNode::new(
        root_id.clone(),
        &run.root_intent,
        CanvasNodeKind::Agent,
        bounds,
    );
    let root_canvas_id = root_node.id;
    canvas.upsert_node(root_node);

    for step in &run.steps {
        let step_sid = SemanticId::new(Realm::Agent, kind_str(&step.kind), &step.id);
        let kind = match step.kind {
            AgentStepKind::Plan => CanvasNodeKind::PlanStep,
            AgentStepKind::ToolCall => CanvasNodeKind::ToolCall,
            AgentStepKind::ModelCall => CanvasNodeKind::PlanStep,
            AgentStepKind::SubAgentSpawn => CanvasNodeKind::Agent,
            AgentStepKind::Decision => CanvasNodeKind::PlanStep,
            AgentStepKind::Wait => CanvasNodeKind::PlanStep,
            AgentStepKind::HumanGate => CanvasNodeKind::PlanStep,
            AgentStepKind::FileEdit => CanvasNodeKind::Diff,
            AgentStepKind::TestRun => CanvasNodeKind::Test,
            AgentStepKind::Deploy => CanvasNodeKind::Service,
            AgentStepKind::Approval => CanvasNodeKind::PlanStep,
            AgentStepKind::Error => CanvasNodeKind::PlanStep,
        };
        let node = CanvasNode::new(step_sid, &step.label, kind, bounds);
        let node_id = node.id;
        canvas.upsert_node(node);

        let parent_id = if let Some(p) = &step.parent_id {
            let parent_sid = run
                .steps
                .iter()
                .find(|s| &s.id == p)
                .map(|s| SemanticId::new(Realm::Agent, kind_str(&s.kind), &s.id))
                .unwrap_or_else(|| root_id.clone());
            // Find the canvas node bound to the parent SemanticId.
            canvas
                .nodes
                .values()
                .find(|n| n.semantic_id == parent_sid)
                .map(|n| n.id)
                .unwrap_or(root_canvas_id)
        } else {
            root_canvas_id
        };
        canvas.upsert_edge(CanvasEdge::new(parent_id, node_id, EdgeKind::Spawned));
    }

    layout::run(&mut canvas);
    canvas
}

/// Build a [`CanvasPatch`] that produces the same DAG. Useful for agents
/// that want to emit a single patch to a live canvas.
pub fn build_dag_patch(run: &AgentRun) -> CanvasPatch {
    let canvas = build_dag(run);
    let mut ops = Vec::with_capacity(canvas.nodes.len() + canvas.edges.len());
    for n in canvas.nodes.values() {
        ops.push(CanvasPatchOp::AddNode(n.clone()));
    }
    for e in canvas.edges.values() {
        ops.push(CanvasPatchOp::AddEdge(e.clone()));
    }
    CanvasPatch::new(ops)
}

fn kind_str(k: &AgentStepKind) -> &'static str {
    match k {
        AgentStepKind::Plan => "plan",
        AgentStepKind::ToolCall => "tool_call",
        AgentStepKind::ModelCall => "model_call",
        AgentStepKind::SubAgentSpawn => "sub_agent",
        AgentStepKind::Decision => "decision",
        AgentStepKind::Wait => "wait",
        AgentStepKind::HumanGate => "human_gate",
        AgentStepKind::FileEdit => "file_edit",
        AgentStepKind::TestRun => "test_run",
        AgentStepKind::Deploy => "deploy",
        AgentStepKind::Approval => "approval",
        AgentStepKind::Error => "error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_build_dag() {
        let run = AgentRun {
            session_id: "s1".into(),
            root_intent: "fix failing test".into(),
            steps: vec![
                AgentStep {
                    id: "1".into(),
                    parent_id: None,
                    kind: AgentStepKind::Plan,
                    label: "inspect files".into(),
                    status: AgentStepStatus::Ok,
                },
                AgentStep {
                    id: "2".into(),
                    parent_id: Some("1".into()),
                    kind: AgentStepKind::FileEdit,
                    label: "patch foo.rs".into(),
                    status: AgentStepStatus::Ok,
                },
                AgentStep {
                    id: "3".into(),
                    parent_id: Some("2".into()),
                    kind: AgentStepKind::TestRun,
                    label: "cargo test".into(),
                    status: AgentStepStatus::Ok,
                },
            ],
        };
        let canvas = build_dag(&run);
        // root + 3 steps = 4 nodes; 3 edges from each step to its parent.
        assert_eq!(canvas.nodes.len(), 4);
        assert_eq!(canvas.edges.len(), 3);
        let patch = build_dag_patch(&run);
        assert!(patch.ops.len() >= 7);
    }
}
