//! High-level view openers. Each one composes the right data source
//! (codegraph, agent-visualizer, terminal history) into a `Canvas`.

use std::path::Path;

use wish_canvas_core::types::Canvas;

/// Open the Repo Map view for a project rooted at `repo_root`.
pub fn open_repo_canvas(repo_root: &Path) -> Canvas {
    let graph = wish_codegraph::extract_repo_graph(repo_root);
    wish_codegraph::to_canvas(&graph)
}

/// Open the Agent Plan DAG view from a recorded agent run.
///
/// Live subscription to the in-flight agent session lands in
/// `v0.5.0-step-07`.
pub fn open_agent_canvas(run: &wish_agent_visualizer::AgentRun) -> Canvas {
    wish_agent_visualizer::build_dag(run)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_repo_canvas_returns_empty_for_missing_path() {
        let c = open_repo_canvas(Path::new("/this/path/does/not/exist"));
        assert!(c.nodes.is_empty());
    }
}
