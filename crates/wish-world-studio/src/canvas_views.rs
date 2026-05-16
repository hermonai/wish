//! World → Canvas projections.
//!
//! Lets any tool (the CLI, an integration test, the future Canvas
//! pane) render a `WishWorld` as a layered Canvas without owning the
//! projection logic itself.

use wish_canvas_core::{
    layout,
    types::{Canvas, CanvasEdge, CanvasNode, CanvasNodeKind, EdgeKind, LayoutMode, Rect},
};
use wish_world_model::{EntityKind, Realm, SemanticId, WishWorld};

/// Project a `WishWorld` as a force-directed Canvas. Every entity
/// becomes a node; the world id becomes a root node; every entity is
/// linked to the root by a `Mentions` edge.
pub fn world_to_canvas(world: &WishWorld) -> Canvas {
    let mut canvas = Canvas::new();
    canvas.layout = LayoutMode::ForceDirected;
    canvas.world_ref = Some(world.id.clone());
    let bounds = Rect {
        x: 0.0,
        y: 0.0,
        w: 140.0,
        h: 36.0,
    };

    // Root node for the world itself.
    let world_sid = SemanticId::new(Realm::World, "world", &world.id);
    let root_node = CanvasNode::new(
        world_sid.clone(),
        &world.name,
        CanvasNodeKind::Custom("world".into()),
        bounds,
    );
    let root_id = root_node.id;
    canvas.upsert_node(root_node);

    for entity in world.entities.values() {
        let kind = match entity.kind {
            EntityKind::File => CanvasNodeKind::File,
            EntityKind::Function => CanvasNodeKind::Function,
            EntityKind::Crate => CanvasNodeKind::Crate,
            EntityKind::Package => CanvasNodeKind::Package,
            EntityKind::Module => CanvasNodeKind::Module,
            EntityKind::Service => CanvasNodeKind::Service,
            EntityKind::Agent => CanvasNodeKind::Agent,
            EntityKind::ToolCall => CanvasNodeKind::ToolCall,
            EntityKind::Test => CanvasNodeKind::Test,
            EntityKind::Commit => CanvasNodeKind::Commit,
            EntityKind::Diff => CanvasNodeKind::Diff,
            EntityKind::TerminalBlock => CanvasNodeKind::TerminalBlock,
            EntityKind::DocumentSection => CanvasNodeKind::DocumentSection,
            EntityKind::Npc => CanvasNodeKind::Npc,
            EntityKind::Quest => CanvasNodeKind::Quest,
            EntityKind::SacredArchitecture => CanvasNodeKind::Custom("sacred_architecture".into()),
            EntityKind::Asset => CanvasNodeKind::Custom("asset".into()),
            EntityKind::Custom(ref s) => CanvasNodeKind::Custom(s.clone()),
        };
        let node = CanvasNode::new(entity.id.clone(), &entity.display_name, kind, bounds);
        let node_id = node.id;
        canvas.upsert_node(node);
        canvas.upsert_edge(CanvasEdge::new(root_id, node_id, EdgeKind::Mentions));
    }

    for agent in world.agents.values() {
        let node = CanvasNode::new(
            agent.id.clone(),
            &agent.display_name,
            CanvasNodeKind::Agent,
            bounds,
        );
        let node_id = node.id;
        canvas.upsert_node(node);
        canvas.upsert_edge(CanvasEdge::new(root_id, node_id, EdgeKind::Spawned));
    }

    layout::run(&mut canvas);
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;
    use wish_world_model::{EntityKind, Realm, WishWorld, WorldEntity, WorldKind};

    #[test]
    fn world_to_canvas_includes_root_and_each_entity() {
        let mut w = WishWorld::new("test", WorldKind::EducationWorld);
        w.upsert_entity(WorldEntity::stub(
            SemanticId::new(Realm::Npc, "npc", "liu"),
            "Liu",
            EntityKind::Npc,
        ));
        w.upsert_entity(WorldEntity::stub(
            SemanticId::new(Realm::Scene, "sacred_architecture", "temple"),
            "Temple",
            EntityKind::SacredArchitecture,
        ));
        let c = world_to_canvas(&w);
        // 1 root + 2 entities.
        assert_eq!(c.nodes.len(), 3);
        // 2 edges from root.
        assert_eq!(c.edges.len(), 2);
        assert_eq!(c.world_ref.as_deref(), Some(w.id.as_str()));
    }
}
