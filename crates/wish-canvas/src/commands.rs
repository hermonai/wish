//! Command palette actions exposed by the Canvas pane.

use serde::{Deserialize, Serialize};
use wish_canvas_core::types::CanvasView;
use wish_world_model::SemanticId;

/// Every action a user can invoke from the command palette that targets
/// the Canvas pane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanvasCommand {
    OpenRepoCanvas,
    OpenAgentCanvas,
    OpenGitCanvas,
    OpenCommandTimeline,
    RevealInCanvas { semantic_id: SemanticId },
    ExportCanvasSvg,
    ExportCanvasMermaid,
    SetView { view: CanvasView },
    SetMode { mode: CanvasMode },
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanvasMode {
    Explain,
    #[default]
    Operate,
    Agent,
}

impl CanvasCommand {
    /// Stable identifier used by the WishUI command palette registration.
    pub fn id(&self) -> &'static str {
        match self {
            CanvasCommand::OpenRepoCanvas => "wish.canvas.open_repo",
            CanvasCommand::OpenAgentCanvas => "wish.canvas.open_agent",
            CanvasCommand::OpenGitCanvas => "wish.canvas.open_git",
            CanvasCommand::OpenCommandTimeline => "wish.canvas.open_command_timeline",
            CanvasCommand::RevealInCanvas { .. } => "wish.canvas.reveal",
            CanvasCommand::ExportCanvasSvg => "wish.canvas.export_svg",
            CanvasCommand::ExportCanvasMermaid => "wish.canvas.export_mermaid",
            CanvasCommand::SetView { .. } => "wish.canvas.set_view",
            CanvasCommand::SetMode { .. } => "wish.canvas.set_mode",
        }
    }

    /// Human-readable label for the palette.
    pub fn label(&self) -> &'static str {
        match self {
            CanvasCommand::OpenRepoCanvas => "Open Repo Canvas",
            CanvasCommand::OpenAgentCanvas => "Open Agent Canvas",
            CanvasCommand::OpenGitCanvas => "Open Git Canvas",
            CanvasCommand::OpenCommandTimeline => "Open Command Timeline",
            CanvasCommand::RevealInCanvas { .. } => "Reveal in Canvas",
            CanvasCommand::ExportCanvasSvg => "Export Canvas as SVG",
            CanvasCommand::ExportCanvasMermaid => "Export Canvas as Mermaid",
            CanvasCommand::SetView { .. } => "Set Canvas View",
            CanvasCommand::SetMode { .. } => "Set Canvas Mode",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_stable() {
        assert_eq!(CanvasCommand::OpenRepoCanvas.id(), "wish.canvas.open_repo");
        assert_eq!(
            CanvasCommand::ExportCanvasMermaid.id(),
            "wish.canvas.export_mermaid"
        );
    }
}
